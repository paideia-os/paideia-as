//! `ExprRecordCons` child-list canonicalization.
//! Split out of `lower.rs` (2026-07-08); functional core of #1092.
//!
//! When lowering a record literal, canonicalize the field-value order to
//! match the struct's declared field order (looked up via `StructRegistry`).
//! Also emit T0537 (missing field), T0538 (duplicate field), and T0539
//! (unknown field) diagnostics along the way.
//!
//! Falls back to literal order if the type name cannot be resolved or the
//! registry has no declared fields for the type — that keeps
//! `populate_record_layout_table`'s T0552 as the single point of truth for
//! the unknown-type diagnostic.

use paideia_as_ast::{AstArena, NodeId};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap};
use std::collections::HashMap;

use super::text_extract::extract_source_text_for_record_cons;

/// Build the child list `[type_name, field_values...]` for an `ExprRecordCons`
/// node, canonicalized to declared field order.
///
/// `record_cons_span` is the outer node's span, used for the T0537 missing-field
/// diagnostic. `type_name` and `fields` come from the `ExprData::RecordCons`
/// variant.
pub(super) fn record_cons_children(
    ast: &AstArena,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
    registry: &crate::StructRegistry,
    record_cons_span: paideia_as_diagnostics::Span,
    type_name: NodeId,
    fields: &[(NodeId, NodeId)],
) -> Vec<NodeId> {
    // Look up the type name. If we can't extract it, fall back to literal order.
    let type_name_text = match extract_source_text_for_record_cons(ast, source_map, type_name) {
        Some(text) => text,
        None => return literal_order_children(type_name, fields),
    };

    // Registry lookup: if the type isn't there, fall back to literal order.
    // populate_record_layout_table already handles the T0552 "unknown type".
    let type_id = match registry.get_by_name(&type_name_text) {
        Some(id) => id,
        None => return literal_order_children(type_name, fields),
    };

    // Canonicalize path: registry lookup succeeded.
    // 1. Build HashMap<String, (NodeId, NodeId)> from literal fields,
    //    catching duplicates. We store both name_node and value_node
    //    so we can emit diagnostics with proper spans.
    let mut literal_map: HashMap<String, (NodeId, NodeId)> = HashMap::new();
    let mut duplicate_names: Vec<(String, NodeId)> = Vec::new();

    for (name_node, value_node) in fields.iter() {
        let field_name_text = match extract_source_text_for_record_cons(ast, source_map, *name_node) {
            Some(text) => text,
            None => continue, // Skip if we can't extract name
        };

        if literal_map.insert(field_name_text.clone(), (*name_node, *value_node)).is_some() {
            duplicate_names.push((field_name_text, *name_node));
        }
    }

    // Emit T0538 diagnostics for duplicate fields
    for (name, name_node) in duplicate_names {
        if let Some(node) = ast.get(name_node) {
            if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 538) {
                let diag = Diagnostic::error(code)
                    .message(format!(
                        "Record literal contains field '{}' more than once",
                        name
                    ))
                    .with_span(node.span)
                    .finish();
                let _ = sink.emit(diag);
            }
        }
    }

    // 2. Iterate declared fields in order, popping from literal_map
    let mut ordered_values: Vec<NodeId> = Vec::new();
    let mut missing_names: Vec<String> = Vec::new();
    let declared_fields_vec: Vec<(String, u8)> = registry
        .get_fields(type_id)
        .map(|f| f.clone())
        .unwrap_or_default();

    if !declared_fields_vec.is_empty() {
        // Only perform canonicalization if struct has declared fields
        for (decl_name, _byte_code) in declared_fields_vec.iter() {
            match literal_map.remove(decl_name) {
                Some((_name_node, value_node)) => {
                    ordered_values.push(value_node);
                }
                None => {
                    missing_names.push(decl_name.clone());
                }
            }
        }

        // Emit T0537 diagnostics for missing fields
        for name in missing_names {
            if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 537) {
                let diag = Diagnostic::error(code)
                    .message(format!("Record literal omits declared field '{}'", name))
                    .with_span(record_cons_span)
                    .finish();
                let _ = sink.emit(diag);
            }
        }

        // 3. Residual entries in literal_map are unknown fields
        for (unknown_name, (name_node, _value_node)) in literal_map.into_iter() {
            if let Some(node) = ast.get(name_node) {
                if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 539) {
                    let diag = Diagnostic::error(code)
                        .message(format!(
                            "Record literal contains field '{}' not declared in the record type",
                            unknown_name
                        ))
                        .with_span(node.span)
                        .finish();
                    let _ = sink.emit(diag);
                }
            }
        }
    } else {
        // Struct has no declared fields (likely due to registration failure).
        // Use literal order as fallback and don't emit canonicalization diagnostics.
        for (_name_node, value_node) in literal_map.into_values() {
            ordered_values.push(value_node);
        }
    }

    // 4. Assemble IR children: [type_name_node, ordered_values...]
    let mut children = vec![type_name];
    children.extend(ordered_values);
    children
}

fn literal_order_children(type_name: NodeId, fields: &[(NodeId, NodeId)]) -> Vec<NodeId> {
    let mut children = vec![type_name];
    for (_field_name, field_value) in fields {
        children.push(*field_value);
    }
    children
}
