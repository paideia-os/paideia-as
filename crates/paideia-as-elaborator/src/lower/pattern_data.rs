//! Recursive PatternBinding builder for match arms.
//! Split out of `lower.rs` (2026-07-08); part of #1053.
//!
//! Converts an AST pattern node into a nested `PatternBinding` structure
//! representing the full pattern hierarchy (e.g., `Ok(Point { x, y })`).
//! Used by `populate_match_arm_meta` to build the `pattern_binding` field
//! of `MatchArmMeta`.
//!
//! Scope: cross-enum nested patterns (e.g. `Ok(Some(x))` where `Some` lives
//! in a different enum) are not supported — the recursive variant-name
//! lookup reuses the outer scrutinee's enum, so a mismatch silently yields
//! `payload=None`.

use paideia_as_ast::{AstArena, NodeId, NodeKind};
use paideia_as_diagnostics::SourceMap;

use super::text_extract::extract_source_text_for_record_cons;

/// Recursive helper for building nested PatternBinding trees from match patterns.
///
/// # Arguments
/// - `pat_id`: The AST pattern node to lower
/// - `ast`: The AST arena
/// - `source_map`: Source map for extracting text
/// - `enum_registry`: For resolving enum types and variant indices
/// - `struct_registry`: For resolving struct types
/// - `payload_map`: HashMap from (EnumTypeId, variant_idx) to Option<RecordTypeId>
/// - `scrutinee_enum_id`: The enum type of the match scrutinee (for type checking)
///
/// # Returns
/// Some(PatternBinding) on success, None if pattern cannot be lowered
/// (e.g., nested EnumVariant with non-scrutinee enum type).
pub(super) fn lower_pattern_data(
    pat_id: NodeId,
    ast: &AstArena,
    source_map: &SourceMap,
    enum_registry: &crate::EnumRegistry,
    struct_registry: &crate::StructRegistry,
    payload_map: &std::collections::HashMap<
        (paideia_as_ir::enum_layout::EnumTypeId, u32),
        Option<paideia_as_ir::record_layout::RecordTypeId>,
    >,
    scrutinee_enum_id: paideia_as_ir::enum_layout::EnumTypeId,
) -> Option<paideia_as_ir::enum_layout::PatternBinding> {
    use paideia_as_ir::enum_layout::PatternBinding;

    let pattern_node = ast.get(pat_id)?;

    // Issue #1002: Unwrap PatBinding transparently
    let (actual_node, actual_pat_id) = if pattern_node.kind == NodeKind::PatBinding {
        if let Some(paideia_as_ast::PatternData::Binding { inner, .. }) = ast.pattern_data(pat_id) {
            if let Some(inner_node) = ast.get(*inner) {
                (inner_node, *inner)
            } else {
                return None;
            }
        } else {
            return None;
        }
    } else {
        (pattern_node, pat_id)
    };

    match actual_node.kind {
        NodeKind::PatWildcard => Some(PatternBinding::Wildcard),

        NodeKind::PatIdent => {
            // Extract the identifier text
            let text = extract_source_text_for_record_cons(ast, source_map, actual_pat_id)?;
            if text == "_" {
                Some(PatternBinding::Wildcard)
            } else {
                Some(PatternBinding::Simple(text))
            }
        }

        NodeKind::PatEnumVariant => {
            // Extract variant path and arguments
            let (path_id, args) = match ast.pattern_data(actual_pat_id) {
                Some(paideia_as_ast::PatternData::EnumVariant { path, args }) => (*path, args.clone()),
                _ => return None,
            };

            // Get variant name from path
            let variant_text = extract_source_text_for_record_cons(ast, source_map, path_id)?;

            // Resolve variant index in the scrutinee enum
            let variants = enum_registry.get_variants(scrutinee_enum_id)?;
            let (variant_idx, (_, _)) = variants
                .iter()
                .enumerate()
                .find(|(_, (name, _))| name == &variant_text)?;
            let variant_idx = variant_idx as u32;

            // Get payload type from the payload_map if present
            let payload_type = payload_map.get(&(scrutinee_enum_id, variant_idx)).copied().flatten();

            // Scope: cross-enum nested patterns (e.g. Ok(Some(x)) where Some is a different enum) are not supported —
            // the recursive variant-name lookup reuses the outer scrutinee's enum, so a mismatch silently yields payload=None.
            // If there's one argument, try to lower it recursively
            let payload_binding = if args.len() == 1 {
                // For single payload, recursively lower the argument
                lower_pattern_data(
                    args[0],
                    ast,
                    source_map,
                    enum_registry,
                    struct_registry,
                    payload_map,
                    scrutinee_enum_id,
                )
                .map(Box::new)
            } else if args.is_empty() {
                None
            } else {
                // Multiple arguments: not supported yet
                None
            };

            Some(PatternBinding::EnumVariant {
                variant_index: variant_idx,
                payload_type,
                payload: payload_binding,
            })
        }

        NodeKind::PatStruct => {
            // Extract struct path and fields
            let (struct_path_id, fields) = match ast.pattern_data(actual_pat_id) {
                Some(paideia_as_ast::PatternData::Struct { path, fields }) => (*path, fields.clone()),
                _ => return None,
            };

            // Get struct name and resolve to RecordTypeId
            let struct_text = extract_source_text_for_record_cons(ast, source_map, struct_path_id)?;
            let record_type_id = struct_registry.get_by_name(&struct_text)?;

            // Lower each field pattern recursively
            let mut field_bindings = Vec::new();
            for field in fields {
                let field_name = extract_source_text_for_record_cons(ast, source_map, field.name)?;
                let field_pattern = lower_pattern_data(
                    field.pattern,
                    ast,
                    source_map,
                    enum_registry,
                    struct_registry,
                    payload_map,
                    scrutinee_enum_id,
                )?;
                field_bindings.push((field_name, field_pattern));
            }

            Some(PatternBinding::Record {
                type_id: record_type_id,
                fields: field_bindings,
            })
        }

        _ => None,
    }
}
