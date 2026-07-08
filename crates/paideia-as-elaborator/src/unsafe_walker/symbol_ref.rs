//! Symbol-reference resolution: bare identifiers, `[rip + sym]`, symbol+offset sums, and support predicates.
//! Split out of `unsafe_walker.rs` (2026-07-08).

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_ir::instruction::{Mnemonic, Operand};

use super::OperandError;
use super::immediate::try_extract_integer_literal;
use super::memory::get_infix_op_name;
use super::register::{get_register_name, is_rip_identifier, register_name_to_regid};

/// Check if a mnemonic supports bare-identifier symbol references.
///
/// Phase 6 m4-005: Only `call` and `jmp` (conditional and unconditional)
/// mnemonics support symbol references in operand position.
/// PA10-006h: Also `ljmp`/`farjmp` for far-jump target offsets.
/// PA10-006j: Also `lgdt` and `lidt` for RIP-relative memory operands with symbols.
pub(super) fn supports_symbol_ref(mnemonic: Mnemonic) -> bool {
    matches!(
        mnemonic,
        Mnemonic::Call
            | Mnemonic::Jmp
            | Mnemonic::Jcc(_)
            | Mnemonic::Mov
            | Mnemonic::Lea
            | Mnemonic::FarJmp
            | Mnemonic::Lgdt
            | Mnemonic::Lidt
    )
}

/// Check if a mnemonic supports label references (local labels within unsafe blocks).
///
/// Issue #900: Local labels in unsafe blocks should parse as LabelRef (not SymbolRef)
/// when they are defined within the same unsafe block and used in branch mnemonics.
/// Only Jcc (conditional jump), Jmp (unconditional jump), and Call support label references.
pub(super) fn supports_label_ref(mnemonic: Mnemonic) -> bool {
    matches!(mnemonic, Mnemonic::Jcc(_) | Mnemonic::Jmp | Mnemonic::Call)
}

/// Parse a symbol reference from a bare identifier (Phase 6 m4-005, Phase 6 m5-004).
///
/// Returns `Operand::SymbolRef { name, addend: 0 }` for bare-identifier
/// symbols used in call/jmp/mov/lea position. These are resolved at link time
/// with PC-relative 32-bit relocations. Phase 6 m5-004 extends support to
/// mov/lea for .bss symbol references (e.g., `mov rax, cap_table`).
pub(super) fn parse_symbol_ref_from_ident(
    ast: &AstArena,
    ident_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<Operand, OperandError> {
    let span = ast.get(ident_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // Extract the symbol name from the source
    let symbol_name = match get_register_name(ast, ident_node, source_map) {
        Some(name) => name,
        None => return Err(OperandError::MalformedOperand(span)),
    };

    Ok(Operand::SymbolRef {
        name: symbol_name,
        addend: 0,
    })
}

/// Helper function to extract symbol name from Ident or single-segment ExprPath nodes.
/// Returns None if the node is not a symbol (e.g., if it's a register or "rip").
fn try_extract_symbol_name(
    ast: &AstArena,
    node_id: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Option<String> {
    let node = ast.get(node_id)?;

    match node.kind {
        NodeKind::Ident => {
            let name = get_register_name(ast, node_id, source_map)?;
            // Check if it's a known register or special register (if so, not a symbol)
            if register_name_to_regid(&name).is_some() || name == "rip" {
                return None;
            }
            Some(name)
        }
        NodeKind::ExprPath => {
            match ast.expr_data(node_id) {
                Some(ExprData::Path { segments }) if segments.len() == 1 => {
                    let name = get_register_name(ast, segments[0], source_map)?;
                    // Check if it's a known register or special register (if so, not a symbol)
                    if register_name_to_regid(&name).is_some() || name == "rip" {
                        return None;
                    }
                    Some(name)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Try to parse a memory operand as a symbol reference (bare symbol or [rip + symbol]).
///
/// PA10-006c: Support both `[symbol]` and `[rip + symbol]` syntax for RIP-relative addressing.
/// Returns Ok(SymbolRef) if successful, Err if not a symbol memory operand.
pub(super) fn try_parse_symbol_memory(
    ast: &AstArena,
    addr_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<Operand, OperandError> {
    let span = ast.get(addr_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    let node = ast
        .get(addr_node)
        .ok_or(OperandError::MalformedOperand(span))?;

    match node.kind {
        // Bare symbol: [symbol_name] → MemRipRelSym (bracketed, not bare SymbolRef)
        NodeKind::Ident => {
            let name = try_extract_symbol_name(ast, addr_node, source_map)
                .ok_or(OperandError::MalformedOperand(span))?;
            Ok(Operand::MemRipRelSym { name, addend: 0 })
        }
        // Bare symbol via path: [symbol_name] (single-segment path) → MemRipRelSym
        NodeKind::ExprPath => {
            let name = try_extract_symbol_name(ast, addr_node, source_map)
                .ok_or(OperandError::MalformedOperand(span))?;
            Ok(Operand::MemRipRelSym { name, addend: 0 })
        }
        // [rip + symbol] or [symbol ± IntLit] forms: ExprInfix → MemRipRelSym
        NodeKind::ExprInfix => {
            if let Some((name, addend)) = try_extract_symbol_sum(ast, addr_node, source_map) {
                return Ok(Operand::MemRipRelSym { name, addend });
            }
            Err(OperandError::MalformedOperand(span))
        }
        _ => Err(OperandError::MalformedOperand(span)),
    }
}

/// Attempt to decompose a memory-address expression as a sum of terms and
/// return `(symbol_name, addend)` if it consists of:
///   * exactly one symbol identifier,
///   * at most one occurrence of the pseudo-register `rip`,
///   * zero or more integer literals combined with `+` / `-`.
///
/// Handles all associative and commuted orderings of these terms — e.g.
/// `[rip + sym + N]`, `[rip + sym - N]`, `[rip + (sym + N)]`, `[N + rip + sym]`.
/// Returns `None` if any other construct appears (register other than `rip`,
/// second symbol, non-`+/-` operator, etc.) or the addend overflows `i32`.
fn try_extract_symbol_sum(
    ast: &AstArena,
    node_id: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Option<(String, i32)> {
    struct Acc {
        name: Option<String>,
        addend: i32,
        rip_seen: bool,
    }

    fn walk(
        ast: &AstArena,
        node_id: NodeId,
        sign: i32,
        acc: &mut Acc,
        source_map: &paideia_as_diagnostics::SourceMap,
    ) -> bool {
        let node = match ast.get(node_id) {
            Some(n) => n,
            None => return false,
        };
        match node.kind {
            NodeKind::ExprInfix => match ast.expr_data(node_id) {
                Some(ExprData::Infix { op, lhs, rhs }) => {
                    let rhs_sign = match get_infix_op_name(ast, *op, source_map) {
                        Some("+") => sign,
                        Some("-") => -sign,
                        _ => return false,
                    };
                    walk(ast, *lhs, sign, acc, source_map)
                        && walk(ast, *rhs, rhs_sign, acc, source_map)
                }
                _ => false,
            },
            NodeKind::Ident | NodeKind::ExprPath => {
                if is_rip_identifier(ast, node_id, source_map) {
                    if acc.rip_seen {
                        return false;
                    }
                    acc.rip_seen = true;
                    return true;
                }
                match try_extract_symbol_name(ast, node_id, source_map) {
                    Some(name) => {
                        if acc.name.is_some() {
                            return false;
                        }
                        acc.name = Some(name);
                        true
                    }
                    None => false,
                }
            }
            NodeKind::ExprLiteral => {
                let v = match try_extract_integer_literal(ast, node_id, source_map) {
                    Some(v) => v,
                    None => return false,
                };
                let signed = (v as i64).checked_mul(sign as i64);
                let signed_i32 = match signed {
                    Some(s) if (i32::MIN as i64..=i32::MAX as i64).contains(&s) => s as i32,
                    _ => return false,
                };
                match acc.addend.checked_add(signed_i32) {
                    Some(sum) => {
                        acc.addend = sum;
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        }
    }

    let mut acc = Acc { name: None, addend: 0, rip_seen: false };
    if !walk(ast, node_id, 1, &mut acc, source_map) {
        return None;
    }
    acc.name.map(|n| (n, acc.addend))
}
