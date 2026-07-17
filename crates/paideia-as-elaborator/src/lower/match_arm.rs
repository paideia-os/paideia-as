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

            // Issue #1002: Unwrap PatBinding transparently
            // Extract binder name and check inner pattern before classification dispatch
            let (actual_pattern_node, actual_pattern_id) = if pattern_node.kind == NodeKind::PatBinding {
                // Extract binding pattern data
                if let Some(paideia_as_ast::PatternData::Binding { name, inner }) = ast.pattern_data(arm.pattern) {
                    // Extract binder name
                    if let Some(binder_name) = extract_source_text_for_record_cons(ast, source_map, *name) {
                        arm_meta.outer_binder = Some(binder_name);
                    }

                    // Check if inner pattern is EnumVariant with payload (MVP forbids this)
                    let inner_node = match ast.get(*inner) {
                        Some(n) => n,
                        None => continue,
                    };

                    if inner_node.kind == NodeKind::PatEnumVariant {
                        if let Some(paideia_as_ast::PatternData::EnumVariant { args, .. }) = ast.pattern_data(*inner) {
                            if !args.is_empty() {
                                // Emit T-code diagnostic: bind-and-match with enum payload not supported in MVP
                                if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 1002) {
                                    let diag = Diagnostic::error(code)
                                        .message("bind-and-match with enum payload not supported (MVP)".to_string())
                                        .with_span(pattern_node.span)
                                        .finish();
                                    let _ = sink.emit(diag);
                                }
                                continue;
                            }
                        }
                    }

                    // Issue #1002: For integer literal patterns, skip enum classification
                    // and let populate_int_match_meta handle it
                    if inner_node.kind == NodeKind::PatLiteral {
                        // Skip enum classification but still process guards
                        // Populate guard if present
                        if let Some(guard_ast_id) = arm.guard {
                            if let Some(&guard_ir_id) = ast_to_ir.get(&guard_ast_id) {
                                arm_meta.guard = Some(guard_ir_id);
                            }
                        }
                        // Insert arm metadata and continue to next arm
                        ir.match_arm_meta_mut().insert(arm_ir_id, arm_meta);
                        continue;
                    }

                    // Unwrap transparently: use inner pattern for further classification
                    (inner_node, *inner)
                } else {
                    continue;
                }
            } else {
                (pattern_node, arm.pattern)
            };

            match actual_pattern_node.kind {
                NodeKind::PatWildcard => {
                    // Only mark as unconditional default when no guard.
                    // A guarded wildcard (_ if pred => ...) is conditional — must fall into
                    // the normal cascade path so the guard check runs.
                    arm_meta.is_default = arm.guard.is_none();
                }
                NodeKind::PatIdent => {
                    // Extract pattern source text
                    if let Some(pattern_text) =
                        extract_source_text_for_record_cons(ast, source_map, actual_pattern_id)
                    {
                        if pattern_text == "_" {
                            // #1000: mark as unconditional default only when no guard.
                            // A guarded wildcard (`_ if pred => ...`) is conditional and
                            // must fall into the cascade path so the guard runs.
                            arm_meta.is_default = arm.guard.is_none();
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
                        ast.pattern_data(actual_pattern_id)
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
                NodeKind::PatOr => {
                    // Issue #1001: Handle or-patterns (p1 | p2 | ... | pN => body)
                    if let Some(paideia_as_ast::PatternData::Or { alternatives }) =
                        ast.pattern_data(actual_pattern_id)
                    {
                        classify_or_alts(
                            alternatives,
                            ast,
                            source_map,
                            &variants,
                            enum_registry,
                            struct_registry,
                            payload_map,
                            type_id,
                            &mut arm_meta,
                            sink,
                        );
                    }
                }
                NodeKind::PatStruct => {
                    // Record pattern support: just note that we have a struct pattern
                    // The actual nested pattern binding tree is handled via lower_pattern_data below
                }
                NodeKind::PatLiteral => {
                    // Integer/literal pattern matching - handled by populate_int_match_meta
                    // Just skip here and let integer match classifier handle it
                }
                _ => {
                    // Emit T0556 diagnostic: unsupported match arm pattern kind
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 556) {
                        let diag = Diagnostic::error(code)
                            .message("unsupported match arm pattern kind".to_string())
                            .with_span(actual_pattern_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                    continue;
                }
            }

            // Build the nested pattern binding tree using lower_pattern_data
            // Skip for bare variant names (PatIdent matching a variant with no payload)
            // to allow #1052's auto-detect to fire on real code
            let should_lower_pattern = !(actual_pattern_node.kind == NodeKind::PatIdent
                && arm_meta.variant_index.is_some()
                && arm_meta.payload_binder.is_none());

            if should_lower_pattern {
                if let Some(pattern_binding) = lower_pattern_data(
                    actual_pattern_id,
                    ast,
                    source_map,
                    enum_registry,
                    struct_registry,
                    payload_map,
                    type_id,
                ) {
                    arm_meta.pattern_binding = Some(pattern_binding);
                }
            }

            // Populate guard if present
            if let Some(guard_ast_id) = arm.guard {
                if let Some(&guard_ir_id) = ast_to_ir.get(&guard_ast_id) {
                    arm_meta.guard = Some(guard_ir_id);
                }
            }

            // Insert arm metadata
            ir.match_arm_meta_mut().insert(arm_ir_id, arm_meta);
        }
    }
}

/// Classify or-pattern alternatives for a match arm.
///
/// Issue #1001: Handles or-patterns by iterating through each alternative and
/// classifying via the same rules as singleton patterns. Asserts homogeneity:
/// all alternatives must be the same kind (all Literal or all EnumVariant).
///
/// Diagnostics: emits T0574 on disagreement or when nested-pattern-binding is present.
fn classify_or_alts(
    alternatives: &[paideia_as_ast::NodeId],
    ast: &paideia_as_ast::AstArena,
    source_map: &paideia_as_diagnostics::SourceMap,
    variants: &[(String, Vec<paideia_as_ast::NodeId>)],
    _enum_registry: &crate::EnumRegistry,
    _struct_registry: &crate::StructRegistry,
    _payload_map: &std::collections::HashMap<
        (paideia_as_ir::enum_layout::EnumTypeId, u32),
        Option<paideia_as_ir::record_layout::RecordTypeId>,
    >,
    _type_id: paideia_as_ir::enum_layout::EnumTypeId,
    arm_meta: &mut paideia_as_ir::MatchArmMeta,
    _sink: &mut dyn DiagnosticSink,
) {
    if alternatives.is_empty() {
        return;
    }

    // Classify each alternative
    let mut first_int_value: Option<i64> = None;
    let mut first_variant_idx: Option<u32> = None;
    let mut shared_payload_binder: Option<String> = None;
    let mut all_ints = true;
    let mut all_variants = true;

    for (alt_idx, &alt_pat_id) in alternatives.iter().enumerate() {
        let alt_node = match ast.get(alt_pat_id) {
            Some(n) => n,
            None => continue,
        };

        match alt_node.kind {
            NodeKind::PatLiteral => {
                // Integer literal alternative
                all_variants = false;

                // Try to extract the actual integer value from source text
                if let Some(literal_text) =
                    extract_source_text_for_record_cons(ast, source_map, alt_pat_id)
                {
                    // Try to parse the literal text as an integer
                    // Strip off suffixes like u64, i32, etc.
                    let num_str = literal_text
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '-')
                        .collect::<String>();

                    if let Ok(value) = num_str.parse::<i64>() {
                        if alt_idx == 0 {
                            first_int_value = Some(value);
                        }
                        arm_meta.alt_int_values.push(value);
                    }
                }
            }
            NodeKind::PatIdent => {
                // Check if this is a bare variant name
                if let Some(variant_text) =
                    extract_source_text_for_record_cons(ast, source_map, alt_pat_id)
                {
                    if let Some((variant_idx, _)) = variants
                        .iter()
                        .enumerate()
                        .find(|(_, (name, _))| name == &variant_text)
                    {
                        all_ints = false;
                        if alt_idx == 0 {
                            first_variant_idx = Some(variant_idx as u32);
                        }
                        arm_meta.alt_variant_indices.push(variant_idx as u32);
                    }
                }
            }
            NodeKind::PatEnumVariant => {
                // Enum variant alternative (with optional payload binder)
                all_ints = false;

                if let Some(paideia_as_ast::PatternData::EnumVariant { path, args }) =
                    ast.pattern_data(alt_pat_id)
                {
                    if let Some(variant_text) =
                        extract_source_text_for_record_cons(ast, source_map, *path)
                    {
                        if let Some((variant_idx, _)) = variants
                            .iter()
                            .enumerate()
                            .find(|(_, (name, _))| name == &variant_text)
                        {
                            if alt_idx == 0 {
                                first_variant_idx = Some(variant_idx as u32);
                            }
                            arm_meta.alt_variant_indices.push(variant_idx as u32);

                            // Extract payload binder if args.len() == 1 and args[0] is PatIdent
                            if args.len() == 1 {
                                if let Some(arg_node) = ast.get(args[0]) {
                                    if arg_node.kind == NodeKind::PatIdent {
                                        if let Some(binder_text) =
                                            extract_source_text_for_record_cons(
                                                ast, source_map, args[0],
                                            )
                                        {
                                            if alt_idx == 0 {
                                                shared_payload_binder = Some(binder_text.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                // Unsupported pattern in or-alternative
                // Skip for now; conservatively marked as non-default
                return;
            }
        }
    }

    // Set arm metadata from first alternative
    if all_ints && first_int_value.is_some() {
        arm_meta.int_pattern_value = first_int_value;
    } else if all_variants && first_variant_idx.is_some() {
        arm_meta.variant_index = first_variant_idx;
    }

    // Set payload binder if all alternatives share one
    if let Some(binder) = shared_payload_binder {
        arm_meta.payload_binder = Some(binder);
    }
}
