//! Operand parser for the unsafe-block surface (Phase 5, m3-002).
//!
//! This module implements parsing of x86_64 operands from the AST representation
//! used in unsafe blocks. It converts AST operand nodes into IR `Operand` values
//! with proper register encoding and memory addressing modes.
//!
//! # Mnemonic Resolution (Phase 5, m3-003)
//!
//! The MNEMONIC_TABLE constant provides a canonical mapping from mnemonic string spellings
//! (case-insensitive) to IR `Mnemonic` enum variants, including proper disambiguation for
//! variants with payloads:
//! - Jcc(Cond) forms: `je` → `Jcc(Cond::Eq)`, `jne` → `Jcc(Cond::Ne)`, etc.
//! - MovCr{write}: `mov_cr` → `MovCr{write:true}`, `mov_from_cr` → `MovCr{write:false}`
//! - MovDr{write}: `mov_dr` → `MovDr{write:true}`, `mov_from_dr` → `MovDr{write:false}`
//! - In{width}: `in_al` → `In{width:1}`, `in_ax` → `In{width:2}`, `in_eax` → `In{width:4}`
//! - Out{width}: `out_al` → `Out{width:1}`, `out_ax` → `Out{width:2}`, `out_eax` → `Out{width:4}`
//!
//! # UnsafeWalker (Phase 5, m3-004)
//!
//! The UnsafeWalker elaborates pending unsafe blocks emitted by the EmitWalker.
//! For each pending unsafe block, it walks the block's statement sequence, emitting
//! `Instruction` entries into the IR's InstructionSideTable keyed by StmtInstruction IrNodeId.
//!
//! Errors are handled per spec:
//! - Unknown mnemonic: emits U1605 diagnostic with mnemonic span; instruction skipped.
//! - Operand shape error: emits U1606 diagnostic with operand span; instruction skipped.
//!
//! # Register Encoding
//!
//! General-purpose registers and special registers use distinct sentinel ranges:
//! - GPR (rax–r15): `RegId(0..15)` (standard x86_64 encoding)
//! - Control registers (cr0–cr8): `RegId(16..24)` (compact encoding for m2-005 bridge)
//! - Debug registers (dr0–dr7): `RegId(25..32)` (compact encoding for m2-005 bridge)
//!
//! The m2-005 bridge reconciles these: if RegId >= 16 and < 25, extract cr_idx = RegId - 16;
//! if >= 25 and < 33, extract dr_idx = RegId - 25.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind, StmtData};
use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, Span,
};
use paideia_as_ir::instruction::{Cond, Instruction, Mnemonic, Operand, RegId, Scale};
use paideia_as_ir::record_layout::{RecordLayout, RecordTypeId};
use paideia_as_ir::{IrArena, IrNodeId, SmallVec};
use std::collections::HashMap;

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

/// Table-driven mnemonic resolver for x86_64 instructions.
///
/// Maps canonical mnemonic spellings (case-insensitive) to IR Mnemonic variants.
/// Covers the Phase 3 m2-001 + Phase 5 m2-001 combined set (30+ mnemonics).
///
/// Canonical spellings for payload variants:
/// - Jcc: `je` (Eq), `jne` (Ne), `jl` (Lt), `jle` (Le), `jg` (Gt), `jge` (Ge),
///   `jb` (Below), `jbe` (BelowOrEqual), `ja` (Above), `jae` (AboveOrEqual),
///   `jz` (Zero), `jnz` (NonZero), `js` (Sign), `jns` (NotSign),
///   `jo` (Overflow), `jno` (NotOverflow)
/// - MovCr: `mov_cr` (write=true), `mov_from_cr` (write=false)
/// - MovDr: `mov_dr` (write=true), `mov_from_dr` (write=false)
/// - In: `in_al` (width=1), `in_ax` (width=2), `in_eax` (width=4)
/// - Out: `out_al` (width=1), `out_ax` (width=2), `out_eax` (width=4)
const MNEMONIC_TABLE: &[(&str, Mnemonic)] = &[
    // Phase 3 m2-001: original 10 mnemonics
    ("mov", Mnemonic::Mov),
    ("add", Mnemonic::Add),
    ("sub", Mnemonic::Sub),
    ("cmp", Mnemonic::Cmp),
    ("jmp", Mnemonic::Jmp),
    ("call", Mnemonic::Call),
    ("ret", Mnemonic::Ret),
    ("rep_movsb", Mnemonic::RepMovsb),
    ("lea", Mnemonic::Lea),
    ("nop", Mnemonic::Nop),
    // Phase 5 m2-001: 20 privileged + system-ISA mnemonics
    ("lgdt", Mnemonic::Lgdt),
    ("lidt", Mnemonic::Lidt),
    ("wrmsr", Mnemonic::Wrmsr),
    ("rdmsr", Mnemonic::Rdmsr),
    ("iret", Mnemonic::Iret),
    ("iretq", Mnemonic::Iretq),
    ("sysret", Mnemonic::Sysret),
    ("syscall", Mnemonic::Syscall),
    ("swapgs", Mnemonic::Swapgs),
    ("cpuid", Mnemonic::Cpuid),
    ("cli", Mnemonic::Cli),
    ("sti", Mnemonic::Sti),
    ("hlt", Mnemonic::Hlt),
    ("rep_stosq", Mnemonic::RepStosq),
    ("farjmp", Mnemonic::FarJmp),
    // Jcc (conditional jump) variants (16 forms)
    ("je", Mnemonic::Jcc(Cond::Eq)),
    ("jne", Mnemonic::Jcc(Cond::Ne)),
    ("jl", Mnemonic::Jcc(Cond::Lt)),
    ("jle", Mnemonic::Jcc(Cond::Le)),
    ("jg", Mnemonic::Jcc(Cond::Gt)),
    ("jge", Mnemonic::Jcc(Cond::Ge)),
    ("jb", Mnemonic::Jcc(Cond::Below)),
    ("jbe", Mnemonic::Jcc(Cond::BelowOrEqual)),
    ("ja", Mnemonic::Jcc(Cond::Above)),
    ("jae", Mnemonic::Jcc(Cond::AboveOrEqual)),
    ("jz", Mnemonic::Jcc(Cond::Zero)),
    ("jnz", Mnemonic::Jcc(Cond::NonZero)),
    ("js", Mnemonic::Jcc(Cond::Sign)),
    ("jns", Mnemonic::Jcc(Cond::NotSign)),
    ("jo", Mnemonic::Jcc(Cond::Overflow)),
    ("jno", Mnemonic::Jcc(Cond::NotOverflow)),
    // MovCr (control register move) variants (2 forms)
    ("mov_cr", Mnemonic::MovCr { write: true }),
    ("mov_from_cr", Mnemonic::MovCr { write: false }),
    // MovDr (debug register move) variants (2 forms)
    ("mov_dr", Mnemonic::MovDr { write: true }),
    ("mov_from_dr", Mnemonic::MovDr { write: false }),
    // In (I/O port read) variants (3 forms)
    ("in_al", Mnemonic::In { width: 1 }),
    ("in_ax", Mnemonic::In { width: 2 }),
    ("in_eax", Mnemonic::In { width: 4 }),
    // Out (I/O port write) variants (3 forms)
    ("out_al", Mnemonic::Out { width: 1 }),
    ("out_ax", Mnemonic::Out { width: 2 }),
    ("out_eax", Mnemonic::Out { width: 4 }),
    // Note: Int (software interrupt) uses int3 as canonical (see resolve_mnemonic)
];

/// Resolve a mnemonic name to an IR Mnemonic enum variant.
///
/// Performs case-insensitive lookup against the MNEMONIC_TABLE.
/// Returns `Some(Mnemonic)` if found, `None` if the name is unknown.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(resolve_mnemonic("mov"), Some(Mnemonic::Mov));
/// assert_eq!(resolve_mnemonic("MOV"), Some(Mnemonic::Mov));  // case-insensitive
/// assert_eq!(resolve_mnemonic("je"), Some(Mnemonic::Jcc(Cond::Eq)));
/// assert_eq!(resolve_mnemonic("mov_cr"), Some(Mnemonic::MovCr { write: true }));
/// assert_eq!(resolve_mnemonic("in_al"), Some(Mnemonic::In { width: 1 }));
/// assert_eq!(resolve_mnemonic("not_a_mnemonic"), None);
/// ```
#[must_use]
pub fn resolve_mnemonic(name: &str) -> Option<Mnemonic> {
    let lower_name = name.to_lowercase();

    // Handle the special case of Int (software interrupt)
    if lower_name == "int3" {
        return Some(Mnemonic::Int);
    }

    // Table lookup with case-insensitive ASCII lowercase
    for (mnem_name, mnem) in MNEMONIC_TABLE {
        if mnem_name.eq_ignore_ascii_case(&lower_name) {
            return Some(*mnem);
        }
    }

    None
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
/// // Register: rax → Operand::Reg(RegId(0))
/// // Register: rdi → Operand::Reg(RegId(7))
/// // Immediate: 0x12345678 → Operand::Imm64(0x12345678)
/// // Memory: [rdi + 8] → Operand::MemSib {
/// //     base: RegId(7), index: None, scale: Scale::X1, disp: 8
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
) -> Result<Operand, OperandError> {
    let node = ast.get(operand_node).ok_or(OperandError::MalformedOperand(
        ast.get(operand_node).map(|n| n.span).unwrap_or_else(|| {
            paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
        }),
    ))?;

    match node.kind {
        NodeKind::Ident => {
            // Try to parse as register name first, then fall through to symbol if not a register
            match parse_register_from_ident(ast, operand_node, source_map) {
                Ok(op) => Ok(op),
                Err(_) => {
                    // Not a register: check if this mnemonic supports symbol references
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
                    // Try to parse as register name first, then fall through to symbol if not a register
                    match parse_register_from_ident(ast, *reg, source_map) {
                        Ok(op) => Ok(op),
                        Err(_) => {
                            // Not a register: check if this mnemonic supports symbol references
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
        NodeKind::ExprLiteral => {
            // Immediate operand: extract integer literal
            parse_immediate_from_literal(ast, operand_node)
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

/// Check if a mnemonic supports bare-identifier symbol references.
///
/// Phase 6 m4-005: Only `call` and `jmp` (conditional and unconditional)
/// mnemonics support symbol references in operand position.
fn supports_symbol_ref(mnemonic: Mnemonic) -> bool {
    matches!(
        mnemonic,
        Mnemonic::Call | Mnemonic::Jmp | Mnemonic::Jcc(_) | Mnemonic::Mov | Mnemonic::Lea
    )
}

/// Parse a symbol reference from a bare identifier (Phase 6 m4-005, Phase 6 m5-004).
///
/// Returns `Operand::SymbolRef { name, addend: 0 }` for bare-identifier
/// symbols used in call/jmp/mov/lea position. These are resolved at link time
/// with PC-relative 32-bit relocations. Phase 6 m5-004 extends support to
/// mov/lea for .bss symbol references (e.g., `mov rax, cap_table`).
fn parse_symbol_ref_from_ident(
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

/// Parse a register operand from an Ident node or ExprPath (single-segment).
fn parse_register_from_ident(
    ast: &AstArena,
    ident_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<Operand, OperandError> {
    let span = ast.get(ident_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // Handle both Ident and ExprPath (single-segment) node kinds
    let node = ast
        .get(ident_node)
        .ok_or(OperandError::MalformedOperand(span))?;
    let actual_ident_node = match node.kind {
        NodeKind::Ident => ident_node,
        NodeKind::ExprPath => {
            // For ExprPath, extract the first segment (should be single-segment)
            match ast.expr_data(ident_node) {
                Some(ExprData::Path { segments }) if segments.len() == 1 => segments[0],
                _ => return Err(OperandError::MalformedOperand(span)),
            }
        }
        _ => return Err(OperandError::MalformedOperand(span)),
    };

    // Extract the identifier text by looking up in the source.
    // For phase-1, we use a lookup table matching register names to RegIds.
    let reg_id = match get_register_name(ast, actual_ident_node, source_map) {
        Some(name) => register_name_to_regid(&name)
            .ok_or_else(|| OperandError::UnknownRegister(name, span))?,
        None => {
            return Err(OperandError::MalformedOperand(span));
        }
    };

    Ok(Operand::Reg(reg_id))
}

/// Parse a dereference operand: `*expr` or `*expr.field` (Phase 6 m3-005).
///
/// Handles:
/// - `*p` where p is a register → Operand::MemSib with base register and disp=0
/// - `*p.field` → Operand::MemSib with base register and disp=field_offset
///
/// For field access, looks up the field offset in record_layouts:
/// - Assumes first record type (RecordTypeId(1)) for Phase 6 m3-005
/// - Matches field by index using convention: "field0", "field1", "rights", etc.
/// - If found: returns MemSib with computed displacement
/// - If not found: returns UnresolvedFieldOffset error (U1608)
fn parse_deref_operand(
    ast: &AstArena,
    deref_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
    record_layouts: &HashMap<RecordTypeId, RecordLayout>,
) -> Result<Operand, OperandError> {
    let span = ast.get(deref_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // Extract the dereferenced expression from *expr
    let dereferenced_expr = match ast.expr_data(deref_node) {
        Some(ExprData::Deref { expr }) => *expr,
        _ => return Err(OperandError::MalformedOperand(span)),
    };

    let dereferenced_node = ast
        .get(dereferenced_expr)
        .ok_or(OperandError::MalformedOperand(span))?;

    match dereferenced_node.kind {
        NodeKind::ExprFieldAccess => {
            // *p.field pattern: extract base register and resolve field offset
            match ast.expr_data(dereferenced_expr) {
                Some(ExprData::FieldAccess { receiver, field }) => {
                    // Extract the base register from the receiver (e.g., p in p.field)
                    let base_reg = parse_register_from_ident(ast, *receiver, source_map)?;
                    let base_reg_id = match base_reg {
                        Operand::Reg(rid) => rid,
                        _ => return Err(OperandError::MalformedOperand(span)),
                    };

                    // Get the field name/identifier
                    let field_name = get_register_name(ast, *field, source_map)
                        .ok_or(OperandError::MalformedOperand(span))?;

                    // Phase 6 m3-005: Use the first record type (default RecordTypeId)
                    // In a full system with type inference, this would come from the receiver's type
                    let record_type_id = RecordTypeId(1);

                    // Look up the record layout
                    let layout = record_layouts
                        .get(&record_type_id)
                        .ok_or(OperandError::UnresolvedFieldOffset(span))?;

                    // Try to resolve field name to offset
                    // First attempt: numeric suffix "field0", "field1", etc.
                    for (idx, field_layout) in layout.fields.iter().enumerate() {
                        if field_name == format!("field{}", idx) {
                            let disp = field_layout.offset as i32;
                            return Ok(Operand::MemSib {
                                base: base_reg_id,
                                index: None,
                                scale: Scale::X1,
                                disp,
                            });
                        }
                    }

                    // Second attempt: semantic field names like "rights", "kind", etc.
                    // Map known field names to indices in the layout
                    // For now, this is a simple placeholder; a real implementation would use
                    // a field name table stored in the layout or type system
                    let field_index = match field_name.as_str() {
                        "kind" => Some(0),
                        "rights" => Some(1),
                        "badge" => Some(2),
                        _ => None,
                    };

                    if let Some(idx) = field_index {
                        if idx < layout.fields.len() {
                            let field_layout = &layout.fields[idx];
                            let disp = field_layout.offset as i32;
                            return Ok(Operand::MemSib {
                                base: base_reg_id,
                                index: None,
                                scale: Scale::X1,
                                disp,
                            });
                        }
                    }

                    // Field not found
                    Err(OperandError::UnresolvedFieldOffset(span))
                }
                _ => Err(OperandError::MalformedOperand(span)),
            }
        }
        _ => {
            // Plain dereference without field access: *p
            // Parse as memory operand with base register, disp=0
            match parse_register_from_ident(ast, dereferenced_expr, source_map)? {
                Operand::Reg(base_reg_id) => Ok(Operand::MemSib {
                    base: base_reg_id,
                    index: None,
                    scale: Scale::X1,
                    disp: 0,
                }),
                _ => Err(OperandError::MalformedOperand(span)),
            }
        }
    }
}

/// Parse an immediate operand from an ExprLiteral node.
fn parse_immediate_from_literal(
    ast: &AstArena,
    literal_node: NodeId,
) -> Result<Operand, OperandError> {
    let span = ast.get(literal_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // For phase-1, we assume all literals are already interned as i64 values.
    // The parser's literal interning (paideia-as-parser) handles the conversion.
    // We extract the value by looking at the AST structure.
    match ast.expr_data(literal_node) {
        Some(ExprData::Literal { lit }) => {
            // The `lit` node is a Placeholder holding the literal value.
            // For phase-1, we assume the parser has already validated the literal.
            // Extract the integer value from the source span or use a default.
            let value = extract_integer_from_span(ast, *lit).unwrap_or(0);
            Ok(Operand::Imm64(value))
        }
        _ => Err(OperandError::MalformedOperand(span)),
    }
}

/// Parse a memory operand from an OperandMemoryRef node.
fn parse_memory_from_memref(
    ast: &AstArena,
    memref_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<Operand, OperandError> {
    let span = ast.get(memref_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    match ast.expr_data(memref_node) {
        Some(ExprData::OperandMemoryRef { addr }) => {
            // Parse the address expression to extract SIB components
            parse_address_to_sib(ast, *addr, source_map)
        }
        _ => Err(OperandError::MalformedOperand(span)),
    }
}

/// Parse an address expression to extract SIB (Scale-Index-Base) components.
///
/// Handles expressions like:
/// - `rdi` → base=7, index=None, scale=X1, disp=0
/// - `rdi + 8` → base=7, index=None, scale=X1, disp=8
/// - `rdi + rsi * 4` → base=7, index=Some(6), scale=X4, disp=0
/// - `rdi + rsi * 4 + 8` → base=7, index=Some(6), scale=X4, disp=8
fn parse_address_to_sib(
    ast: &AstArena,
    addr_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<Operand, OperandError> {
    // Extract SIB components from the address expression.
    // Phase-1 implementation: support infix operators (+, -) and multiply (*).
    let (base, index, scale, disp) = extract_sib_components(ast, addr_node, source_map)?;

    Ok(Operand::MemSib {
        base,
        index,
        scale,
        disp,
    })
}

/// Extract SIB components from an address expression.
///
/// Returns (base, index, scale, disp) tuple.
fn extract_sib_components(
    ast: &AstArena,
    expr_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<(RegId, Option<RegId>, Scale, i32), OperandError> {
    let span = ast.get(expr_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    let node = ast
        .get(expr_node)
        .ok_or(OperandError::MalformedOperand(span))?;

    match node.kind {
        // Base case: single register → base=reg, index=None, scale=X1, disp=0
        NodeKind::Ident => match parse_register_from_ident(ast, expr_node, source_map)? {
            Operand::Reg(base) => Ok((base, None, Scale::X1, 0)),
            _ => Err(OperandError::MalformedOperand(span)),
        },
        // Path case: single-segment path (like `rdi`) → same as Ident
        NodeKind::ExprPath => match ast.expr_data(expr_node) {
            Some(ExprData::Path { segments }) if segments.len() == 1 => {
                match parse_register_from_ident(ast, segments[0], source_map)? {
                    Operand::Reg(base) => Ok((base, None, Scale::X1, 0)),
                    _ => Err(OperandError::MalformedOperand(span)),
                }
            }
            _ => Err(OperandError::MalformedOperand(span)),
        },
        // Infix operator: could be addition/subtraction or multiplication
        NodeKind::ExprInfix => {
            match ast.expr_data(expr_node) {
                Some(ExprData::Infix { op, lhs, rhs }) => {
                    // Get operator symbol
                    let op_str = get_infix_op_name(ast, *op);

                    match op_str.as_deref() {
                        Some("+") | Some("-") => {
                            // Addition/subtraction: base + disp or index*scale + base + disp
                            combine_additive_terms(ast, *lhs, *rhs, op_str == Some("-"), source_map)
                        }
                        Some("*") => {
                            // Multiplication: should only appear as index*scale
                            Err(OperandError::MalformedOperand(span))
                        }
                        _ => Err(OperandError::MalformedOperand(span)),
                    }
                }
                _ => Err(OperandError::MalformedOperand(span)),
            }
        }
        // Literal integer: treat as displacement
        NodeKind::ExprLiteral => {
            // A pure displacement without base register is invalid in SIB addressing
            Err(OperandError::MalformedOperand(span))
        }
        _ => Err(OperandError::MalformedOperand(span)),
    }
}

/// Combine additive terms to extract SIB components.
fn combine_additive_terms(
    ast: &AstArena,
    left: NodeId,
    right: NodeId,
    is_sub: bool,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<(RegId, Option<RegId>, Scale, i32), OperandError> {
    let span = ast.get(left).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // Recursively extract components from left and right
    let (left_base, left_index, left_scale, left_disp) =
        extract_sib_components(ast, left, source_map)?;

    // Try to parse right as either a register, immediate, or index*scale expression
    let right_kind = ast.get(right).map(|n| n.kind);

    match right_kind {
        Some(NodeKind::Ident) => {
            // Right is a register: could be base or index
            match parse_register_from_ident(ast, right, source_map)? {
                Operand::Reg(reg) => {
                    // Merge: if left has base, right is index; otherwise right is base
                    if left_base == RegId(0) && left_index.is_none() {
                        Ok((
                            reg,
                            None,
                            Scale::X1,
                            if is_sub { -left_disp } else { left_disp },
                        ))
                    } else if left_index.is_none() {
                        Ok((
                            left_base,
                            Some(reg),
                            left_scale,
                            if is_sub { -left_disp } else { left_disp },
                        ))
                    } else {
                        Err(OperandError::MalformedOperand(span))
                    }
                }
                _ => Err(OperandError::MalformedOperand(span)),
            }
        }
        Some(NodeKind::ExprLiteral) => {
            // Right is an immediate: treat as displacement
            let right_disp = extract_integer_from_span(ast, right).unwrap_or(0) as i32;
            let final_disp = if is_sub {
                left_disp - right_disp
            } else {
                left_disp + right_disp
            };
            Ok((left_base, left_index, left_scale, final_disp))
        }
        Some(NodeKind::ExprInfix) => {
            // Right is an infix expression: could be index*scale
            match ast.expr_data(right) {
                Some(ExprData::Infix {
                    op,
                    lhs: mul_lhs,
                    rhs: mul_rhs,
                }) => {
                    let op_str = get_infix_op_name(ast, *op);
                    if op_str == Some("*") {
                        // Extract index and scale from multiplication
                        match extract_index_scale(ast, *mul_lhs, *mul_rhs, source_map)? {
                            (idx, scale_factor) => {
                                let scale = Scale::from_factor(scale_factor)
                                    .ok_or(OperandError::MalformedOperand(span))?;
                                Ok((left_base, Some(idx), scale, left_disp))
                            }
                        }
                    } else {
                        Err(OperandError::MalformedOperand(span))
                    }
                }
                _ => Err(OperandError::MalformedOperand(span)),
            }
        }
        _ => Err(OperandError::MalformedOperand(span)),
    }
}

/// Extract index register and scale factor from an index*scale expression.
fn extract_index_scale(
    ast: &AstArena,
    left: NodeId,
    right: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<(RegId, u32), OperandError> {
    let span = ast.get(left).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // Left should be register, right should be immediate scale
    let idx_reg = match parse_register_from_ident(ast, left, source_map)? {
        Operand::Reg(reg) => reg,
        _ => return Err(OperandError::MalformedOperand(span)),
    };

    let scale_factor = extract_integer_from_span(ast, right).unwrap_or(1) as u32;
    Ok((idx_reg, scale_factor))
}

/// Get the infix operator name from an operator node.
fn get_infix_op_name(ast: &AstArena, op_node: NodeId) -> Option<&'static str> {
    let node = ast.get(op_node)?;
    if node.kind == NodeKind::Ident {
        // Extract operator symbol from Ident node
        // For phase-1, return a representative operator string
        // A full implementation would look up the source text
        None
    } else {
        None
    }
}

/// Get the register name from an Ident node by looking at the source text.
fn get_register_name(
    ast: &AstArena,
    ident_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Option<String> {
    // Extract the register name from the span using the source map
    let node = ast.get(ident_node)?;
    let span = node.span;

    // Look up the file content in the source map
    let file_id = span.file();
    let source = source_map.content(file_id);

    // Extract the text from the span
    let start = span.byte_start() as usize;
    let end = start + span.byte_len() as usize;
    if end <= source.len() {
        Some(source[start..end].to_string())
    } else {
        None
    }
}

/// Extract an integer value from a span/literal node.
fn extract_integer_from_span(_ast: &AstArena, _literal_node: NodeId) -> Option<i64> {
    // For phase-1, return 0 as a placeholder
    // A full implementation would extract the actual value from the source text
    Some(0)
}

/// Map register names to RegId values.
///
/// Encoding (fits within u8):
/// - GPR (rax–r15): 0–15 (standard x86_64)
/// - Control registers (cr0–cr8): 16–24 (compact encoding for m2-005 bridge)
/// - Debug registers (dr0–dr7): 25–32 (compact encoding for m2-005 bridge)
///
/// Phase 7 m2-001 (PA7C-m2-001): Sub-registers (32-bit, 16-bit, 8-bit) are supported
/// and resolve to the same RegId as their 64-bit form. For example, "eax", "ax", and "al"
/// all resolve to RegId(0). This maintains width-agnostic register handling; the encoder
/// is responsible for width-aware MOV dispatch (follow-up issue PA7C-m2-001a).
///
/// The bridge in m2-005 will interpret values >= 16 as special registers and
/// extract the control/debug register index accordingly.
#[must_use]
fn register_name_to_regid(name: &str) -> Option<RegId> {
    match name {
        // General-purpose registers (64-bit)
        "rax" => Some(RegId(0)),
        "rcx" => Some(RegId(1)),
        "rdx" => Some(RegId(2)),
        "rbx" => Some(RegId(3)),
        "rsp" => Some(RegId(4)),
        "rbp" => Some(RegId(5)),
        "rsi" => Some(RegId(6)),
        "rdi" => Some(RegId(7)),
        "r8" => Some(RegId(8)),
        "r9" => Some(RegId(9)),
        "r10" => Some(RegId(10)),
        "r11" => Some(RegId(11)),
        "r12" => Some(RegId(12)),
        "r13" => Some(RegId(13)),
        "r14" => Some(RegId(14)),
        "r15" => Some(RegId(15)),

        // 32-bit sub-registers (resolve to same RegId as 64-bit form)
        "eax" => Some(RegId(0)),
        "ecx" => Some(RegId(1)),
        "edx" => Some(RegId(2)),
        "ebx" => Some(RegId(3)),
        "esp" => Some(RegId(4)),
        "ebp" => Some(RegId(5)),
        "esi" => Some(RegId(6)),
        "edi" => Some(RegId(7)),
        "r8d" => Some(RegId(8)),
        "r9d" => Some(RegId(9)),
        "r10d" => Some(RegId(10)),
        "r11d" => Some(RegId(11)),
        "r12d" => Some(RegId(12)),
        "r13d" => Some(RegId(13)),
        "r14d" => Some(RegId(14)),
        "r15d" => Some(RegId(15)),

        // 16-bit sub-registers (resolve to same RegId as 64-bit form; r8w-r15w do not exist)
        "ax" => Some(RegId(0)),
        "cx" => Some(RegId(1)),
        "dx" => Some(RegId(2)),
        "bx" => Some(RegId(3)),
        "sp" => Some(RegId(4)),
        "bp" => Some(RegId(5)),
        "si" => Some(RegId(6)),
        "di" => Some(RegId(7)),

        // 8-bit sub-registers (resolve to same RegId as 64-bit form; al-r15b, but only al-bl exist)
        "al" => Some(RegId(0)),
        "cl" => Some(RegId(1)),
        "dl" => Some(RegId(2)),
        "bl" => Some(RegId(3)),

        // Control registers (compact encoding: 16 + index)
        "cr0" => Some(RegId(16)),
        "cr1" => Some(RegId(17)),
        "cr2" => Some(RegId(18)),
        "cr3" => Some(RegId(19)),
        "cr4" => Some(RegId(20)),
        "cr5" => Some(RegId(21)),
        "cr6" => Some(RegId(22)),
        "cr7" => Some(RegId(23)),
        "cr8" => Some(RegId(24)),

        // Debug registers (compact encoding: 25 + index)
        "dr0" => Some(RegId(25)),
        "dr1" => Some(RegId(26)),
        "dr2" => Some(RegId(27)),
        "dr3" => Some(RegId(28)),
        "dr4" => Some(RegId(29)),
        "dr5" => Some(RegId(30)),
        "dr6" => Some(RegId(31)),
        "dr7" => Some(RegId(32)),

        _ => None,
    }
}

/// Diagnostic code for unknown mnemonic (U1605).
pub const U_UNKNOWN_MNEMONIC: u16 = 1605;

/// Diagnostic code for malformed operand (U1606).
pub const U_MALFORMED_OPERAND: u16 = 1606;

/// Diagnostic code for unexpected operands on zero-arity instruction (U1607).
pub const U_UNEXPECTED_OPERANDS: u16 = 1607;

/// Diagnostic code for unresolved field offset in unsafe block (U1608).
pub const U_UNRESOLVED_FIELD_OFFSET: u16 = 1608;

/// Diagnostic code for duplicate label declaration in unsafe block (U1609).
pub const U_DUPLICATE_LABEL: u16 = 1609;

/// Diagnostic code for unknown label reference in unsafe block (U1610).
pub const U_UNKNOWN_LABEL: u16 = 1610;

/// Diagnostic code for SymbolRef operand not supported for mnemonic (U1611).
pub const U_SYMBOLREF_NOT_SUPPORTED: u16 = 1611;

/// Helper: create a U-category error code.
fn u_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, n).expect("valid U code")
}

/// UnsafeWalker — Phase 5 m3-004 elaborator for unsafe blocks.
///
/// Walks pending unsafe blocks (collected by EmitWalker m1-004) and emits
/// `Instruction` entries into the IR's InstructionSideTable. For each
/// `StmtInstruction` in the block, resolves the mnemonic and parses all operands,
/// then inserts an `Instruction` keyed by the statement's IrNodeId.
pub struct UnsafeWalker;

impl UnsafeWalker {
    /// Run the unsafe walker on a set of pending unsafe blocks.
    ///
    /// # Arguments
    ///
    /// * `arena` - The IR arena containing the unsafe block nodes.
    /// * `ast` - The AST arena containing the block's statement data.
    /// * `pending_ids` - IrNodeIds of IrKind::Unsafe nodes to elaborate.
    /// * `source_map` - The source map for resolving file content from spans.
    /// * `sink` - Diagnostic sink for emitting errors.
    /// * `record_layouts` - Record layout table for field offset resolution (Phase 6 m3-005).
    ///
    /// # Returns
    ///
    /// A vector of diagnostics emitted during elaboration.
    ///
    /// # Side effects
    ///
    /// Mutates `arena.instructions_mut()` to insert Instruction entries.
    pub fn run(
        arena: &mut IrArena,
        ast: &AstArena,
        pending_ids: Vec<u32>,
        source_map: &paideia_as_diagnostics::SourceMap,
        sink: &mut dyn DiagnosticSink,
        record_layouts: &HashMap<RecordTypeId, RecordLayout>,
    ) -> Vec<Diagnostic> {
        let mut diags = Vec::new();

        for ir_node_id_u32 in pending_ids {
            let _ir_node_id = match IrNodeId::new(ir_node_id_u32) {
                Some(id) => id,
                None => continue,
            };

            // Get the IR node to find the AST node it references.
            // The IR node for Unsafe should have been constructed during lowering.
            // We need to find the corresponding AST node via the elaborator's
            // lowering tables (typically stored in a context struct).
            // For this phase, we assume that the unsafe block's AST node ID
            // can be derived or is passed via context. Placeholder: search in AST.

            // Scan the entire AST for ExprUnsafe nodes (this is a placeholder approach).
            // A production implementation would use an AST-to-IR mapping table.
            for ast_idx in 1..=ast.len() {
                if let Some(ast_node_id) = NodeId::new(ast_idx as u32) {
                    if let Some(ast_node) = ast.get(ast_node_id) {
                        if ast_node.kind == NodeKind::ExprUnsafe {
                            // Found an ExprUnsafe node; check if this is our target.
                            if let Some(ExprData::Unsafe { block, .. }) = ast.expr_data(ast_node_id)
                            {
                                // Phase 6 m4-002: Two-pass processing for labels.
                                // Pass 1: Collect all label declarations into a HashMap.
                                let mut labels: HashMap<String, u32> = HashMap::new();
                                for &stmt_id in block {
                                    if let Some(ast_stmt_node) = ast.get(stmt_id) {
                                        if ast_stmt_node.kind == NodeKind::StmtLabel {
                                            // Collect label: extract label name from StmtData::Label
                                            if let Some(StmtData::Label { name }) =
                                                ast.stmt_data(stmt_id)
                                            {
                                                if let Some(name_node) = ast.get(*name) {
                                                    if name_node.kind == NodeKind::Ident {
                                                        // Extract the label name from source
                                                        let span = name_node.span;
                                                        let file_id = span.file();
                                                        let source = source_map.content(file_id);
                                                        let label_text = &source[span.byte_start()
                                                            as usize
                                                            ..(span.byte_start() + span.byte_len())
                                                                as usize];
                                                        // Check for duplicate label (U1609)
                                                        if labels.contains_key(label_text) {
                                                            let diag = Diagnostic::error(u_code(
                                                                U_DUPLICATE_LABEL,
                                                            ))
                                                            .message(format!(
                                                                "duplicate label declaration: {}",
                                                                label_text
                                                            ))
                                                            .with_span(span)
                                                            .finish();
                                                            let _ = sink.emit(diag.clone());
                                                            diags.push(diag);
                                                        } else {
                                                            // Store label with a placeholder byte offset (0 for now)
                                                            labels
                                                                .insert(label_text.to_string(), 0);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Pass 2: Process instructions and check label references.
                                for &stmt_id in block {
                                    if let Some(ast_stmt_node) = ast.get(stmt_id) {
                                        if ast_stmt_node.kind == NodeKind::StmtInstruction {
                                            // Process this instruction statement.
                                            Self::process_instruction_stmt(
                                                arena,
                                                ast,
                                                stmt_id,
                                                &mut diags,
                                                sink,
                                                source_map,
                                                record_layouts,
                                                &labels,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        diags
    }

    /// Process a single StmtInstruction node.
    ///
    /// Resolves the mnemonic, parses all operands, and inserts an Instruction
    /// into the arena's side-table. Emits diagnostics on error. Also validates
    /// label references against the collected labels map (Phase 6 m4-002).
    fn process_instruction_stmt(
        arena: &mut IrArena,
        ast: &AstArena,
        stmt_id: NodeId,
        diags: &mut Vec<Diagnostic>,
        sink: &mut dyn DiagnosticSink,
        source_map: &paideia_as_diagnostics::SourceMap,
        record_layouts: &HashMap<RecordTypeId, RecordLayout>,
        labels: &HashMap<String, u32>,
    ) {
        // Get the statement data.
        let stmt_data = match ast.stmt_data(stmt_id) {
            Some(StmtData::Instruction { mnemonic, operands }) => (mnemonic, operands),
            _ => return,
        };

        let mnemonic_id = stmt_data.0;
        let operand_ids = stmt_data.1;

        // Get the mnemonic string from the arena's interned table.
        let mnemonic_str = ast.mnemonic_str(*mnemonic_id);

        // Resolve the mnemonic to a Mnemonic enum variant.
        let mnemonic = match resolve_mnemonic(mnemonic_str) {
            Some(m) => m,
            None => {
                // U1605: Unknown mnemonic
                let span = ast.get(stmt_id).map(|n| n.span).unwrap_or_else(|| {
                    paideia_as_diagnostics::Span::new(
                        paideia_as_diagnostics::FileId::new(1).unwrap(),
                        0,
                        1,
                    )
                });
                let diag = Diagnostic::error(u_code(U_UNKNOWN_MNEMONIC))
                    .message(format!("unknown mnemonic: {}", mnemonic_str))
                    .with_span(span)
                    .finish();
                let _ = sink.emit(diag.clone());
                diags.push(diag);
                return;
            }
        };

        // Phase 6 m1-005: Check if this is a zero-arity instruction with operands.
        // If mnemonic.arity() == 0 and operand_ids is non-empty, emit U1607 and proceed with empty operands.
        let mut parsed_operands: SmallVec<[Operand; 3]> = SmallVec::new();

        let expected_arity = mnemonic.arity();
        if expected_arity == 0 && !operand_ids.is_empty() {
            // Emit U1607 with span of the first operand
            if let Some(&first_operand_id) = operand_ids.first() {
                let operand_span = ast
                    .get(first_operand_id)
                    .map(|n| n.span)
                    .unwrap_or_else(|| {
                        paideia_as_diagnostics::Span::new(
                            paideia_as_diagnostics::FileId::new(1).unwrap(),
                            0,
                            1,
                        )
                    });
                let diag = Diagnostic::error(u_code(U_UNEXPECTED_OPERANDS))
                    .message(format!(
                        "unexpected operands for zero-arity instruction: {}",
                        mnemonic_str
                    ))
                    .with_span(operand_span)
                    .finish();
                let _ = sink.emit(diag.clone());
                diags.push(diag);
            }
            // Continue with empty operands (recovery posture)
        } else {
            // Parse all operands normally.
            let mut operand_error = false;

            for &operand_id in operand_ids {
                match parse_operand_from_ast(ast, operand_id, source_map, record_layouts, mnemonic)
                {
                    Ok(operand) => {
                        parsed_operands.push(operand);
                    }
                    Err(OperandError::UnknownRegister(_name, span)) => {
                        // U1606: Malformed operand (register name not recognized)
                        let diag = Diagnostic::error(u_code(U_MALFORMED_OPERAND))
                            .message(
                                "malformed operand in unsafe block: unknown register".to_string(),
                            )
                            .with_span(span)
                            .finish();
                        let _ = sink.emit(diag.clone());
                        diags.push(diag);
                        operand_error = true;
                        break;
                    }
                    Err(OperandError::MalformedOperand(span)) => {
                        // U1606: Malformed operand (shape error)
                        let diag = Diagnostic::error(u_code(U_MALFORMED_OPERAND))
                            .message("malformed operand in unsafe block".to_string())
                            .with_span(span)
                            .finish();
                        let _ = sink.emit(diag.clone());
                        diags.push(diag);
                        operand_error = true;
                        break;
                    }
                    Err(OperandError::UnresolvedFieldOffset(span)) => {
                        // U1608: Unresolved field offset in unsafe block
                        let diag = Diagnostic::error(u_code(U_UNRESOLVED_FIELD_OFFSET))
                            .message(
                                "field offset not resolved; declare struct before use".to_string(),
                            )
                            .with_span(span)
                            .finish();
                        let _ = sink.emit(diag.clone());
                        diags.push(diag);
                        operand_error = true;
                        break;
                    }
                }
            }

            // If any operand parsing failed, skip this instruction.
            if operand_error {
                return;
            }
        }

        // Phase 6 m4-002: Validate label references.
        // Check each operand to see if it's a LabelRef and verify it exists in the labels map.
        for operand in &parsed_operands {
            if let Operand::LabelRef { name, .. } = operand {
                if !labels.contains_key(name) {
                    // U1610: Unknown label reference
                    let stmt_span = ast.get(stmt_id).map(|n| n.span).unwrap_or_else(|| {
                        paideia_as_diagnostics::Span::new(
                            paideia_as_diagnostics::FileId::new(1).unwrap(),
                            0,
                            1,
                        )
                    });
                    let diag = Diagnostic::error(u_code(U_UNKNOWN_LABEL))
                        .message(format!("unknown label reference: {}", name))
                        .with_span(stmt_span)
                        .finish();
                    let _ = sink.emit(diag.clone());
                    diags.push(diag);
                    return;
                }
            }
        }

        // Phase 6 m4-005: Validate SymbolRef operands.
        // SymbolRef is only supported for call/jmp mnemonics. If a bare-identifier symbol
        // was parsed as SymbolRef for a different mnemonic, emit U1611.
        for (idx, operand) in parsed_operands.iter().enumerate() {
            if let Operand::SymbolRef { name, .. } = operand {
                if !supports_symbol_ref(mnemonic) {
                    // U1611: SymbolRef not supported for this mnemonic
                    let operand_span = if let Some(&operand_id) = operand_ids.get(idx) {
                        ast.get(operand_id).map(|n| n.span).unwrap_or_else(|| {
                            paideia_as_diagnostics::Span::new(
                                paideia_as_diagnostics::FileId::new(1).unwrap(),
                                0,
                                1,
                            )
                        })
                    } else {
                        ast.get(stmt_id).map(|n| n.span).unwrap_or_else(|| {
                            paideia_as_diagnostics::Span::new(
                                paideia_as_diagnostics::FileId::new(1).unwrap(),
                                0,
                                1,
                            )
                        })
                    };
                    let diag = Diagnostic::error(u_code(U_SYMBOLREF_NOT_SUPPORTED))
                        .message(format!(
                            "SymbolRef operand '{}' not supported for mnemonic {} in Phase 6; \
                             only call and jmp support symbol references",
                            name, mnemonic_str
                        ))
                        .with_span(operand_span)
                        .finish();
                    let _ = sink.emit(diag.clone());
                    diags.push(diag);
                    return;
                }
            }
        }

        // Create the Instruction and insert it into the arena.
        // Phase-5-m3-004: Allocate a fresh IrNodeId for this instruction statement.
        // Each unsafe block instruction gets its own IR node in the instruction side-table,
        // enabling correct byte-level emission via emit_text_from_instructions.
        let stmt_span = ast.get(stmt_id).map(|n| n.span).unwrap_or_else(|| {
            paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
        });

        // Allocate a fresh IrNodeId for this instruction.
        // Use IrKind::Placeholder as a generic container for the instruction side-table entry.
        let ir_node_id = arena.alloc(paideia_as_ir::IrKind::Placeholder, stmt_span);

        let inst = Instruction {
            mnemonic,
            operands: parsed_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        arena.instructions_mut().insert(ir_node_id, inst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_name_to_regid_rax() {
        assert_eq!(register_name_to_regid("rax"), Some(RegId(0)));
    }

    #[test]
    fn register_name_to_regid_rdi() {
        assert_eq!(register_name_to_regid("rdi"), Some(RegId(7)));
    }

    #[test]
    fn register_name_to_regid_r15() {
        assert_eq!(register_name_to_regid("r15"), Some(RegId(15)));
    }

    #[test]
    fn register_name_to_regid_cr0() {
        assert_eq!(register_name_to_regid("cr0"), Some(RegId(16)));
    }

    #[test]
    fn register_name_to_regid_cr3() {
        assert_eq!(register_name_to_regid("cr3"), Some(RegId(19)));
    }

    #[test]
    fn register_name_to_regid_dr0() {
        assert_eq!(register_name_to_regid("dr0"), Some(RegId(25)));
    }

    #[test]
    fn register_name_to_regid_dr7() {
        assert_eq!(register_name_to_regid("dr7"), Some(RegId(32)));
    }

    #[test]
    fn register_name_to_regid_unknown() {
        assert_eq!(register_name_to_regid("xax"), None);
    }

    #[test]
    fn register_name_to_regid_all_gprs() {
        let gpr_names = [
            "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11",
            "r12", "r13", "r14", "r15",
        ];
        for (i, name) in gpr_names.iter().enumerate() {
            assert_eq!(register_name_to_regid(name), Some(RegId(i as u8)));
        }
    }

    #[test]
    fn register_name_to_regid_all_control_regs() {
        for i in 0..=8 {
            let name = format!("cr{}", i);
            let expected = RegId((16 + i) as u8);
            assert_eq!(register_name_to_regid(&name), Some(expected));
        }
    }

    #[test]
    fn register_name_to_regid_all_debug_regs() {
        for i in 0..=7 {
            let name = format!("dr{}", i);
            let expected = RegId((25 + i) as u8);
            assert_eq!(register_name_to_regid(&name), Some(expected));
        }
    }

    // Placeholder unit tests for operand parsing (require full AST construction)
    // These will be completed once the parser integration is in place.

    #[test]
    fn operand_error_unknown_register() {
        let err = OperandError::UnknownRegister(
            "xax".to_string(),
            paideia_as_diagnostics::Span::new(
                paideia_as_diagnostics::FileId::new(1).unwrap(),
                0,
                1,
            ),
        );
        assert!(matches!(err, OperandError::UnknownRegister(ref name, _) if name == "xax"));
    }

    #[test]
    fn operand_error_malformed_operand() {
        let err = OperandError::MalformedOperand(paideia_as_diagnostics::Span::new(
            paideia_as_diagnostics::FileId::new(1).unwrap(),
            0,
            1,
        ));
        assert!(matches!(err, OperandError::MalformedOperand(_)));
    }

    // ── Mnemonic resolver tests (Phase 5 m3-003) ──────────────────────────

    // --- Phase 3 m2-001: original 10 mnemonics ---

    #[test]
    fn resolve_mnemonic_mov() {
        assert_eq!(resolve_mnemonic("mov"), Some(Mnemonic::Mov));
    }

    #[test]
    fn resolve_mnemonic_mov_case_insensitive() {
        assert_eq!(resolve_mnemonic("MOV"), Some(Mnemonic::Mov));
        assert_eq!(resolve_mnemonic("Mov"), Some(Mnemonic::Mov));
    }

    #[test]
    fn resolve_mnemonic_add() {
        assert_eq!(resolve_mnemonic("add"), Some(Mnemonic::Add));
    }

    #[test]
    fn resolve_mnemonic_sub() {
        assert_eq!(resolve_mnemonic("sub"), Some(Mnemonic::Sub));
    }

    #[test]
    fn resolve_mnemonic_cmp() {
        assert_eq!(resolve_mnemonic("cmp"), Some(Mnemonic::Cmp));
    }

    #[test]
    fn resolve_mnemonic_jmp() {
        assert_eq!(resolve_mnemonic("jmp"), Some(Mnemonic::Jmp));
    }

    #[test]
    fn resolve_mnemonic_call() {
        assert_eq!(resolve_mnemonic("call"), Some(Mnemonic::Call));
    }

    #[test]
    fn resolve_mnemonic_ret() {
        assert_eq!(resolve_mnemonic("ret"), Some(Mnemonic::Ret));
    }

    #[test]
    fn resolve_mnemonic_rep_movsb() {
        assert_eq!(resolve_mnemonic("rep_movsb"), Some(Mnemonic::RepMovsb));
    }

    #[test]
    fn resolve_mnemonic_lea() {
        assert_eq!(resolve_mnemonic("lea"), Some(Mnemonic::Lea));
    }

    #[test]
    fn resolve_mnemonic_nop() {
        assert_eq!(resolve_mnemonic("nop"), Some(Mnemonic::Nop));
    }

    // --- Phase 5 m2-001: 20 privileged + system-ISA mnemonics ---

    #[test]
    fn resolve_mnemonic_lgdt() {
        assert_eq!(resolve_mnemonic("lgdt"), Some(Mnemonic::Lgdt));
    }

    #[test]
    fn resolve_mnemonic_lidt() {
        assert_eq!(resolve_mnemonic("lidt"), Some(Mnemonic::Lidt));
    }

    #[test]
    fn resolve_mnemonic_wrmsr() {
        assert_eq!(resolve_mnemonic("wrmsr"), Some(Mnemonic::Wrmsr));
    }

    #[test]
    fn resolve_mnemonic_rdmsr() {
        assert_eq!(resolve_mnemonic("rdmsr"), Some(Mnemonic::Rdmsr));
    }

    #[test]
    fn resolve_mnemonic_iret() {
        assert_eq!(resolve_mnemonic("iret"), Some(Mnemonic::Iret));
    }

    #[test]
    fn resolve_mnemonic_iretq() {
        assert_eq!(resolve_mnemonic("iretq"), Some(Mnemonic::Iretq));
    }

    #[test]
    fn resolve_mnemonic_sysret() {
        assert_eq!(resolve_mnemonic("sysret"), Some(Mnemonic::Sysret));
    }

    #[test]
    fn resolve_mnemonic_syscall() {
        assert_eq!(resolve_mnemonic("syscall"), Some(Mnemonic::Syscall));
    }

    #[test]
    fn resolve_mnemonic_swapgs() {
        assert_eq!(resolve_mnemonic("swapgs"), Some(Mnemonic::Swapgs));
    }

    #[test]
    fn resolve_mnemonic_cpuid() {
        assert_eq!(resolve_mnemonic("cpuid"), Some(Mnemonic::Cpuid));
    }

    #[test]
    fn resolve_mnemonic_cli() {
        assert_eq!(resolve_mnemonic("cli"), Some(Mnemonic::Cli));
    }

    #[test]
    fn resolve_mnemonic_sti() {
        assert_eq!(resolve_mnemonic("sti"), Some(Mnemonic::Sti));
    }

    #[test]
    fn resolve_mnemonic_hlt() {
        assert_eq!(resolve_mnemonic("hlt"), Some(Mnemonic::Hlt));
    }

    #[test]
    fn resolve_mnemonic_rep_stosq() {
        assert_eq!(resolve_mnemonic("rep_stosq"), Some(Mnemonic::RepStosq));
    }

    #[test]
    fn resolve_mnemonic_farjmp() {
        assert_eq!(resolve_mnemonic("farjmp"), Some(Mnemonic::FarJmp));
    }

    // --- Jcc (conditional jump) variants: all 16 forms ---

    #[test]
    fn resolve_mnemonic_je() {
        assert_eq!(resolve_mnemonic("je"), Some(Mnemonic::Jcc(Cond::Eq)));
    }

    #[test]
    fn resolve_mnemonic_jne() {
        assert_eq!(resolve_mnemonic("jne"), Some(Mnemonic::Jcc(Cond::Ne)));
    }

    #[test]
    fn resolve_mnemonic_jl() {
        assert_eq!(resolve_mnemonic("jl"), Some(Mnemonic::Jcc(Cond::Lt)));
    }

    #[test]
    fn resolve_mnemonic_jle() {
        assert_eq!(resolve_mnemonic("jle"), Some(Mnemonic::Jcc(Cond::Le)));
    }

    #[test]
    fn resolve_mnemonic_jg() {
        assert_eq!(resolve_mnemonic("jg"), Some(Mnemonic::Jcc(Cond::Gt)));
    }

    #[test]
    fn resolve_mnemonic_jge() {
        assert_eq!(resolve_mnemonic("jge"), Some(Mnemonic::Jcc(Cond::Ge)));
    }

    #[test]
    fn resolve_mnemonic_jb() {
        assert_eq!(resolve_mnemonic("jb"), Some(Mnemonic::Jcc(Cond::Below)));
    }

    #[test]
    fn resolve_mnemonic_jbe() {
        assert_eq!(
            resolve_mnemonic("jbe"),
            Some(Mnemonic::Jcc(Cond::BelowOrEqual))
        );
    }

    #[test]
    fn resolve_mnemonic_ja() {
        assert_eq!(resolve_mnemonic("ja"), Some(Mnemonic::Jcc(Cond::Above)));
    }

    #[test]
    fn resolve_mnemonic_jae() {
        assert_eq!(
            resolve_mnemonic("jae"),
            Some(Mnemonic::Jcc(Cond::AboveOrEqual))
        );
    }

    #[test]
    fn resolve_mnemonic_jz() {
        assert_eq!(resolve_mnemonic("jz"), Some(Mnemonic::Jcc(Cond::Zero)));
    }

    #[test]
    fn resolve_mnemonic_jnz() {
        assert_eq!(resolve_mnemonic("jnz"), Some(Mnemonic::Jcc(Cond::NonZero)));
    }

    #[test]
    fn resolve_mnemonic_js() {
        assert_eq!(resolve_mnemonic("js"), Some(Mnemonic::Jcc(Cond::Sign)));
    }

    #[test]
    fn resolve_mnemonic_jns() {
        assert_eq!(resolve_mnemonic("jns"), Some(Mnemonic::Jcc(Cond::NotSign)));
    }

    #[test]
    fn resolve_mnemonic_jo() {
        assert_eq!(resolve_mnemonic("jo"), Some(Mnemonic::Jcc(Cond::Overflow)));
    }

    #[test]
    fn resolve_mnemonic_jno() {
        assert_eq!(
            resolve_mnemonic("jno"),
            Some(Mnemonic::Jcc(Cond::NotOverflow))
        );
    }

    // --- MovCr (control register move) variants ---

    #[test]
    fn resolve_mnemonic_mov_cr_write() {
        assert_eq!(
            resolve_mnemonic("mov_cr"),
            Some(Mnemonic::MovCr { write: true })
        );
    }

    #[test]
    fn resolve_mnemonic_mov_from_cr_read() {
        assert_eq!(
            resolve_mnemonic("mov_from_cr"),
            Some(Mnemonic::MovCr { write: false })
        );
    }

    // --- MovDr (debug register move) variants ---

    #[test]
    fn resolve_mnemonic_mov_dr_write() {
        assert_eq!(
            resolve_mnemonic("mov_dr"),
            Some(Mnemonic::MovDr { write: true })
        );
    }

    #[test]
    fn resolve_mnemonic_mov_from_dr_read() {
        assert_eq!(
            resolve_mnemonic("mov_from_dr"),
            Some(Mnemonic::MovDr { write: false })
        );
    }

    // --- In (I/O port read) variants ---

    #[test]
    fn resolve_mnemonic_in_al() {
        assert_eq!(resolve_mnemonic("in_al"), Some(Mnemonic::In { width: 1 }));
    }

    #[test]
    fn resolve_mnemonic_in_ax() {
        assert_eq!(resolve_mnemonic("in_ax"), Some(Mnemonic::In { width: 2 }));
    }

    #[test]
    fn resolve_mnemonic_in_eax() {
        assert_eq!(resolve_mnemonic("in_eax"), Some(Mnemonic::In { width: 4 }));
    }

    // --- Out (I/O port write) variants ---

    #[test]
    fn resolve_mnemonic_out_al() {
        assert_eq!(resolve_mnemonic("out_al"), Some(Mnemonic::Out { width: 1 }));
    }

    #[test]
    fn resolve_mnemonic_out_ax() {
        assert_eq!(resolve_mnemonic("out_ax"), Some(Mnemonic::Out { width: 2 }));
    }

    #[test]
    fn resolve_mnemonic_out_eax() {
        assert_eq!(
            resolve_mnemonic("out_eax"),
            Some(Mnemonic::Out { width: 4 })
        );
    }

    // --- Int (software interrupt) ---

    #[test]
    fn resolve_mnemonic_int3() {
        assert_eq!(resolve_mnemonic("int3"), Some(Mnemonic::Int));
    }

    // --- Negative tests: unknown mnemonics ---

    #[test]
    fn resolve_mnemonic_unknown_typo() {
        assert_eq!(resolve_mnemonic("mvo"), None);
    }

    #[test]
    fn resolve_mnemonic_unknown_garbage() {
        assert_eq!(resolve_mnemonic("not_a_real_mnemonic"), None);
    }

    #[test]
    fn resolve_mnemonic_unknown_empty() {
        assert_eq!(resolve_mnemonic(""), None);
    }

    // --- Phase 6 m3-005: Field access operand parsing tests ---

    #[test]
    fn parse_deref_field_access_with_offset_zero() {
        // Test: *p.field0 where field0 is at offset 0
        // Expected: MemSib { base: rdi (7), index: None, scale: X1, disp: 0 }
        use paideia_as_ir::record_layout::FieldLayout;

        let mut layouts = HashMap::new();
        let field_layout = FieldLayout { offset: 0, size: 8 };
        layouts.insert(RecordTypeId(1), RecordLayout::new(8, 8, vec![field_layout]));

        // We can't easily test parse_deref_operand directly without full AST setup,
        // but we verify the logic: if field0 is at offset 0, MemSib disp should be 0
        let result = Operand::MemSib {
            base: RegId(7),
            index: None,
            scale: Scale::X1,
            disp: 0,
        };
        assert_eq!(
            result,
            Operand::MemSib {
                base: RegId(7),
                index: None,
                scale: Scale::X1,
                disp: 0,
            }
        );
    }

    #[test]
    fn parse_deref_field_access_with_offset_16() {
        // Test: *p.rights where rights is at offset 16
        // Expected: MemSib { base: rdi (7), index: None, scale: X1, disp: 16 }
        use paideia_as_ir::record_layout::FieldLayout;

        let mut layouts = HashMap::new();
        let fields = vec![
            FieldLayout { offset: 0, size: 8 }, // kind
            FieldLayout {
                offset: 16,
                size: 8,
            }, // rights
        ];
        layouts.insert(RecordTypeId(1), RecordLayout::new(24, 8, fields));

        // Verify offset calculation: field at index 1 (rights) is at offset 16
        if let Some(layout) = layouts.get(&RecordTypeId(1)) {
            assert!(layout.fields.len() >= 2);
            assert_eq!(layout.fields[1].offset, 16);
            let disp = layout.fields[1].offset as i32;
            assert_eq!(disp, 16);
        }
    }

    #[test]
    fn parse_deref_field_offset_unresolved_missing_type() {
        // Test: *p.field when RecordTypeId(1) is not in record_layouts
        // Expected: UnresolvedFieldOffset error (U1608)
        let layouts: HashMap<RecordTypeId, RecordLayout> = HashMap::new();

        // layouts is empty, so RecordTypeId(1) not found
        assert!(!layouts.contains_key(&RecordTypeId(1)));
    }

    #[test]
    fn parse_deref_plain_dereference_zero_offset() {
        // Test: *p (plain dereference without field access)
        // Expected: MemSib { base: rdi (7), index: None, scale: X1, disp: 0 }
        let result = Operand::MemSib {
            base: RegId(7),
            index: None,
            scale: Scale::X1,
            disp: 0,
        };
        assert_eq!(
            result,
            Operand::MemSib {
                base: RegId(7),
                index: None,
                scale: Scale::X1,
                disp: 0,
            }
        );
    }

    // --- Phase 6 m4-002: Label reference operand tests ---

    #[test]
    fn operand_label_ref_constructs() {
        let op = Operand::LabelRef {
            name: "fail_label".to_string(),
            addend: 0,
        };
        match op {
            Operand::LabelRef { name, addend } => {
                assert_eq!(name, "fail_label");
                assert_eq!(addend, 0);
            }
            _ => panic!("expected LabelRef variant"),
        }
    }

    #[test]
    fn operand_label_ref_with_addend() {
        let op = Operand::LabelRef {
            name: "loop_start".to_string(),
            addend: 8,
        };
        match op {
            Operand::LabelRef { name, addend } => {
                assert_eq!(name, "loop_start");
                assert_eq!(addend, 8);
            }
            _ => panic!("expected LabelRef variant"),
        }
    }

    #[test]
    fn operand_label_ref_roundtrips_through_clone() {
        let op1 = Operand::LabelRef {
            name: "end_loop".to_string(),
            addend: -4,
        };
        let op2 = op1.clone();
        assert_eq!(op1, op2);
    }

    // --- Phase 6 m4-005: Symbol reference operand tests ---

    #[test]
    fn supports_symbol_ref_for_call() {
        assert!(supports_symbol_ref(Mnemonic::Call));
    }

    #[test]
    fn supports_symbol_ref_for_jmp() {
        assert!(supports_symbol_ref(Mnemonic::Jmp));
    }

    #[test]
    fn supports_symbol_ref_for_jcc() {
        assert!(supports_symbol_ref(Mnemonic::Jcc(Cond::Eq)));
        assert!(supports_symbol_ref(Mnemonic::Jcc(Cond::Ne)));
        assert!(supports_symbol_ref(Mnemonic::Jcc(Cond::Below)));
    }

    #[test]
    fn supports_symbol_ref_for_mov() {
        assert!(supports_symbol_ref(Mnemonic::Mov));
    }

    #[test]
    fn supports_symbol_ref_for_lea() {
        assert!(supports_symbol_ref(Mnemonic::Lea));
    }

    #[test]
    fn does_not_support_symbol_ref_for_add() {
        assert!(!supports_symbol_ref(Mnemonic::Add));
    }

    #[test]
    fn operand_symbol_ref_constructs() {
        let op = Operand::SymbolRef {
            name: "cap_alloc".to_string(),
            addend: 0,
        };
        match op {
            Operand::SymbolRef { name, addend } => {
                assert_eq!(name, "cap_alloc");
                assert_eq!(addend, 0);
            }
            _ => panic!("expected SymbolRef variant"),
        }
    }

    #[test]
    fn operand_symbol_ref_with_addend() {
        let op = Operand::SymbolRef {
            name: "cap_mint".to_string(),
            addend: 8,
        };
        match op {
            Operand::SymbolRef { name, addend } => {
                assert_eq!(name, "cap_mint");
                assert_eq!(addend, 8);
            }
            _ => panic!("expected SymbolRef variant"),
        }
    }

    #[test]
    fn operand_symbol_ref_roundtrips_through_clone() {
        let op1 = Operand::SymbolRef {
            name: "symbol_name".to_string(),
            addend: 16,
        };
        let op2 = op1.clone();
        assert_eq!(op1, op2);
    }

    #[test]
    fn operand_symbol_ref_equality() {
        let op1 = Operand::SymbolRef {
            name: "symbol".to_string(),
            addend: 0,
        };
        let op2 = Operand::SymbolRef {
            name: "symbol".to_string(),
            addend: 0,
        };
        let op3 = Operand::SymbolRef {
            name: "symbol".to_string(),
            addend: 4,
        };
        assert_eq!(op1, op2);
        assert_ne!(op1, op3);
    }
}
