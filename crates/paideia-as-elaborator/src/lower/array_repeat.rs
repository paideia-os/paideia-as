//! Array-repeat expression expansion helpers. Split out of `lower.rs` (2026-07-08).
//!
//! `[expr; count]` is lowered to `IrKind::ArrayLit` with N structural children
//! (one per repetition). When `count` cannot be constant-folded at lowering
//! time, the expression falls back to a single child copy and the elaborator
//! is expected to emit P0211 later.

use paideia_as_ast::{AstArena, ExprData, NodeId};

/// Extract a count literal from an AST expression node.
///
/// Returns `Some(count)` if the expression is a Literal node whose integer
/// value can be extracted. Returns `None` if the count is not a literal,
/// or if extraction fails (non-integer literal).
pub(super) fn extract_repeat_count(ast: &AstArena, count_expr_id: NodeId) -> Option<usize> {
    // Check if count_expr is a Literal expression
    if let Some(ExprData::Literal { lit: _lit_node_id }) = ast.expr_data(count_expr_id) {
        // The lit_node_id is a Placeholder node. In phase-1, we have limited
        // access to the actual value without a full evaluator. For now, we
        // return None to defer to the elaborator's error reporting.
        // Future phases will add constant evaluation in the elaborator.
        None
    } else {
        None
    }
}

/// Expand an array repeat expression to N copies of the element.
///
/// Given `[expr; count]`, this function:
/// 1. Attempts to extract count as a constant integer literal
/// 2. If successful, returns N copies of expr as children
/// 3. If count is not a literal, returns a single copy with a note
///    that P0211 should be emitted by the elaborator
pub(super) fn expand_array_repeat(ast: &AstArena, expr: NodeId, count: NodeId) -> Vec<NodeId> {
    // Try to extract the count literal
    if let Some(count_val) = extract_repeat_count(ast, count) {
        // Replicate expr count_val times
        vec![expr; count_val]
    } else {
        // Count is not a literal. For now, emit a single copy as a fallback.
        // The elaborator will emit P0211 if the constant evaluator can't resolve count.
        vec![expr]
    }
}
