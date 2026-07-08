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

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
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

        // Lambda parameter type hints (allows match scrutinee resolution for
        // `fn(t: Traffic) -> match t {...}`).
        if ast_node.kind == NodeKind::ExprLambda {
            if let Some(ExprData::Lambda { params, .. }) = ast.expr_data(ast_id) {
                for &pat_id in params {
                    // Get the pattern's declared type via pattern_type_hints side table.
                    if let Some(type_node_id) = ast.pattern_type_hints().get(pat_id) {
                        // Extract param name from PatIdent (skip complex patterns).
                        if ast.get(pat_id).map(|n| n.kind) == Some(NodeKind::PatIdent) {
                            let name_text = extract_source_text_for_record_cons(ast, source_map, pat_id);
                            let type_text =
                                extract_source_text_for_record_cons(ast, source_map, type_node_id);
                            if let (Some(n), Some(t)) = (name_text, type_text) {
                                binding_to_type.insert(n, t);
                            }
                        }
                    }
                }
            }
        }
    }

    binding_to_type
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn make_test_span(file_id: FileId, offset: u32, len: u32) -> paideia_as_diagnostics::Span {
        paideia_as_diagnostics::Span::new(file_id, offset, len)
    }

    #[test]
    fn build_binding_type_map_recognizes_lambda_params() {
        // Build AST: fn(t: Traffic) -> ...
        // Set pattern_type_hints[pat_t] = type_traffic
        // Call build_binding_type_map(&ast, &source_map)
        // Assert map.get("t") == Some(&"Traffic".to_string())

        let mut source_map = SourceMap::new();
        let source_text = "t Traffic";
        let file = source_map.add_file(
            std::path::PathBuf::from("test.pdx"),
            String::from(source_text),
        );
        let file_id = file;

        let mut ast = AstArena::new();

        // Create pattern node for "t" (PatIdent)
        let t_pattern_id = ast.alloc(NodeKind::PatIdent, make_test_span(file_id, 0, 1)); // "t"

        // Create type node for "Traffic"
        let traffic_type_id = ast.alloc(NodeKind::Ident, make_test_span(file_id, 2, 7)); // "Traffic"

        // Register the pattern → type mapping via pattern_type_hints
        ast.pattern_type_hints_mut()
            .insert(t_pattern_id, traffic_type_id);

        // Create lambda expression: fn(t: Traffic) -> body
        let body_id = ast.alloc(NodeKind::ExprLiteral, make_test_span(file_id, 0, 1));
        let lambda_id = ast.alloc_expr(
            NodeKind::ExprLambda,
            make_test_span(file_id, 0, 1),
            ExprData::Lambda {
                generic_params: vec![],
                params: vec![t_pattern_id],
                body: body_id,
                pipe_form: false,
            },
        );

        // Call build_binding_type_map
        let map = build_binding_type_map(&ast, &source_map);

        // Assert map contains the lambda parameter binding
        assert_eq!(
            map.get("t"),
            Some(&"Traffic".to_string()),
            "Lambda param 't' should be mapped to type 'Traffic'"
        );
    }
}
