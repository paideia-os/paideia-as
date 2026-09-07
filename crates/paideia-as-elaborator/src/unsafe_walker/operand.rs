//! Top-level operand-parse dispatch: identifiers (register / binding / label / symbol),
//! immediates, memory refs, and deref forms.
//! Split out of `unsafe_walker.rs` (paideia-as #1403).

use std::collections::HashMap;

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::Span;
use paideia_as_ir::instruction::{Mnemonic, Operand};
use paideia_as_ir::record_layout::{RecordLayout, RecordTypeId};

use crate::LocalBindingTable;

use super::immediate::parse_immediate_from_literal;
use super::memory::{parse_deref_operand, parse_memory_from_memref};
use super::register::{get_register_name, parse_register_from_ident};
use super::symbol_ref;
use super::symbol_ref::{parse_symbol_ref_from_ident, supports_label_ref, supports_symbol_ref};

/// Error type for operand parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperandError {
    /// Unknown register name.
    UnknownRegister(String, Span),
    /// Malformed operand (e.g., invalid memory reference).
    MalformedOperand(Span),
    /// Unresolved field offset in record layout table (Phase 6 m3-005).
    UnresolvedFieldOffset(Span),
}

/// Parse an operand from an AST node.
///
/// Handles multiple operand shapes:
/// 1. Register operands (Ident nodes representing register names)
/// 2. Immediate operands (integer literals)
/// 3. Memory operands (OperandMemoryRef nodes with SIB addressing)
/// 4. Symbol references (bare identifiers in call/jmp position) — Phase 6 m4-005
///
/// # Arguments
///
/// * `ast` - The AST arena
/// * `operand_node` - The NodeId of the operand node
/// * `source_map` - The source map for resolving file content from spans
/// * `record_layouts` - Record layout table for field offset resolution
/// * `mnemonic` - The resolved Mnemonic enum to determine if SymbolRef is supported
///
/// # Returns
///
/// `Ok(Operand)` on successful parsing, `Err(OperandError)` on failure.
///
/// # Examples
///
/// ```ignore
/// // Register: rax → Operand::Reg(abi::RAX)
/// // Register: rdi → Operand::Reg(abi::RDI)
/// // Immediate: 0x12345678 → Operand::Imm64(0x12345678)
/// // Memory: [rdi + 8] → Operand::MemSib {
/// //     base: abi::RDI, index: None, scale: Scale::X1, disp: 8
/// // }
/// // Symbol (in call): call cap_alloc → Operand::SymbolRef {
/// //     name: "cap_alloc", addend: 0
/// // }
/// ```
pub fn parse_operand_from_ast(
    ast: &AstArena,
    operand_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
    record_layouts: &HashMap<RecordTypeId, RecordLayout>,
    mnemonic: Mnemonic,
    local_bindings: &LocalBindingTable,
    labels: &HashMap<String, u32>,
) -> Result<Operand, OperandError> {
    let node = ast.get(operand_node).ok_or(OperandError::MalformedOperand(
        ast.get(operand_node).map(|n| n.span).unwrap_or_else(|| {
            paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
        }),
    ))?;

    match node.kind {
        NodeKind::Ident => {
            // Try to parse as register name first
            match parse_register_from_ident(ast, operand_node, source_map) {
                Ok(op) => Ok(op),
                Err(_) => {
                    // Not a register: check if it's a local binding (Phase 7 m2-003)
                    if let Some(name) = get_register_name(ast, operand_node, source_map) {
                        if local_bindings.get(&name).is_some() {
                            // This is a local binding: emit Operand::Var for later resolution
                            return Ok(Operand::Var { name });
                        }

                        // Issue #900: Check if it's a local label reference before symbol fallback
                        if supports_label_ref(mnemonic) && labels.contains_key(&name) {
                            return Ok(Operand::LabelRef { name, addend: 0 });
                        }
                    }

                    // Not a local binding or label: check if mnemonic supports symbol references
                    if supports_symbol_ref(mnemonic) {
                        // This is a bare identifier symbol reference (Phase 6 m4-005)
                        parse_symbol_ref_from_ident(ast, operand_node, source_map)
                    } else {
                        // Mnemonic doesn't support symbol references: error
                        Err(OperandError::MalformedOperand(node.span))
                    }
                }
            }
        }
        NodeKind::OperandRegister => {
            // Register operand from parsed instruction: extract the register reference
            match ast.expr_data(operand_node) {
                Some(ExprData::OperandRegister { reg }) => {
                    // Try to parse as register name first
                    match parse_register_from_ident(ast, *reg, source_map) {
                        Ok(op) => Ok(op),
                        Err(_) => {
                            // Not a register: check if it's a local binding (Phase 7 m2-003)
                            if let Some(name) = get_register_name(ast, *reg, source_map) {
                                if local_bindings.get(&name).is_some() {
                                    // This is a local binding: emit Operand::Var for later resolution
                                    return Ok(Operand::Var { name });
                                }

                                // Issue #900: Check if it's a local label reference before symbol fallback
                                if supports_label_ref(mnemonic) && labels.contains_key(&name) {
                                    return Ok(Operand::LabelRef { name, addend: 0 });
                                }
                            }

                            // Not a local binding or label: check if mnemonic supports symbol references
                            if supports_symbol_ref(mnemonic) {
                                // This is a bare identifier symbol reference (Phase 6 m4-005)
                                parse_symbol_ref_from_ident(ast, *reg, source_map)
                            } else {
                                // Mnemonic doesn't support symbol references: error
                                Err(OperandError::MalformedOperand(node.span))
                            }
                        }
                    }
                }
                _ => Err(OperandError::MalformedOperand(node.span)),
            }
        }
        NodeKind::OperandImmediate => {
            // PA10-006i: Immediate operand from parsed instruction (e.g., `mov al, 0x42`).
            // The operand is wrapped in OperandImmediate, which contains an inner expression.
            // Unwrap and recurse to parse the inner expression.
            match ast.expr_data(operand_node) {
                Some(ExprData::OperandImmediate { expr }) => {
                    // Recursively parse the inner expression as an operand
                    parse_operand_from_ast(
                        ast,
                        *expr,
                        source_map,
                        record_layouts,
                        mnemonic,
                        local_bindings,
                        labels,
                    )
                }
                _ => Err(OperandError::MalformedOperand(node.span)),
            }
        }
        NodeKind::ExprPath => {
            // Issue #1319: a path expression in operand position (typically
            // reached via OperandImmediate's recursive unwrap when the
            // parser sees `call Module::function`). Mirrors the Ident case:
            // try register first, then local binding, then label, then
            // symbol_ref. For multi-segment paths the effective name is
            // the last segment (`try_extract_symbol_name` handles that);
            // for single-segment paths the behavior is unchanged.
            match parse_register_from_ident(ast, operand_node, source_map) {
                Ok(op) => Ok(op),
                Err(_) => {
                    // Not a register: fall through to symbol-shaped resolution.
                    if let Some(name) =
                        symbol_ref::extract_symbol_name_for_operand(ast, operand_node, source_map)
                    {
                        if local_bindings.get(&name).is_some() {
                            return Ok(Operand::Var { name });
                        }
                        if supports_label_ref(mnemonic) && labels.contains_key(&name) {
                            return Ok(Operand::LabelRef { name, addend: 0 });
                        }
                    }

                    if supports_symbol_ref(mnemonic) {
                        parse_symbol_ref_from_ident(ast, operand_node, source_map)
                    } else {
                        Err(OperandError::MalformedOperand(node.span))
                    }
                }
            }
        }
        NodeKind::ExprLiteral => {
            // Immediate operand: extract integer literal
            parse_immediate_from_literal(ast, operand_node, source_map)
        }
        NodeKind::OperandMemoryRef => {
            // Memory operand: parse memory reference with SIB addressing
            parse_memory_from_memref(ast, operand_node, source_map)
        }
        NodeKind::ExprDeref => {
            // Dereference operand: could be *p or *p.field (Phase 6 m3-005)
            // Delegate to deref-specific handler
            parse_deref_operand(ast, operand_node, source_map, record_layouts)
        }
        _ => Err(OperandError::MalformedOperand(node.span)),
    }
}
