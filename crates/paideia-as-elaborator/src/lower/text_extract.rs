//! Source-text extraction and binding-type map used across the AST→IR
//! populators. Split out of `lower.rs` (2026-07-08).
//!
//! `extract_source_text_for_record_cons` reads the byte range from a
//! `SourceMap` for a given node's span; every populator that needs to
//! identify a struct/enum/binding by name goes through it.
//!
//! `build_binding_type_map` walks the AST once and collects
//! `binding_name → declared_type_text` for every item-level or statement-
//! level `let` with an explicit type annotation. Consumers filter by
//! their own registry (`StructRegistry` for field access, `EnumRegistry`
//! for match arms).

use paideia_as_ast::{AstArena, NodeId, NodeKind};
use paideia_as_diagnostics::SourceMap;
use std::collections::HashMap;

/// Extract source text for a RecordCons type_name node.
///
/// Similar to extract_source_text but returns a String instead of Option<String>
/// for better ergonomics in the populate_record_layout_table context.
pub(super) fn extract_source_text_for_record_cons(
    ast: &AstArena,
    source_map: &SourceMap,
    node_id: NodeId,
) -> Option<String> {
    let node = ast.get(node_id)?;
    let span = node.span;
    let file_id = span.file();
    let source = source_map.content(file_id);

    let start = span.byte_start() as usize;
    let len = span.byte_len() as usize;
    if start + len > source.len() {
        return None;
    }

    let text = &source[start..start + len];
    Some(text.to_string())
}

/// Build the `binding_name → declared_type_text` map used by multiple
/// populators (`populate_field_access_info`, `populate_match_arm_meta`).
///
/// Walks the AST once and collects every `NodeKind::Let` (item-level) and
/// `NodeKind::StmtLet` (statement-level) binding that has an explicit type
/// annotation. Keyed on the source text of the binding name; valued on the
/// source text of the declared type. Downstream populators filter the map
/// against their own registry (StructRegistry for field access,
/// EnumRegistry for match arms).
///
/// Refactor 2026-07-07 Step 7: extracted from duplicated Step-1 walks that
/// previously lived at `populate_field_access_info` and
/// `populate_match_arm_meta`. Retires the "update one, forget the other"
/// hazard for any future change to how binding types are extracted.
pub(super) fn build_binding_type_map(ast: &AstArena, source_map: &SourceMap) -> HashMap<String, String> {
    let mut binding_to_type = HashMap::new();

    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        // Process item-level Let bindings (ItemData::Let).
        if ast_node.kind == NodeKind::Let {
            if let Some(paideia_as_ast::ItemData::Let { name, ty, .. }) = ast.item_data(ast_id) {
                if let Some(ty_node_id) = ty {
                    if let Some(type_text) =
                        extract_source_text_for_record_cons(ast, source_map, *ty_node_id)
                    {
                        if let Some(binding_name) =
                            extract_source_text_for_record_cons(ast, source_map, *name)
                        {
                            binding_to_type.insert(binding_name, type_text);
                        }
                    }
                }
            }
        }

        // Process statement-level Let bindings (StmtData::Let).
        if ast_node.kind == NodeKind::StmtLet {
            if let Some(paideia_as_ast::StmtData::Let { name, ty, .. }) = ast.stmt_data(ast_id) {
                if let Some(ty_node_id) = ty {
                    if let Some(type_text) =
                        extract_source_text_for_record_cons(ast, source_map, *ty_node_id)
                    {
                        if let Some(binding_name) =
                            extract_source_text_for_record_cons(ast, source_map, *name)
                        {
                            binding_to_type.insert(binding_name, type_text);
                        }
                    }
                }
            }
        }
    }

    binding_to_type
}
