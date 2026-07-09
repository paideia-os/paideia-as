//! Second-pass AST → IR child extraction.
//! Split out of `lower.rs` (2026-07-08).
//!
//! Given an already-allocated pair of AST/IR nodes, this module reads the
//! AST node's variant-specific children and returns them as a `Vec<NodeId>`
//! for the caller to remap into IR ids and stitch onto the IR node.
//!
//! Complex per-kind logic (RecordCons canonicalization, Store rearrangement,
//! ArrayRepeat expansion, unsafe-wrapped Loop suppression) is delegated to
//! sibling modules.

use paideia_as_ast::{AstArena, ExprData, NodeId};
use paideia_as_diagnostics::{DiagnosticSink, SourceMap};
use paideia_as_ir::{IrArena, IrKind, IrNodeId};
use std::collections::HashSet;

use super::array_repeat::expand_array_repeat;
use super::record_cons::record_cons_children;
use super::store_lvalue::store_children;

/// Extract the list of AST child ids for the given AST node, given the
/// already-computed IR node id (needed so Store rewriting knows whether the
/// first pass classified this Infix as Store or App).
///
/// `nodes_in_unsafe_blocks` comes from `unsafe_scan::collect_nodes_in_unsafe_blocks`
/// and is used to suppress child transfer for Loop/While nodes that live
/// inside an unsafe block (PA-R17-012c / #990).
pub(super) fn collect_ast_children(
    ast: &AstArena,
    ir: &IrArena,
    ast_id: NodeId,
    ir_id: IrNodeId,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
    registry: &crate::StructRegistry,
    nodes_in_unsafe_blocks: &HashSet<u32>,
) -> Vec<NodeId> {
    let ast_node = &ast[ast_id];

    // Expression forms.
    if let Some(expr_data) = ast.expr_data(ast_id) {
        return expr_children(
            ast,
            ir,
            ast_id,
            ir_id,
            expr_data,
            ast_node.span,
            source_map,
            sink,
            registry,
            nodes_in_unsafe_blocks,
        );
    }

    // Item-level declarations.
    if let Some(item_data) = ast.item_data(ast_id) {
        use paideia_as_ast::ItemData;
        return match item_data {
            ItemData::Let { value, .. } => vec![*value],
            ItemData::Structure { items, .. } => items.clone(),
            ItemData::Module { body, .. } => vec![*body],
            _ => Vec::new(),
        };
    }

    // Statements.
    if let Some(stmt_data) = ast.stmt_data(ast_id) {
        use paideia_as_ast::StmtData;
        return match stmt_data {
            StmtData::Let { name, ty, value, .. } => {
                let mut children = vec![*name, *value];
                if let Some(t) = ty {
                    children.push(*t);
                }
                children
            }
            StmtData::Expr { expr } => vec![*expr],
            StmtData::Return { value } => value.iter().copied().collect(),
            StmtData::Instruction { operands, .. } => operands.clone(),
            StmtData::Label { name } => vec![*name],
        };
    }

    Vec::new()
}

#[allow(clippy::too_many_arguments)]
fn expr_children(
    ast: &AstArena,
    ir: &IrArena,
    ast_id: NodeId,
    ir_id: IrNodeId,
    expr_data: &ExprData,
    ast_span: paideia_as_diagnostics::Span,
    source_map: &SourceMap,
    sink: &mut dyn DiagnosticSink,
    registry: &crate::StructRegistry,
    nodes_in_unsafe_blocks: &HashSet<u32>,
) -> Vec<NodeId> {
    match expr_data {
        ExprData::Lambda { body, .. } => {
            // Lambda: body is a single child node
            vec![*body]
        }
        ExprData::Call { callee, args } => {
            // Call (App): callee + all arguments
            let mut children = vec![*callee];
            children.extend(args.iter().copied());
            children
        }
        ExprData::Infix { op, lhs, rhs } => {
            // Check if this is an l-value assignment.
            // Phase 7 m5-001 & m5-002; Phase 17 m6-b: if this lowered to Store, rearrange children.
            // Store expects [addr_or_field_access, index_or_unused, value].
            // For field access patterns, we pass the FieldAccess IR node as child[0]
            // so populate_field_access_info can access field info via the side-table.
            let is_store = ir
                .get(ir_id)
                .map(|n| n.kind == IrKind::Store)
                .unwrap_or(false);
            if is_store {
                if let Some(children) = store_children(ast, *lhs, *op, *rhs) {
                    return children;
                }
            }
            vec![*op, *lhs, *rhs]
        }
        ExprData::Prefix { op, expr, kind } => {
            // Prefix `~` (BitNot) lowers to `IrKind::BitNot`, whose
            // sole child is the operand (no operator node). All other
            // prefix operators desugar to `App` with structure
            // [callee (the op), arg0 (expr)].
            match kind {
                paideia_as_ast::PrefixOp::BitNot => vec![*expr],
                _ => vec![*op, *expr],
            }
        }
        ExprData::Postfix { expr, op } => {
            // Postfix: expr (argument), op (postfix operator)
            // App structure: [callee (the op), arg0 (expr)]
            vec![*op, *expr]
        }
        ExprData::Cast { expr, .. } => {
            // Cast lowers to IrKind::Cast with a single child (the
            // operand). The target type is metadata recorded out-of-band
            // in the CastSideTable, not a structural child.
            vec![*expr]
        }
        ExprData::Literal { .. } => Vec::new(),
        ExprData::StringLiteral(_) | ExprData::ByteStringLiteral(_) => {
            // StringLiteral (PA10-002): no structural children.
            // Byte payload is stored in literal_bytes side-table by the elaborator.
            Vec::new()
        }
        ExprData::InlineBytes(_) => {
            // InlineBytes (Issue #1012): @guid and @include_bytes produce inline bytes.
            // No structural children; byte payload is stored in literal_bytes side-table.
            Vec::new()
        }
        ExprData::InlineStr(_) => {
            // InlineStr (Issue #1014): @include_str and @include_bytes_as_str produce inline strings.
            // No structural children; byte payload is stored in literal_bytes side-table.
            Vec::new()
        }
        ExprData::Block { stmts, tail } => {
            // Block: all statements + optional tail expression
            let mut children = stmts.iter().copied().collect::<Vec<_>>();
            if let Some(tail_expr) = tail {
                children.push(*tail_expr);
            }
            children
        }
        ExprData::ActionBlock {
            effects,
            capabilities,
            body,
        } => {
            // ActionBlock: optional effects/capabilities + all statements
            let mut children = Vec::new();
            if let Some(eff) = effects {
                children.push(*eff);
            }
            if let Some(cap) = capabilities {
                children.push(*cap);
            }
            children.extend(body.iter().copied());
            children
        }
        ExprData::ArrayLit(elements) => elements.clone(),
        ExprData::ArrayRepeat { expr, count } => {
            // ArrayRepeat: expand [expr; count] to N copies of expr.
            expand_array_repeat(ast, *expr, *count)
        }
        ExprData::RecordCons { type_name, fields } => record_cons_children(
            ast,
            source_map,
            sink,
            registry,
            ast_span,
            *type_name,
            fields,
        ),
        ExprData::FieldAccess { receiver, .. } => {
            // FieldAccess: single child is the receiver expression.
            // The field ident is metadata (in FieldAccessInfo side-table).
            vec![*receiver]
        }
        ExprData::Unsafe { block, .. } => {
            // Unsafe block: the block's statements become children.
            // Phase 8 m4-001: walk the block's statements and emit each as an IR child.
            block.clone()
        }
        ExprData::Borrow { expr, .. } => {
            // Borrow (& and &mut): single child is the inner expression.
            vec![*expr]
        }
        ExprData::Deref { expr } => vec![*expr],
        ExprData::If {
            cond,
            then_block,
            else_block,
        } => {
            // If-then-else: condition, then block, and optional else block.
            let mut children = vec![*cond, *then_block];
            if let Some(else_id) = else_block {
                children.push(*else_id);
            }
            children
        }
        ExprData::Match { scrutinee, arms, .. } => {
            // Match: scrutinee + arm bodies only.
            // Patterns and guards are dropped here; populate_match_arm_meta
            // walks AST arms directly to extract pattern classification.
            let mut children = vec![*scrutinee];
            for arm in arms {
                children.push(arm.body);
            }
            children
        }
        ExprData::Loop { header, body, .. } => {
            // Loop (infinite or while): optional header (condition) + body.
            // BUGFIX PA-R17-012c (#990): Skip child transfer for While/Loop nodes inside
            // unsafe blocks. While nodes inside unsafe blocks should not have children to
            // avoid conflicts with unsafe_walker's label handling. The unsafe_walker
            // processes the block statements independently, so the While control flow and
            // unsafe block context must be kept separate.
            if nodes_in_unsafe_blocks.contains(&ast_id.get()) {
                Vec::new()
            } else {
                let mut children = Vec::new();
                if let Some(header_id) = header {
                    children.push(*header_id);
                }
                children.push(*body);
                children
            }
        }
        _ => Vec::new(),
    }
}
