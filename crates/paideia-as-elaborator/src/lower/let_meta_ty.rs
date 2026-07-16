//! Populator for `LetInfo::ty` field from explicit type annotations.
//!
//! Issue #1219: Populates the `ty: Option<TypeId>` field in LetInfo
//! so that downstream passes like `resolve_let_width` and (planned)
//! `resolve_let_enum_layout` can consume the declared type.
//!
//! Walks all AST nodes and, for each `NodeKind::Let` or `NodeKind::StmtLet`
//! with an explicit type annotation, calls `lower_type_ast` to intern the type
//! and write it to the corresponding IR Let node's LetInfo.
//!
//! Errors (unresolved types) are silently swallowed: T0535 already re-lowers
//! these and would double-emit diagnostics if we pushed to the sink here.

use paideia_as_ast::{AstArena, ItemData, NodeId, NodeKind, StmtData};
use paideia_as_diagnostics::SourceMap;
use paideia_as_ir::{IrArena, IrNodeId, LetInfo};
use paideia_as_ir::monomorphisation::TypeId;
use paideia_as_types::TypeInterner;
use paideia_as_effects::EffectInterner;
use paideia_as_types::CapSetInterner;
use std::collections::HashMap;

/// Populate `LetInfo::ty` from explicit type annotations on Let bindings.
///
/// Walks all AST nodes (1..=ast.len()) and, for each `NodeKind::Let` (item-level)
/// or `NodeKind::StmtLet` (statement-level) with an explicit type annotation:
/// 1. Looks up the IR node ID via `ast_to_ir`.
/// 2. Calls `lower_type_ast` on the annotation subtree.
/// 3. On success: uses read-modify-write to preserve existing fields and write `ty`.
/// 4. On error: silently drops the diagnostic (swallow Err).
///
/// # Arguments
///
/// * `ast` - The AST arena
/// * `ir` - The IR arena (mutable for updating let_meta)
/// * `ast_to_ir` - Mapping from AST node IDs to IR node IDs
/// * `source_map` - Source map for type lowering
/// * `types` - Type interner (mutable for interning)
/// * `effects` - Effect interner (mutable for interning)
/// * `caps` - Capability set interner (mutable for interning)
/// * `registry` - Struct registry for type lookup
pub fn populate_let_meta_ty(
    ast: &AstArena,
    ir: &mut IrArena,
    ast_to_ir: &HashMap<NodeId, IrNodeId>,
    source_map: &SourceMap,
    types: &mut TypeInterner,
    effects: &mut EffectInterner,
    caps: &mut CapSetInterner,
    registry: &crate::StructRegistry,
) {
    // Walk all AST nodes
    for i in 1..=ast.len() {
        let Some(ast_id) = NodeId::new(i as u32) else { continue };
        let Some(node) = ast.get(ast_id) else { continue };

        // Check for NodeKind::Let (item-level)
        let type_annotation = if node.kind == NodeKind::Let {
            if let Some(ItemData::Let { ty: Some(ty_node), .. }) = ast.item_data(ast_id) {
                Some(*ty_node)
            } else {
                None
            }
        } else if node.kind == NodeKind::StmtLet {
            // Check for NodeKind::StmtLet (statement-level)
            if let Some(StmtData::Let { ty: Some(ty_node), .. }) = ast.stmt_data(ast_id) {
                Some(*ty_node)
            } else {
                None
            }
        } else {
            None
        };

        // If there's no explicit type annotation, skip this Let
        let Some(ty_node) = type_annotation else { continue };

        // Look up the IR node ID for this Let
        let Some(let_ir_id) = ast_to_ir.get(&ast_id) else { continue };

        // Try to lower the type annotation
        match super::super::lower_type::lower_type_ast(
            ast,
            source_map,
            ty_node,
            types,
            effects,
            caps,
            registry,
        ) {
            Ok(types_tid) => {
                // Success: read-modify-write to preserve existing LetInfo fields
                let mut info = ir
                    .let_meta()
                    .get(*let_ir_id)
                    .cloned()
                    .unwrap_or_else(LetInfo::immutable);
                info.ty = Some(TypeId(types_tid.get()));
                ir.let_meta_mut().insert(*let_ir_id, info);
            }
            Err(_) => {
                // Error: silently swallow the diagnostic.
                // Reason: T0535 already re-lowers these type annotations and would double-emit.
                // We drop the Err diagnostic list without pushing to sink.
            }
        }
    }
}
