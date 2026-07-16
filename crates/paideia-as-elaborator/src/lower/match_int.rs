//! Integer-scrutinee match elaboration (#1210).
//!
//! Populates IntMatchScrutineeTable and MatchArmMeta.int_pattern_value for
//! matches over integer types. Runs post-populate_match_arm_meta to avoid
//! interfering with enum-match classification.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_ir::{
    IrArena, IrKind, IrNodeId, IntMatchScrutinee, IntWidth,
};
use paideia_as_diagnostics::{SourceMap};
use std::collections::HashMap;

use super::text_extract::{extract_source_text_for_record_cons, build_binding_type_map};

/// Strip integer type suffix (u8/u16/u32/u64/i8/i16/i32/i64) from source text.
/// Returns the string without the suffix, or the original string if no suffix found.
fn strip_integer_suffix(text: &str) -> &str {
    // Find the suffix by scanning from the end for non-digit characters
    let chars_vec: Vec<char> = text.chars().collect();
    for (i, &c) in chars_vec.iter().enumerate().rev() {
        if !c.is_ascii_digit() && c != '_' {
            // Found the start of the suffix
            return &text[..i];
        }
    }
    text
}

/// Parse an integer type suffix (u8/u16/u32/u64/i8/i16/i32/i64) from source text.
///
/// Returns (width, signed) or None if the text doesn't match a known suffix pattern.
fn parse_integer_suffix(text: &str) -> Option<(IntWidth, bool)> {
    match text {
        "u8" => Some((IntWidth::W8, false)),
        "u16" => Some((IntWidth::W16, false)),
        "u32" => Some((IntWidth::W32, false)),
        "u64" => Some((IntWidth::W64, false)),
        "i8" => Some((IntWidth::W8, true)),
        "i16" => Some((IntWidth::W16, true)),
        "i32" => Some((IntWidth::W32, true)),
        "i64" => Some((IntWidth::W64, true)),
        _ => None,
    }
}

/// Extract integer type suffix (e.g. "u64" from "123u64") from source text.
fn extract_integer_type_suffix(text: &str) -> Option<(IntWidth, bool)> {
    // Strip leading 0x/0o/0b if present to handle different bases
    let mut candidate = text;
    if candidate.starts_with("0x") || candidate.starts_with("0X") {
        candidate = &candidate[2..];
    } else if candidate.starts_with("0o") || candidate.starts_with("0O") {
        candidate = &candidate[2..];
    } else if candidate.starts_with("0b") || candidate.starts_with("0B") {
        candidate = &candidate[2..];
    }

    // Find the suffix by scanning from the end for a non-digit character
    // This is a simplistic approach that works for "123u64" etc.
    let chars_vec: Vec<char> = candidate.chars().collect();
    for (i, &c) in chars_vec.iter().enumerate().rev() {
        if !c.is_ascii_digit() && c != '_' {
            // Found the start of the suffix
            let suffix = &candidate[i..];
            if let Some(result) = parse_integer_suffix(suffix) {
                return Some(result);
            }
            break;
        }
    }
    None
}

/// Populate integer-scrutinee match metadata for a single match expression.
///
/// Returns true if this match was classified as integer-match, false otherwise
/// (in which case the enum pipeline should handle it).
fn classify_match_integer(
    ast: &AstArena,
    ir: &mut IrArena,
    source_map: &SourceMap,
    match_ast_id: NodeId,
    match_ir_id: IrNodeId,
) -> bool {
    // Step 1: Skip if enum pipeline already claimed this match
    if ir.match_scrutinee_table().contains_key(match_ir_id) {
        return false;
    }

    // Step 2: Get match arms from AST
    let (_scrutinee_id, arms) = match ast.expr_data(match_ast_id) {
        Some(ExprData::Match { scrutinee, arms, .. }) => (scrutinee, arms),
        _ => return false,
    };

    // Step 3: Classify scrutinee
    let scrutinee_ir_id = ir.children(match_ir_id).first().copied();
    let scrutinee_ir_id = match scrutinee_ir_id {
        Some(id) => id,
        None => return false,
    };

    let scrutinee_width_signed = match ir.get(scrutinee_ir_id) {
        Some(n) => match n.kind {
            IrKind::Literal => {
                // ExprLiteral: parse suffix from source text
                // Get the source text for the scrutinee literal
                let span = n.span;
                let file_id = span.file();
                let source = source_map.content(file_id);
                let start = span.byte_start() as usize;
                let end = start + span.byte_len() as usize;
                if end <= source.len() {
                    let text = &source[start..end];
                    // Try to extract suffix
                    if let Some((width, signed)) = extract_integer_type_suffix(text) {
                        Some((width, signed))
                    } else {
                        // Unsuffixed: default to u64
                        Some((IntWidth::W64, false))
                    }
                } else {
                    None
                }
            }
            IrKind::Var => {
                // #1218 (follow-up to #1210): currently unreachable in practice.
                // binding_names is populated in cmd_build's Phase 6 m2-004/m2-004b
                // AFTER populate_int_match_meta runs, so the lookup below always
                // returns None for local let-bindings and lambda parameters.
                // Match on variable u64 scrutinee falls through to enum path → U1650.
                // Fix path: traverse AST directly to find binding's type annotation.
                let binding_type_map = build_binding_type_map(ast, source_map);
                if let Some(name) = ir.binding_names().get(scrutinee_ir_id) {
                    binding_type_map.get(name).and_then(|type_text| {
                        parse_integer_suffix(type_text)
                    })
                } else {
                    None
                }
            }
            _ => None,
        },
        None => return false,
    };

    let (_scrutinee_width, _scrutinee_signed) = match scrutinee_width_signed {
        Some(ws) => ws,
        None => return false,
    };

    // Step 4: For each arm, extract integer pattern values
    let arm_ir_ids: Vec<IrNodeId> = ir.children(match_ir_id)[1..].to_vec();
    let mut default_arm_count = 0;

    for (arm_idx, arm) in arms.iter().enumerate() {
        let arm_ir_id = match arm_ir_ids.get(arm_idx) {
            Some(id) => *id,
            None => continue,
        };

        let pattern_node = match ast.get(arm.pattern) {
            Some(n) => n,
            None => continue,
        };

        match pattern_node.kind {
            NodeKind::PatLiteral => {
                // Integer pattern: extract value and store in arm metadata
                // Extract literal text from source, strip type suffix, and parse
                let pattern_node = pattern_node;
                let span = pattern_node.span;
                let file_id = span.file();
                let source = source_map.content(file_id);
                let start = span.byte_start() as usize;
                let end = start + span.byte_len() as usize;
                if end <= source.len() {
                    let text = &source[start..end];
                    // Strip the type suffix (u64, i32, etc.)
                    let text_no_suffix = strip_integer_suffix(text);
                    // Try to parse the number without suffix
                    if let Ok(value) = text_no_suffix.parse::<i64>() {
                        let mut meta = ir.match_arm_meta_mut()
                            .remove(arm_ir_id)
                            .unwrap_or_default();
                        meta.int_pattern_value = Some(value);
                        ir.match_arm_meta_mut().insert(arm_ir_id, meta);
                    } else if let Ok(value) = text_no_suffix.parse::<u64>() {
                        let mut meta = ir.match_arm_meta_mut()
                            .remove(arm_ir_id)
                            .unwrap_or_default();
                        meta.int_pattern_value = Some(value as i64);
                        ir.match_arm_meta_mut().insert(arm_ir_id, meta);
                    }
                }
            }
            NodeKind::PatWildcard => {
                // Wildcard pattern → default arm
                let mut meta = ir.match_arm_meta_mut().remove(arm_ir_id).unwrap_or_default();
                meta.is_default = true;
                ir.match_arm_meta_mut().insert(arm_ir_id, meta);
                default_arm_count += 1;
            }
            NodeKind::PatIdent => {
                // Check if this is the underscore wildcard
                if let Some(text) = extract_source_text_for_record_cons(ast, source_map, arm.pattern)
                {
                    if text == "_" {
                        let mut meta = ir.match_arm_meta_mut().remove(arm_ir_id).unwrap_or_default();
                        meta.is_default = true;
                        ir.match_arm_meta_mut().insert(arm_ir_id, meta);
                        default_arm_count += 1;
                    } else {
                        // Named pattern in integer match (non-default) - skip classification
                        return false;
                    }
                } else {
                    return false;
                }
            }
            _ => {
                // Non-integer pattern in integer match
                return false; // Skip classification
            }
        }
    }

    // Step 5: Only classify if we found at least one integer pattern or default arm
    // (We'll require a default arm, but allow integer-only arms if there's also a default)
    let has_integer_patterns = arms.iter().enumerate().any(|(_arm_idx, arm)| {
        if let Some(pattern_node) = ast.get(arm.pattern) {
            matches!(pattern_node.kind, NodeKind::PatLiteral)
        } else {
            false
        }
    });

    if !has_integer_patterns {
        return false;
    }

    if default_arm_count == 0 {
        return false;
    }

    // Insert into int_match_scrutinee table
    let int_meta = IntMatchScrutinee {
        width: _scrutinee_width,
        signed: _scrutinee_signed,
    };
    ir.int_match_scrutinee_mut().insert(match_ir_id, int_meta);
    true
}

/// Populate integer-scrutinee match metadata for all matches in the IR.
///
/// Called AFTER populate_match_arm_meta and BEFORE populate_auto_jump_table_meta.
/// Classifies integer-scrutinee matches and populates IntMatchScrutineeTable.
pub(super) fn populate_int_match_meta(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    source_map: &SourceMap,
) {
    for ast_id in 1..=ast.len() {
        if let Some(match_ast_id) = NodeId::new(ast_id as u32) {
            if let Some(ast_node) = ast.get(match_ast_id) {
                if ast_node.kind == NodeKind::ExprMatch {
                    if let Some(match_ir_id) = ast_to_ir.get(&match_ast_id) {
                        classify_match_integer(
                            ast,
                            ir,
                            source_map,
                            match_ast_id,
                            *match_ir_id,
                        );
                    }
                }
            }
        }
    }
}
