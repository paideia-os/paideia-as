//! L-value assignment detection and Store child rearrangement.
//! Split out of `lower.rs` (2026-07-08).
//!
//! Phase 7 m5-001 / m5-002, Phase 17 m6-b, and Phase 17 m6-c introduced five
//! assignment patterns that lower to `IrKind::Store` rather than the generic `App`:
//!
//! 1. `a[i] = value` — LHS is `ExprCall` with 1 argument.
//! 2. `*p = value` — LHS is `ExprDeref`.
//! 3. `(*p).f = value` — LHS is `ExprFieldAccess` whose receiver is `ExprDeref`.
//! 4. `r.f = value` — LHS is `ExprFieldAccess` whose receiver is `ExprPath` / `Ident`.
//! 5. `x = value` — LHS is bare `Ident` or single-segment `ExprPath`.
//!
//! `is_lvalue_infix_assignment` classifies an infix `=` node in the first
//! pass so `map_node_kind`'s `App` bucket can be refined to `Store`.
//! `store_children` rebuilds the child list for the Store node in the
//! second pass, producing `[lhs, op, rhs]` for Pattern 5 or the
//! pattern-specific layout for Patterns 1-4.

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
    // Pattern 5: x = value — bare Ident or single-segment ExprPath
    if let Some(ExprData::Path { segments }) = ast.expr_data(lhs) {
        // Single-segment ExprPath is a variable assignment
        if segments.len() == 1 {
            return true;
        }
    }
    // Pattern 5 (alt): bare Ident in identifier position
    if let Some(node) = ast.get(lhs) {
        if node.kind == NodeKind::Ident {
            return true;
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
/// - Pattern 5 `x = value` → `[lhs, op, rhs]` (bare Ident or single-segment ExprPath)
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
    // Try Pattern 5: x = value (single-segment ExprPath or bare Ident on LHS)
    if let Some(ExprData::Path { segments }) = ast.expr_data(lhs) {
        if segments.len() == 1 {
            // Single-segment path: x = value
            return Some(vec![lhs, op, rhs]);
        }
    }
    // Pattern 5 (alt): bare Ident
    if let Some(node) = ast.get(lhs) {
        if node.kind == NodeKind::Ident {
            return Some(vec![lhs, op, rhs]);
        }
    }
    None
}
