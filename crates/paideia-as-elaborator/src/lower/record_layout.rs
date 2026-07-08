//! `RecordLayoutTable` populator for `ExprRecordCons` nodes.
//! Split out of `lower.rs` (2026-07-08); PA-r17-010a (#1070).
//!
//! For every RecordCons node:
//! 1. Extract the type_name (child[0] of RecordCons per softarch spec).
//! 2. Get source text for that ident.
//! 3. Look up in `StructRegistry.by_name`.
//! 4. If found: insert into `record_layout_table`.
//! 5. If NOT found: emit T0552 diagnostic with the unknown type name.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap};
use paideia_as_ir::{IrArena, IrNodeId};
use std::collections::HashMap;

use super::text_extract::extract_source_text_for_record_cons;

/// Populate the RecordLayoutTable with RecordCons type information.
///
/// PA-r17-010a (#1070): Walk all ExprRecordCons nodes in the AST, look up their
/// struct type name in the registry, and insert the (ir_node_id -> RecordTypeId)
/// mapping into the record_layout_table.
pub(super) fn populate_record_layout_table(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    registry: &crate::StructRegistry,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    // Walk all AST nodes looking for RecordCons expressions
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        // Get the AST node and check if it's an ExprRecordCons
        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        if ast_node.kind != NodeKind::ExprRecordCons {
            continue;
        }

        // Get the ExprData::RecordCons to access type_name
        let type_name_id = match ast.expr_data(ast_id) {
            Some(ExprData::RecordCons { type_name, .. }) => type_name,
            _ => continue,
        };

        // Extract the type name from source
        let type_name_text = match extract_source_text_for_record_cons(ast, source_map, *type_name_id) {
            Some(text) => text,
            None => {
                // Skip if we can't extract the type name
                continue;
            }
        };

        // Get the corresponding IR node ID for this RecordCons
        let ir_record_cons_id = match ast_to_ir.get(&ast_id) {
            Some(id) => *id,
            None => continue,
        };

        // Look up the struct type in the registry
        match registry.get_by_name(&type_name_text) {
            Some(type_id) => {
                // Insert the RecordCons -> RecordTypeId mapping
                ir.record_layout_table_mut().insert(ir_record_cons_id, type_id);
            }
            None => {
                // Emit T0552 diagnostic: unknown struct type
                if let Some(type_name_node) = ast.get(*type_name_id) {
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 552) {
                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "Unsupported struct type: '{}' (not found in struct registry)",
                                type_name_text
                            ))
                            .with_span(type_name_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                }
            }
        }
    }
}
