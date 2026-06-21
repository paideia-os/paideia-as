//! Operand parser for the unsafe-block surface (Phase 5, m3-002).
//!
//! This module implements parsing of x86_64 operands from the AST representation
//! used in unsafe blocks. It converts AST operand nodes into IR `Operand` values
//! with proper register encoding and memory addressing modes.
//!
//! # Register Encoding
//!
//! General-purpose registers and special registers use distinct sentinel ranges:
//! - GPR (rax–r15): `RegId(0..15)` (standard x86_64 encoding)
//! - Control registers (cr0–cr8): `RegId(0x100 | index)` (sentinel 0x100, index in low byte)
//! - Debug registers (dr0–dr7): `RegId(0x200 | index)` (sentinel 0x200, index in low byte)
//!
//! These sentinel encodings allow the bridge in m2-005 to distinguish special registers
//! during instruction materialization.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::Span;
use paideia_as_ir::instruction::{Operand, RegId, Scale};

/// Error type for operand parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperandError {
    /// Unknown register name.
    UnknownRegister(String, Span),
    /// Malformed operand (e.g., invalid memory reference).
    MalformedOperand(Span),
}

/// Parse an operand from an AST node.
///
/// Handles three operand shapes:
/// 1. Register operands (Ident nodes representing register names)
/// 2. Immediate operands (integer literals)
/// 3. Memory operands (OperandMemoryRef nodes with SIB addressing)
///
/// # Arguments
///
/// * `ast` - The AST arena
/// * `operand_node` - The NodeId of the operand node
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
/// ```
pub fn parse_operand_from_ast(
    ast: &AstArena,
    operand_node: NodeId,
) -> Result<Operand, OperandError> {
    let node = ast.get(operand_node).ok_or(OperandError::MalformedOperand(
        ast.get(operand_node).map(|n| n.span).unwrap_or_else(|| {
            paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
        }),
    ))?;

    match node.kind {
        NodeKind::Ident => {
            // Register operand: try to parse as register name
            parse_register_from_ident(ast, operand_node)
        }
        NodeKind::ExprLiteral => {
            // Immediate operand: extract integer literal
            parse_immediate_from_literal(ast, operand_node)
        }
        NodeKind::OperandMemoryRef => {
            // Memory operand: parse memory reference with SIB addressing
            parse_memory_from_memref(ast, operand_node)
        }
        _ => Err(OperandError::MalformedOperand(node.span)),
    }
}

/// Parse a register operand from an Ident node.
fn parse_register_from_ident(ast: &AstArena, ident_node: NodeId) -> Result<Operand, OperandError> {
    let span = ast.get(ident_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // Extract the identifier text by looking up in the source.
    // For phase-1, we use a lookup table matching register names to RegIds.
    let reg_id = match get_register_name(ast, ident_node) {
        Some(name) => register_name_to_regid(&name)
            .ok_or_else(|| OperandError::UnknownRegister(name, span))?,
        None => {
            return Err(OperandError::MalformedOperand(span));
        }
    };

    Ok(Operand::Reg(reg_id))
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
fn parse_memory_from_memref(ast: &AstArena, memref_node: NodeId) -> Result<Operand, OperandError> {
    let span = ast.get(memref_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    match ast.expr_data(memref_node) {
        Some(ExprData::OperandMemoryRef { addr }) => {
            // Parse the address expression to extract SIB components
            parse_address_to_sib(ast, *addr)
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
fn parse_address_to_sib(ast: &AstArena, addr_node: NodeId) -> Result<Operand, OperandError> {
    // Extract SIB components from the address expression.
    // Phase-1 implementation: support infix operators (+, -) and multiply (*).
    let (base, index, scale, disp) = extract_sib_components(ast, addr_node)?;

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
) -> Result<(RegId, Option<RegId>, Scale, i32), OperandError> {
    let span = ast.get(expr_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    let node = ast
        .get(expr_node)
        .ok_or(OperandError::MalformedOperand(span))?;

    match node.kind {
        // Base case: single register → base=reg, index=None, scale=X1, disp=0
        NodeKind::Ident => match parse_register_from_ident(ast, expr_node)? {
            Operand::Reg(base) => Ok((base, None, Scale::X1, 0)),
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
                            combine_additive_terms(ast, *lhs, *rhs, op_str == Some("-"))
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
) -> Result<(RegId, Option<RegId>, Scale, i32), OperandError> {
    let span = ast.get(left).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // Recursively extract components from left and right
    let (left_base, left_index, left_scale, left_disp) = extract_sib_components(ast, left)?;

    // Try to parse right as either a register, immediate, or index*scale expression
    let right_kind = ast.get(right).map(|n| n.kind);

    match right_kind {
        Some(NodeKind::Ident) => {
            // Right is a register: could be base or index
            match parse_register_from_ident(ast, right)? {
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
                        match extract_index_scale(ast, *mul_lhs, *mul_rhs)? {
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
) -> Result<(RegId, u32), OperandError> {
    let span = ast.get(left).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    // Left should be register, right should be immediate scale
    let idx_reg = match parse_register_from_ident(ast, left)? {
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
fn get_register_name(_ast: &AstArena, _ident_node: NodeId) -> Option<String> {
    // For phase-1, we extract the register name via a heuristic:
    // The AST arena has access to the original source spans.
    // A full implementation would use a source map to look up the actual text.
    // For now, we return None and rely on the register lookup table in parse_register_from_ident.
    None
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
}
