//! `EnumConsInfoTable` populator for `ExprCall` nodes shaped `EnumType::Variant(..)`.
//! Split out of `lower.rs` (2026-07-08); Phase 7 m4-003 (#1048/#1049).
//!
//! Walks all AST Call nodes. For each Call whose callee is an ExprPath with
//! exactly 2 segments:
//!
//! - Extracts seg0_name (enum type name) and seg1_name (variant name) from source text.
//! - Looks up seg0_name in `EnumRegistry.by_name` → `EnumTypeId`.
//! - Looks up seg1_name in `registry.variants[type_id]` → variant_index.
//! - Rewrites the IR node kind from `App` (placeholder) to `EnumCons`.
//! - Inserts `(EnumCons_ir_id -> EnumConsInfo)` into `enum_cons_info` side-table.
//!
//! Emits T0554 on: known enum but unknown variant.
//! Silently skips on: seg0 not in enum registry (that's a plain call).

use paideia_as_ast::{AstArena, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap};
use paideia_as_ir::{IrArena, IrNodeId};
use std::collections::HashMap;

use super::text_extract::extract_source_text_for_record_cons;

/// Phase 7 m4-003 (#1048/#1049): Populate EnumConsInfoTable for EnumCons expressions.
pub(super) fn populate_enum_cons_info(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    registry: &crate::EnumRegistry,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    // Walk all AST nodes with NodeKind::ExprCall
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        if ast_node.kind != NodeKind::ExprCall {
            continue;
        }

        // Get the ExprData::Call to access callee
        let callee_id = match ast.expr_data(ast_id) {
            Some(paideia_as_ast::ExprData::Call { callee, .. }) => callee,
            _ => continue,
        };

        // Check if callee is an ExprPath
        let callee_node = match ast.get(*callee_id) {
            Some(n) => n,
            None => continue,
        };

        if callee_node.kind != NodeKind::ExprPath {
            // Not a path; skip silently (could be a computed function)
            continue;
        }

        // Extract path segments
        let segments = match ast.expr_data(*callee_id) {
            Some(paideia_as_ast::ExprData::Path { segments }) => segments,
            _ => continue,
        };

        // We're looking for exactly 2 segments: EnumType::Variant
        if segments.len() != 2 {
            continue;
        }

        // Extract segment names from source text
        let seg0_name = match extract_source_text_for_record_cons(ast, source_map, segments[0]) {
            Some(name) => name,
            None => continue,
        };

        let seg1_name = match extract_source_text_for_record_cons(ast, source_map, segments[1]) {
            Some(name) => name,
            None => continue,
        };

        // Look up the enum type in the registry
        let type_id = match registry.get_by_name(&seg0_name) {
            Some(id) => id,
            None => {
                // Enum type not found: skip silently (this could be a plain function call)
                continue;
            }
        };

        // Find the variant index
        let variants = match registry.get_variants(type_id) {
            Some(v) => v,
            None => {
                // No variants for this type: skip
                continue;
            }
        };

        let variant_index = match variants.iter().position(|(name, _)| name == &seg1_name) {
            Some(idx) => idx as u32,
            None => {
                // Emit T0554 diagnostic: unknown variant in known enum
                if let Some(variant_node) = ast.get(segments[1]) {
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 554) {
                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "Unknown variant '{}' in enum '{}'",
                                seg1_name, seg0_name
                            ))
                            .with_span(variant_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                }
                continue;
            }
        };

        // Get the corresponding IR node ID for this Call
        let ir_call_id = match ast_to_ir.get(&ast_id) {
            Some(id) => *id,
            None => continue,
        };

        // Rewrite the IR node from App to EnumCons
        // We need to mutate the IR node's kind
        if let Some(ir_node) = ir.get_mut(ir_call_id) {
            ir_node.kind = paideia_as_ir::IrKind::EnumCons;
        }

        // Strip the callee (first child) from the children list.
        // App node has children [callee, arg0, arg1, ...], but EnumCons should only have [arg0, arg1, ...]
        if let Some(children) = ir.children_mut(ir_call_id) {
            if !children.is_empty() {
                children.remove(0); // Remove the callee at index 0
            }
        }

        // Insert into enum_cons_info_mut()
        let enum_cons_info = paideia_as_ir::EnumConsInfo { type_id, variant_index };
        ir.enum_cons_info_mut().insert(ir_call_id, enum_cons_info);
    }
}
