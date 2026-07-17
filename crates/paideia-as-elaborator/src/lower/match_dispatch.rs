//! Jump-table match dispatch metadata populator.
//! Split out of `lower.rs` (2026-07-08); PA-r15-009c (#1055).
//!
//! For every `ExprMatch` node marked with the `@jump_table` attribute in the
//! AST, this pass computes and inserts `MatchDispatchMeta` (min_arm, range,
//! covered_arms, density_ok) into the IR arena's side-table, and records
//! the per-arm `(value, arm_index)` pairs for rodata synthesis.
//!
//! Non-integer, non-wildcard patterns inside a jump-table match emit T0550
//! and force `density_ok = false`.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap};
use paideia_as_ir::{IrArena, IrNodeId};
use std::collections::HashMap;

use crate::unsafe_walker::extract_integer_from_span;

/// PA-r15-009c (#1055): Populate match dispatch metadata for @jump_table matches.
///
/// For each Match node in the IR with the `@jump_table` attribute on the
/// corresponding AST node, compute and insert the dispatch metadata (min_arm,
/// range, covered_arms, density_ok) into the arena's side-table.
///
/// If any arm's pattern is not an integer literal, emit T0550 diagnostic and
/// set `density_ok = false`.
pub(super) fn populate_match_dispatch_meta(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    // Walk through all IR nodes looking for Match nodes.
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        // Get the AST node and check if it's an ExprMatch.
        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        if ast_node.kind != NodeKind::ExprMatch {
            continue;
        }

        // Get the ExprData::Match to access attrs and arms.
        let (attrs, arms) = match ast.expr_data(ast_id) {
            Some(ExprData::Match { attrs, arms, .. }) => (attrs, arms),
            _ => continue,
        };

        // Only process if @jump_table attribute is present.
        if !attrs.jump_table {
            continue;
        }

        // Get the corresponding IR node ID.
        let ir_match_id = match ast_to_ir.get(&ast_id) {
            Some(id) => *id,
            None => continue,
        };

        // Extract integer values from arm patterns with their arm indices.
        let mut arm_values: Vec<i64> = Vec::new();
        let mut arm_value_index_pairs: Vec<(i64, u32)> = Vec::new();
        let mut has_non_integer_pattern = false;

        for (arm_idx, arm) in arms.iter().enumerate() {
            // Try to extract integer value from the pattern.
            if let Some(value) = try_extract_integer_pattern(ast, arm.pattern, source_map) {
                arm_values.push(value);
                arm_value_index_pairs.push((value, arm_idx as u32));
            } else {
                // Check if it's a wildcard (which is OK, just not counted).
                if !is_wildcard_pattern(ast, arm.pattern, source_map) {
                    // Non-integer, non-wildcard pattern found.
                    // Emit T0550 diagnostic.
                    if let Some(pattern_node) = ast.get(arm.pattern) {
                        if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 550) {
                            let diag = Diagnostic::error(code)
                                .message("non-integer pattern in @jump_table match")
                                .with_span(pattern_node.span)
                                .finish();
                            let _ = sink.emit(diag);
                        }
                    }
                    has_non_integer_pattern = true;
                }
                // Wildcard patterns are skipped but don't cause an error.
            }
        }

        // Compute dispatch metadata.
        let density_ok = if has_non_integer_pattern || arm_values.is_empty() {
            false
        } else {
            let min_val = *arm_values.iter().min().unwrap_or(&0);
            let max_val = *arm_values.iter().max().unwrap_or(&0);
            let range = (max_val - min_val + 1) as u32;
            let covered_arms = arm_values.len() as u32;
            covered_arms.saturating_mul(2) >= range
        };

        // Extract min_arm and range.
        let (min_arm, range) = if !arm_values.is_empty() {
            let min_val = *arm_values.iter().min().unwrap();
            let max_val = *arm_values.iter().max().unwrap();
            let r = (max_val - min_val + 1) as u32;
            (min_val, r)
        } else {
            (0, 1)
        };

        let covered_arms = arm_values.len() as u32;

        // Insert into the dispatch metadata side-table.
        ir.match_dispatch_meta_mut().insert(
            ir_match_id,
            paideia_as_ir::MatchDispatchMeta {
                jump_table: true,
                min_arm,
                range,
                covered_arms,
                density_ok,
            },
        );

        // Also store the per-arm (value, index) pairs for rodata synthesis.
        ir.match_jump_table_arm_values_mut()
            .insert(ir_match_id, arm_value_index_pairs);
    }
}

/// Try to extract an integer value from a pattern node.
///
/// Returns Some(value) if the pattern is an integer literal, None otherwise.
/// Uses the SourceMap to extract and parse the actual integer value from source.
/// Issue #1002: Transparently unwrap PatBinding patterns.
fn try_extract_integer_pattern(
    ast: &AstArena,
    pattern_id: NodeId,
    source_map: &SourceMap,
) -> Option<i64> {
    let mut current_pattern_id = pattern_id;

    // Issue #1002: Unwrap PatBinding transparently
    if let Some(pattern_node) = ast.get(current_pattern_id) {
        if pattern_node.kind == NodeKind::PatBinding {
            if let Some(paideia_as_ast::PatternData::Binding { inner, .. }) = ast.pattern_data(current_pattern_id) {
                current_pattern_id = *inner;
            }
        }
    }

    let pattern_node = ast.get(current_pattern_id)?;

    // Check if it's a PatLiteral node (real pattern literal, not expression literal).
    if pattern_node.kind != NodeKind::PatLiteral {
        return None;
    }

    // Get the pattern data and verify it's a Literal pattern.
    let pattern_data = ast.pattern_data(current_pattern_id)?;
    if let paideia_as_ast::PatternData::Literal { lit } = pattern_data {
        // Use extract_integer_from_span to get the actual value from source text.
        extract_integer_from_span(ast, *lit, source_map)
    } else {
        None
    }
}

/// Check if a pattern is a wildcard (`_`).
///
/// Returns true if:
/// 1. The pattern is a NodeKind::PatWildcard with PatternData::Wildcard, OR
/// 2. The pattern is a NodeKind::PatIdent with identifier text "_"
/// Issue #1002: Transparently unwrap PatBinding patterns.
fn is_wildcard_pattern(ast: &AstArena, pattern_id: NodeId, source_map: &SourceMap) -> bool {
    let mut current_pattern_id = pattern_id;

    // Issue #1002: Unwrap PatBinding transparently
    if let Some(pattern_node) = ast.get(current_pattern_id) {
        if pattern_node.kind == NodeKind::PatBinding {
            if let Some(paideia_as_ast::PatternData::Binding { inner, .. }) = ast.pattern_data(current_pattern_id) {
                current_pattern_id = *inner;
            }
        }
    }

    let pattern_node = match ast.get(current_pattern_id) {
        Some(n) => n,
        None => return false,
    };

    // Check if it's a PatWildcard node (real wildcard pattern).
    if pattern_node.kind == NodeKind::PatWildcard {
        if let Some(paideia_as_ast::PatternData::Wildcard) = ast.pattern_data(current_pattern_id) {
            return true;
        }
    }

    // Check if it's a PatIdent with identifier text "_"
    if pattern_node.kind == NodeKind::PatIdent {
        if let Some(paideia_as_ast::PatternData::Ident { .. }) = ast.pattern_data(current_pattern_id) {
            // Extract the identifier text from the pattern's span
            let span = pattern_node.span;
            let file_id = span.file();
            let source = source_map.content(file_id);
            let start = span.byte_start() as usize;
            let end = start + span.byte_len() as usize;
            if end <= source.len() {
                let text = &source[start..end];
                if text == "_" {
                    return true;
                }
            }
        }
    }

    false
}
