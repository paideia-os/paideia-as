//! `FieldAccessSideTable` populator for `ExprFieldAccess` nodes.
//! Split out of `lower.rs` (2026-07-08); Phase 6 m3-002 (#1073).
//!
//! For each FieldAccess node:
//! 1. Extract the receiver's name (for Ident/ExprPath only; skip computed exprs).
//! 2. Look up the binding in the binding→struct_type map.
//! 3. Look up the struct type in `StructRegistry.get_by_name`.
//! 4. Find the field index matching the field name.
//! 5. Insert into `field_access_info_mut()`.
//!
//! Non-blocking: silently skips if the receiver is computed (not in the
//! binding map). Emits T0552 on unknown struct type or unknown field.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, SourceMap};
use paideia_as_ir::{IrArena, IrNodeId};
use std::collections::HashMap;

use super::text_extract::{build_binding_type_map, extract_source_text_for_record_cons};

/// Populate the FieldAccessSideTable with FieldAccess type and field index information.
pub(super) fn populate_field_access_info(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    registry: &crate::StructRegistry,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
) {
    // Step 1: Build binding_name → struct_type_text map.
    // Refactor 2026-07-07 Step 7: extracted into build_binding_type_map,
    // shared with populate_match_arm_meta.
    let binding_to_struct_type = build_binding_type_map(ast, source_map);

    // Step 2: Walk all AST nodes with NodeKind::ExprFieldAccess
    for ast_node_id in 1..=ast.len() {
        let ast_id = match NodeId::new(ast_node_id as u32) {
            Some(nid) => nid,
            None => continue,
        };

        let ast_node = match ast.get(ast_id) {
            Some(n) => n,
            None => continue,
        };

        if ast_node.kind != NodeKind::ExprFieldAccess {
            continue;
        }

        // Get the ExprData::FieldAccess to access receiver and field
        let (receiver_id, field_id) = match ast.expr_data(ast_id) {
            Some(ExprData::FieldAccess { receiver, field }) => (receiver, field),
            _ => continue,
        };

        // #1146 follow-up: unwrap a single level of Deref so `(*p).field`
        // resolves against the pointee's declared type instead of trying
        // (and failing) to match the raw `*p` source text against the
        // binding map, which is keyed on the bare identifier `p`.
        let (receiver_name_id, receiver_is_deref) = match ast.expr_data(*receiver_id) {
            Some(ExprData::Deref { expr }) => (*expr, true),
            _ => (*receiver_id, false),
        };

        // Extract receiver name (only simple Ident/ExprPath, not computed expressions)
        let receiver_name =
            match extract_source_text_for_record_cons(ast, source_map, receiver_name_id) {
                Some(name) => name,
                None => {
                    // Receiver is computed or we can't extract text: skip silently
                    continue;
                }
            };

        // Extract field name
        let field_name = match extract_source_text_for_record_cons(ast, source_map, *field_id) {
            Some(name) => name,
            None => {
                // Skip if we can't extract field name
                continue;
            }
        };

        // Look up the struct type for this receiver
        let struct_type_text = match binding_to_struct_type.get(&receiver_name) {
            Some(text) => text.clone(),
            None => {
                // Receiver not in binding map: skip silently (non-blocking failure mode)
                continue;
            }
        };

        // #1146 follow-up: for a Deref receiver, the binding map holds the
        // declared *pointer* type text (e.g. "*Point", from `p: *Point`).
        // Strip the pointer sigil to recover the pointee's struct name
        // before looking it up in the struct registry, which is keyed on
        // bare struct names.
        let struct_type_text = if receiver_is_deref {
            struct_type_text.trim_start_matches('*').trim().to_string()
        } else {
            struct_type_text
        };

        // Look up the struct type in the registry
        let type_id = match registry.get_by_name(&struct_type_text) {
            Some(id) => id,
            None => {
                // Emit T0552 diagnostic: unknown struct type
                if let Some(receiver_node) = ast.get(*receiver_id) {
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 552) {
                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "Unsupported struct type: '{}' (not found in struct registry)",
                                struct_type_text
                            ))
                            .with_span(receiver_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                }
                continue;
            }
        };

        // Find the field index
        let fields = match registry.get_fields(type_id) {
            Some(f) => f,
            None => {
                // No fields for this type: skip
                continue;
            }
        };

        let field_index = match fields.iter().position(|(name, _)| name == &field_name) {
            Some(idx) => idx as u32,
            None => {
                // Emit T0552 diagnostic: unknown field
                if let Some(field_node) = ast.get(*field_id) {
                    if let Ok(code) = DiagnosticCode::new(Category::T, Severity::Error, 552) {
                        let diag = Diagnostic::error(code)
                            .message(format!(
                                "Unknown field '{}' in struct '{}'",
                                field_name, struct_type_text
                            ))
                            .with_span(field_node.span)
                            .finish();
                        let _ = sink.emit(diag);
                    }
                }
                continue;
            }
        };

        // Get the corresponding IR node ID for this FieldAccess
        let ir_field_access_id = match ast_to_ir.get(&ast_id) {
            Some(id) => *id,
            None => continue,
        };

        // Insert into field_access_info_mut()
        let field_access_info = paideia_as_ir::FieldAccessInfo { type_id, field_index };
        ir.field_access_info_mut().insert(ir_field_access_id, field_access_info);
    }
}
