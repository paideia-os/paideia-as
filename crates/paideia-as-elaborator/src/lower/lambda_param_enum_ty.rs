//! Populator for lambda parameter enum types (#1156).
//!
//! Issue #1156 (receiver-side): Populates the lambda_param_enum_types side-table
//! for lambda parameters with explicitly declared enum types. This enables
//! register_nested_lambda_params to detect enum-typed pos-0 parameters and
//! install (RAX, RDX) pair bindings instead of scalar RDI binding.
//!
//! Walks all AST nodes and, for each `NodeKind::ExprLambda` with explicitly
//! typed parameters, checks if the parameter's type resolves to a known enum.
//! On match, inserts (lambda_ir_id, param_index) -> enum_type_id.
//!
//! Errors (unresolved types) are silently swallowed: type checking already
//! validates parameters elsewhere.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind, TypeData};
use paideia_as_ir::IrArena;
use paideia_as_diagnostics::SourceMap;
use std::collections::HashMap;

use crate::EnumRegistry;

/// Populate lambda_param_enum_types from explicit type annotations on lambda parameters.
///
/// Walks all AST nodes (1..=ast.len()) and, for each `NodeKind::ExprLambda`
/// with an explicit type annotation on a parameter:
/// 1. Looks up the IR node ID via `ast_to_ir`.
/// 2. For each parameter with a type annotation, checks if the type name
///    resolves to a registered enum.
/// 3. On match: inserts (lambda_ir_id, param_index) -> enum_type_id.
/// 4. On mismatch: silently skips (not an enum or unresolved).
///
/// # Arguments
///
/// * `ast` - The AST arena
/// * `ir` - The IR arena (mutable for updating lambda_param_enum_types)
/// * `ast_to_ir` - Mapping from AST node IDs to IR node IDs
/// * `source_map` - Source map for text extraction
/// * `enum_registry` - Enum registry for enum type lookup
pub fn populate_lambda_param_enum_types(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, paideia_as_ir::IrNodeId>,
    source_map: &SourceMap,
    enum_registry: &EnumRegistry,
) {
    // Walk all AST nodes
    for i in 1..=ast.len() {
        let Some(ast_id) = NodeId::new(i as u32) else { continue };
        let Some(node) = ast.get(ast_id) else { continue };

        // Check for NodeKind::ExprLambda
        if node.kind != NodeKind::ExprLambda {
            continue;
        }

        let Some(ExprData::Lambda { params, .. }) = ast.expr_data(ast_id) else {
            continue;
        };

        // Look up the IR node ID for this Lambda
        let Some(&lambda_ir_id) = ast_to_ir.get(&ast_id) else {
            continue;
        };

        // Iterate over parameters by index
        for (idx, &pat_id) in params.iter().enumerate() {
            // Try to get type hint for this parameter
            let Some(ty_node_id) = ast.pattern_type_hints().get(pat_id) else {
                continue;
            };

            // Extract type name from TypeData::Name
            let Some(TypeData::Name { name, .. }) = ast.type_data(ty_node_id) else {
                continue;
            };

            // Extract text of the name identifier
            let Ok(name_text) = super::super::lower_type::get_ident_text(ast, *name, source_map) else {
                continue;
            };

            // Look up enum in registry
            if let Some(eid) = enum_registry.get_by_name(&name_text) {
                ir.lambda_param_enum_types_mut()
                    .insert((lambda_ir_id, idx as u32), eid);
            }
        }
    }
}
