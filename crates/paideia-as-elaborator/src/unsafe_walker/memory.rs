//! Memory-operand parsing: SIB decomposition, `*expr` deref, `[base + index*scale + disp]`.
//! Split out of `unsafe_walker.rs` (2026-07-08).

use std::collections::HashMap;

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_ir::instruction::{Operand, RegId, Scale};
use paideia_as_ir::record_layout::{RecordLayout, RecordTypeId};

use super::OperandError;
use super::immediate::extract_integer_from_span;
use super::register::{get_register_name, parse_register_from_ident};
use super::symbol_ref::try_parse_symbol_memory;

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
pub(super) fn parse_deref_operand(
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

/// Parse a memory operand from an OperandMemoryRef node.
///
/// PA10-006c: Support both traditional SIB addressing and RIP-relative symbol references.
/// Handles:
/// - `[base + disp]` → MemSib
/// - `[base + index*scale + disp]` → MemSib
/// - `[symbol]` → SymbolRef (RIP-relative)
/// - `[rip + symbol]` → SymbolRef (RIP-relative)
pub(super) fn parse_memory_from_memref(
    ast: &AstArena,
    memref_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Result<Operand, OperandError> {
    let span = ast.get(memref_node).map(|n| n.span).unwrap_or_else(|| {
        paideia_as_diagnostics::Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    });

    match ast.expr_data(memref_node) {
        Some(ExprData::OperandMemoryRef { segment, addr }) => {
            // Parse the inner memory operand first.
            // First, check if this is a bare symbol or [rip + symbol] form
            let inner_operand = if let Ok(symbol_operand) = try_parse_symbol_memory(ast, *addr, source_map) {
                // PA-R13-002 deferral: gs-relative symbols not yet supported
                if segment.is_some() {
                    return Err(OperandError::MalformedOperand(span));
                }
                symbol_operand
            } else {
                // Otherwise, fall back to standard SIB addressing
                parse_address_to_sib(ast, *addr, source_map)?
            };

            // Wrap in MemSeg if segment prefix is present.
            match segment {
                Some(seg) => {
                    use paideia_as_ast::SegPrefix as AstSegPrefix;
                    use paideia_as_ir::SegPrefix as IrSegPrefix;
                    let ir_seg = match seg {
                        AstSegPrefix::Fs => IrSegPrefix::Fs,
                        AstSegPrefix::Gs => IrSegPrefix::Gs,
                    };
                    Ok(Operand::MemSeg { seg: ir_seg, inner: Box::new(inner_operand) })
                }
                None => Ok(inner_operand),
            }
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
pub(super) fn parse_address_to_sib(
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
                    let op_str = get_infix_op_name(ast, *op, source_map);

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
                    if left_base == paideia_as_ir::abi::RAX && left_index.is_none() {
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
            let right_disp = extract_integer_from_span(ast, right, source_map).unwrap_or(0) as i32;
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
                    let op_str = get_infix_op_name(ast, *op, source_map);
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

    let scale_factor = extract_integer_from_span(ast, right, source_map).unwrap_or(1) as u32;
    Ok((idx_reg, scale_factor))
}

/// Extract infix operator name from an operator node.
///
/// PA10-006g: Parses infix operator names from their source text representation.
/// Supports +, -, *, /, %, &, |, ^, <<, >>, ==, !=, <, >, <=, >=.
/// Returns the canonical operator string, or None if extraction fails.
pub(super) fn get_infix_op_name(
    ast: &AstArena,
    op_node: NodeId,
    source_map: &paideia_as_diagnostics::SourceMap,
) -> Option<&'static str> {
    let node = ast.get(op_node)?;

    // Extract the operator span (works for both Ident and Placeholder nodes).
    // PA10-006g: The parser creates Placeholder nodes for operators with the operator span.
    // This allows us to extract the operator text regardless of node kind.
    let span = node.span;
    let file_id = span.file();
    let source = source_map.content(file_id);

    // Extract the text from the span
    let start = span.byte_start() as usize;
    let end = start + span.byte_len() as usize;
    if end > source.len() {
        return None;
    }

    let text = &source[start..end];

    // Match common operators
    match text {
        "+" => Some("+"),
        "-" => Some("-"),
        "*" => Some("*"),
        "/" => Some("/"),
        "%" => Some("%"),
        "&" => Some("&"),
        "|" => Some("|"),
        "^" => Some("^"),
        "==" => Some("=="),
        "!=" => Some("!="),
        "<" => Some("<"),
        ">" => Some(">"),
        "<=" => Some("<="),
        ">=" => Some(">="),
        "<<" => Some("<<"),
        ">>" => Some(">>"),
        _ => None,
    }
}
