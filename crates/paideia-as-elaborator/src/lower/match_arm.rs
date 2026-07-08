//! `MatchArmMeta` populator + `match_scrutinee_table` writer.
//! Split out of `lower.rs` (2026-07-08); Phase 7 m9-009 (#1081/#1082).
//!
//! Walks all AST `ExprMatch` nodes and their arms, classifying patterns:
//! - `Wildcard` or `PatIdent` with text "_" → default arm.
//! - `PatIdent` with variant name text → bare-variant no-payload arm.
//! - `PatEnumVariant { path, args }` → variant with optional payload binder.
//! - `PatStruct { path, fields }` → record pattern with nested binding tree.
//! - Other patterns → emit T0556 diagnostic.
//!
//! Requires `enum_registry` (variant names for lookup), `struct_registry`
//! and `payload_map` (nested pattern matching).

use paideia_as_ast::{AstArena, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap};
use paideia_as_ir::{IrArena, IrNodeId};
use std::collections::HashMap;

use super::pattern_data::lower_pattern_data;
use super::text_extract::{build_binding_type_map, extract_source_text_for_record_cons};

/// Phase 7 m9-009 (#1081/#1082): Populate MatchArmMeta side-table for match arms.
pub(super) fn populate_match_arm_meta(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    enum_registry: &crate::EnumRegistry,
    struct_registry: &crate::StructRegistry,
    payload_map: &std::collections::HashMap<
        (paideia_as_ir::enum_layout::EnumTypeId, u32),
        Option<paideia_as_ir::record_layout::RecordTypeId>,
    >,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    // Step 1: Build binding_name → enum_type_text map for enum bindings.
    // Refactor 2026-07-07 Step 7: extracted into build_binding_type_map,
    // shared with populate_field_access_info.
    let binding_to_enum_type = build_binding_type_map(ast, source_map);

    // Step 2: Walk all AST ExprMatch nodes
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        if ast_node.kind != NodeKind::ExprMatch {
            continue;
        }

        // Get ExprData::Match to access scrutinee and arms
        let (scrutinee, arms) = match ast.expr_data(ast_id) {
            Some(paideia_as_ast::ExprData::Match { scrutinee, arms, .. }) => (scrutinee, arms),
            _ => continue,
        };

        // Resolve scrutinee type
        let scrutinee_text = match extract_source_text_for_record_cons(ast, source_map, *scrutinee) {
            Some(text) => text,
            None => continue,
        };

        let enum_type_text = match binding_to_enum_type.get(&scrutinee_text) {
            Some(text) => text.clone(),
            None => continue,
        };

        let type_id = match enum_registry.get_by_name(&enum_type_text) {
            Some(id) => id,
            None => continue,
        };

        let variants = match enum_registry.get_variants(type_id) {
            Some(v) => v,
            None => continue,
        };

        // Get the IR match node
        let match_ir_id = match ast_to_ir.get(&ast_id) {
            Some(id) => *id,
            None => continue,
        };

        // Insert enum type into match_scrutinee_table
        ir.match_scrutinee_table_mut().insert(match_ir_id, type_id);

        // Step 3: Process each arm
        // Get children of the match IR node: [scrutinee, arm_body1, arm_body2, ...]
        let match_children = ir.children(match_ir_id);
        let arm_ir_ids = if !match_children.is_empty() {
            match_children[1..].to_vec()
        } else {
            Vec::new()
        };

        for (arm_idx, arm) in arms.iter().enumerate() {
            // Get the corresponding arm IR id from children
            let arm_ir_id = match arm_ir_ids.get(arm_idx) {
                Some(id) => *id,
                None => continue,
            };

            // Classify the pattern
            let pattern_node = match ast.get(arm.pattern) {
                Some(n) => n,
                None => continue,
            };

            let mut arm_meta = paideia_as_ir::MatchArmMeta::default();

            match pattern_node.kind {
                NodeKind::PatWildcard => {
                    arm_meta.is_default = true;
                }
                NodeKind::PatIdent => {
                    // Extract pattern source text
                    if let Some(pattern_text) =
                        extract_source_text_for_record_cons(ast, source_map, arm.pattern)
                    {
                        if pattern_text == "_" {
                            arm_meta.is_default = true;
                        } else {
                            // Check if this matches a bare variant name
                            if let Some((variant_idx, _)) = variants
                                .iter()
                                .enumerate()
                                .find(|(_, (name, _))| name == &pattern_text)
                            {
                                arm_meta.variant_index = Some(variant_idx as u32);
                            }
                        }
                    }
                }
                NodeKind::PatEnumVariant => {
                    // Extract variant path and arguments
                    if let Some(paideia_as_ast::PatternData::EnumVariant { path, args }) =
                        ast.pattern_data(arm.pattern)
                    {
                        // Get variant name from path source text
                        if let Some(variant_text) =
                            extract_source_text_for_record_cons(ast, source_map, *path)
                        {
                            // Find variant index
                            if let Some((variant_idx, _)) = variants
                                .iter()
                                .enumerate()
                                .find(|(_, (name, _))| name == &variant_text)
                            {
                                arm_meta.variant_index = Some(variant_idx as u32);

                                // Extract payload binder if args.len() == 1 and args[0] is PatIdent
                                if args.len() == 1 {
                                    if let Some(arg_node) = ast.get(args[0]) {
                                        if arg_node.kind == NodeKind::PatIdent {
                                            if let Some(binder_text) =
                                                extract_source_text_for_record_cons(
                                                    ast, source_map, args[0],
                                                )
                                            {
                                                arm_meta.payload_binder = Some(binder_text);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                NodeKind::PatStruct => {
                    // Record pattern support: just note that we have a struct pattern
                    // The actual nested pattern binding tree is handled via lower_pattern_data below
                }
                _ => {
                    // Emit T0556 diagnostic: unsupported match arm pattern kind
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 556) {
                        let diag = Diagnostic::error(code)
                            .message("unsupported match arm pattern kind".to_string())
                            .with_span(pattern_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                    continue;
                }
            }

            // Build the nested pattern binding tree using lower_pattern_data
            if let Some(pattern_binding) = lower_pattern_data(
                arm.pattern,
                ast,
                source_map,
                enum_registry,
                struct_registry,
                payload_map,
                type_id,
            ) {
                arm_meta.pattern_binding = Some(pattern_binding);
            }

            // Insert arm metadata
            ir.match_arm_meta_mut().insert(arm_ir_id, arm_meta);
        }
    }
}
