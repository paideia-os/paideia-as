//! Pre-pass that collects Loop/While AST nodes reachable from inside an
//! `unsafe { … }` block. Split out of `lower.rs` (2026-07-08).
//!
//! Used so that the second-pass child transfer can skip attaching children
//! to Loop/While nodes that live inside unsafe blocks — the `unsafe_walker`
//! processes their statements independently and would conflict with the
//! usual control-flow child layout (see PA-R17-012c / #990).

use paideia_as_ast::{AstArena, NodeId, NodeKind};
use std::collections::HashSet;

/// Walk the AST from every `ExprUnsafe` root and record the raw `NodeId`
/// (as `u32`) of every `ExprLoop` reachable through the unsafe block's
/// statement graph.
pub(super) fn collect_nodes_in_unsafe_blocks(ast: &AstArena) -> HashSet<u32> {
    let mut nodes_in_unsafe_blocks = HashSet::new();

    for i in 0..ast.len() {
        let ast_id = NodeId::new((i + 1) as u32).expect("non-zero node id");
        if let Some(node) = ast.get(ast_id) {
            if node.kind == NodeKind::ExprUnsafe {
                if let Some(paideia_as_ast::ExprData::Unsafe { block, .. }) =
                    ast.expr_data(ast_id)
                {
                    // Recursively collect While/Loop nodes inside this unsafe block
                    for &stmt_id in block {
                        collect_unsafe_descendants(ast, stmt_id, &mut nodes_in_unsafe_blocks);
                    }
                }
            }
        }
    }

    nodes_in_unsafe_blocks
}

fn collect_unsafe_descendants(ast: &AstArena, node_id: NodeId, dest: &mut HashSet<u32>) {
    if let Some(node) = ast.get(node_id) {
        // If this is a While/Loop node, mark it
        if node.kind == NodeKind::ExprLoop {
            dest.insert(node_id.get());
        }

        // Recursively check children of the current node
        if let Some(stmt_data) = ast.stmt_data(node_id) {
            match stmt_data {
                paideia_as_ast::StmtData::Let { value, .. } => {
                    collect_unsafe_descendants(ast, *value, dest);
                }
                paideia_as_ast::StmtData::Expr { expr } => {
                    collect_unsafe_descendants(ast, *expr, dest);
                }
                _ => {}
            }
        }

        if let Some(expr_data) = ast.expr_data(node_id) {
            match expr_data {
                paideia_as_ast::ExprData::Loop { body, .. } => {
                    collect_unsafe_descendants(ast, *body, dest);
                }
                paideia_as_ast::ExprData::If { cond, then_block, else_block } => {
                    collect_unsafe_descendants(ast, *cond, dest);
                    collect_unsafe_descendants(ast, *then_block, dest);
                    if let Some(else_id) = else_block {
                        collect_unsafe_descendants(ast, *else_id, dest);
                    }
                }
                paideia_as_ast::ExprData::Match { scrutinee, arms, .. } => {
                    collect_unsafe_descendants(ast, *scrutinee, dest);
                    for arm in arms {
                        collect_unsafe_descendants(ast, arm.body, dest);
                    }
                }
                _ => {}
            }
        }
    }
}
