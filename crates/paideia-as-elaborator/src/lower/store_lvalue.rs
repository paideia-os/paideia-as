//! L-value assignment detection and Store child rearrangement.
//! Split out of `lower.rs` (2026-07-08).
//!
//! Phase 7 m5-001 / m5-002 and Phase 17 m6-b introduced four assignment
//! patterns that lower to `IrKind::Store` rather than the generic `App`:
//!
//! 1. `a[i] = value` — LHS is `ExprCall` with 1 argument.
//! 2. `*p = value` — LHS is `ExprDeref`.
//! 3. `(*p).f = value` — LHS is `ExprFieldAccess` whose receiver is `ExprDeref`.
//! 4. `r.f = value` — LHS is `ExprFieldAccess` whose receiver is `ExprPath` / `Ident`.
//!
//! `is_lvalue_infix_assignment` classifies an infix `=` node in the first
//! pass so `map_node_kind`'s `App` bucket can be refined to `Store`.
//! `store_children` rebuilds the child list for the Store node in the
//! second pass, producing `[addr_or_field_access, index_or_unused, value]`.

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};

/// Given an `ExprInfix` node whose operator token is `=` (single byte),
/// return `true` iff the LHS is one of the five l-value shapes that lower
/// to `IrKind::Store`.
pub(super) fn is_lvalue_infix_assignment(ast: &AstArena, lhs: NodeId) -> bool {
    if let Some(ExprData::Call { args, .. }) = ast.expr_data(lhs) {
        // Pattern 1: a[i] = value
        return args.len() == 1;
    }
    if let Some(ExprData::Deref { .. }) = ast.expr_data(lhs) {
        // Pattern 2: *p = value
        return true;
    }
    if let Some(ExprData::FieldAccess { receiver, .. }) = ast.expr_data(lhs) {
        // Pattern 3 & 4: field access on deref or direct variable
        if let Some(ExprData::Deref { .. }) = ast.expr_data(*receiver) {
            // Pattern 3: (*p).f = value
            return true;
        }
        // Pattern 4: r.f = value — receiver must be a simple ExprPath or Ident
        if let Some(n) = ast.get(*receiver) {
            return n.kind == NodeKind::ExprPath || n.kind == NodeKind::Ident;
        }
    }
    // Pattern 5: counter = v (bare Ident or single-segment ExprPath)
    // #1132 classifier guard ensures operator token is literally "="
    if let Some(n) = ast.get(lhs) {
        if n.kind == NodeKind::Ident {
            return true;
        }
        if n.kind == NodeKind::ExprPath {
            if let Some(ExprData::Path { segments }) = ast.expr_data(lhs) {
                return segments.len() == 1;
            }
        }
    }
    false
}

/// Compute the child list for a Store node produced from an Infix `=` node.
///
/// Returns `Some(children)` if `lhs` matches one of the five l-value
/// patterns; children are laid out as `[addr_or_field_access, index_or_unused, value]`
/// per the emit-pass contract:
///
/// - Pattern 1 `a[i] = value` → `[base, index, value]`
/// - Pattern 2 `*p = value` → `[pointer, op, value]` (op is the `=` node — reused as the "unused" slot)
/// - Patterns 3 & 4 field access → `[FieldAccess_ast, op, value]` (the FieldAccess AST id
///   is later remapped to its IR id by the caller's child-transfer loop)
/// - Pattern 5 `counter = v` → `[Var(lhs), op, Var(rhs)]` (bare Ident or single-segment path)
///
/// Returns `None` if `lhs` does not match a supported l-value shape (caller
/// should fall back to the plain Infix `[op, lhs, rhs]` layout).
pub(super) fn store_children(
    ast: &AstArena,
    lhs: NodeId,
    op: NodeId,
    rhs: NodeId,
) -> Option<Vec<NodeId>> {
    // Try Pattern 1: a[i] = value (ExprCall on LHS)
    if let Some(ExprData::Call { callee, args }) = ast.expr_data(lhs) {
        if args.len() == 1 {
            // callee is the base, args[0] is the index, rhs is the value
            return Some(vec![*callee, args[0], rhs]);
        }
        return None;
    }
    // Try Pattern 2: *p = value (ExprDeref on LHS)
    if let Some(ExprData::Deref { expr }) = ast.expr_data(lhs) {
        // For deref store, children are [pointer, unused, value]
        return Some(vec![*expr, op, rhs]);
    }
    // Try Pattern 3 & 4: field access (ExprFieldAccess on LHS)
    // For field access store, children are [FieldAccess_ast, unused, value]
    // The AST-to-IR mapping will convert FieldAccess_ast to FieldAccess_ir
    // in the children transfer loop below.
    if let Some(ExprData::FieldAccess { .. }) = ast.expr_data(lhs) {
        return Some(vec![lhs, op, rhs]);
    }
    // Try Pattern 5: counter = v (bare Ident or single-segment ExprPath)
    // For var store, children are [Var(lhs), op, Var(rhs)]
    // Both LHS and RHS become Var nodes in the IR.
    if let Some(n) = ast.get(lhs) {
        if n.kind == NodeKind::Ident || n.kind == NodeKind::ExprPath {
            if n.kind == NodeKind::Ident {
                return Some(vec![lhs, op, rhs]);
            }
            if let Some(ExprData::Path { segments }) = ast.expr_data(lhs) {
                if segments.len() == 1 {
                    return Some(vec![lhs, op, rhs]);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{AstArena, ExprData};
    use paideia_as_diagnostics::Span;

    fn dummy_span() -> Span {
        Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    }

    #[test]
    fn is_lvalue_call_with_one_arg_is_assignment() {
        // Pattern 1: a[i] = value (ExprCall with 1 argument)
        let mut ast = AstArena::new();

        let callee_id = ast.alloc(NodeKind::Ident, dummy_span());
        let arg_id = ast.alloc(NodeKind::ExprLiteral, dummy_span());
        let call_id = ast.alloc_expr(
            NodeKind::ExprCall,
            dummy_span(),
            ExprData::Call {
                callee: callee_id,
                args: vec![arg_id],
            },
        );

        assert!(is_lvalue_infix_assignment(&ast, call_id));
    }

    #[test]
    fn is_lvalue_call_with_two_args_not_assignment() {
        // Pattern 1 should only match with 1 argument
        let mut ast = AstArena::new();

        let callee_id = ast.alloc(NodeKind::Ident, dummy_span());
        let arg1_id = ast.alloc(NodeKind::ExprLiteral, dummy_span());
        let arg2_id = ast.alloc(NodeKind::ExprLiteral, dummy_span());
        let call_id = ast.alloc_expr(
            NodeKind::ExprCall,
            dummy_span(),
            ExprData::Call {
                callee: callee_id,
                args: vec![arg1_id, arg2_id],
            },
        );

        assert!(!is_lvalue_infix_assignment(&ast, call_id));
    }

    #[test]
    fn is_lvalue_deref_is_assignment() {
        // Pattern 2: *p = value (ExprDeref)
        let mut ast = AstArena::new();

        let expr_id = ast.alloc(NodeKind::Ident, dummy_span());
        let deref_id = ast.alloc_expr(
            NodeKind::ExprDeref,
            dummy_span(),
            ExprData::Deref { expr: expr_id },
        );

        assert!(is_lvalue_infix_assignment(&ast, deref_id));
    }

    #[test]
    fn is_lvalue_field_access_on_deref_is_assignment() {
        // Pattern 3: (*p).f = value (ExprFieldAccess on ExprDeref)
        let mut ast = AstArena::new();

        let pointer_id = ast.alloc(NodeKind::Ident, dummy_span());
        let deref_id = ast.alloc_expr(
            NodeKind::ExprDeref,
            dummy_span(),
            ExprData::Deref {
                expr: pointer_id,
            },
        );
        let field_node_id = ast.alloc(NodeKind::Ident, dummy_span());
        let field_id = ast.alloc_expr(
            NodeKind::ExprFieldAccess,
            dummy_span(),
            ExprData::FieldAccess {
                receiver: deref_id,
                field: field_node_id,
            },
        );

        assert!(is_lvalue_infix_assignment(&ast, field_id));
    }

    #[test]
    fn is_lvalue_field_access_on_ident_is_assignment() {
        // Pattern 4: r.f = value (ExprFieldAccess on Ident)
        let mut ast = AstArena::new();

        let receiver_id = ast.alloc(NodeKind::Ident, dummy_span());
        let field_node_id = ast.alloc(NodeKind::Ident, dummy_span());
        let field_id = ast.alloc_expr(
            NodeKind::ExprFieldAccess,
            dummy_span(),
            ExprData::FieldAccess {
                receiver: receiver_id,
                field: field_node_id,
            },
        );

        assert!(is_lvalue_infix_assignment(&ast, field_id));
    }

    #[test]
    fn is_lvalue_field_access_on_path_is_assignment() {
        // Pattern 4: r.f = value (ExprFieldAccess on ExprPath)
        let mut ast = AstArena::new();

        let segment_id = ast.alloc(NodeKind::Ident, dummy_span());
        let receiver_id = ast.alloc_expr(
            NodeKind::ExprPath,
            dummy_span(),
            ExprData::Path {
                segments: vec![segment_id],
            },
        );
        let field_node_id = ast.alloc(NodeKind::Ident, dummy_span());
        let field_id = ast.alloc_expr(
            NodeKind::ExprFieldAccess,
            dummy_span(),
            ExprData::FieldAccess {
                receiver: receiver_id,
                field: field_node_id,
            },
        );

        assert!(is_lvalue_infix_assignment(&ast, field_id));
    }

    #[test]
    fn is_lvalue_field_access_on_call_not_assignment() {
        // Pattern 4 should only match Ident/ExprPath, not Call
        let mut ast = AstArena::new();

        let callee_id = ast.alloc(NodeKind::Ident, dummy_span());
        let arg_id = ast.alloc(NodeKind::ExprLiteral, dummy_span());
        let call_id = ast.alloc_expr(
            NodeKind::ExprCall,
            dummy_span(),
            ExprData::Call {
                callee: callee_id,
                args: vec![arg_id],
            },
        );
        let field_node_id = ast.alloc(NodeKind::Ident, dummy_span());
        let field_id = ast.alloc_expr(
            NodeKind::ExprFieldAccess,
            dummy_span(),
            ExprData::FieldAccess {
                receiver: call_id,
                field: field_node_id,
            },
        );

        assert!(!is_lvalue_infix_assignment(&ast, field_id));
    }

    #[test]
    fn is_lvalue_ident_is_assignment() {
        // Plain Ident IS an l-value (Pattern 5: counter = v)
        let mut ast = AstArena::new();

        let ident_id = ast.alloc(NodeKind::Ident, dummy_span());

        assert!(is_lvalue_infix_assignment(&ast, ident_id));
    }

    #[test]
    fn is_lvalue_literal_not_assignment() {
        // Literal should NOT be considered an l-value
        let mut ast = AstArena::new();

        let lit_id = ast.alloc(NodeKind::ExprLiteral, dummy_span());

        assert!(!is_lvalue_infix_assignment(&ast, lit_id));
    }
}

