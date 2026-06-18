//! Linearity validation and diagnostic emission.
//!
//! Validates that bindings in a closed scope satisfy the substructural
//! lattice constraints defined in `design/toolchain/custom-assembler.md` §3.1.
//! Emits S-range diagnostic codes for violations.
//!
//! Also provides minimal AST walking for block expressions to maintain proper
//! scope nesting in `LinearityCtx`. Full per-statement tracking arrives when
//! the IR walker is implemented.

use std::collections::HashMap;

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
use paideia_as_ir::LinClass;

use crate::env::Symbol;
use crate::linearity_ctx::{Binding, LinearityCtx};

/// Diagnostic code for a Linear or Ordered binding that is never used (use-count = 0).
pub const S_NEVER_USED: u16 = 900;

/// Diagnostic code for a binding that violates its use-count constraint.
pub const S_OVERUSED: u16 = 901;

/// Construct a DiagnosticCode in the S category.
fn s_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::S, Severity::Error, n).expect("valid S code")
}

/// Inspect a closed scope and produce diagnostics per the substructural lattice.
///
/// The validation rules are:
///
/// - **Linear**: must be used exactly once.
///   - Use-count 0 → S0900 (never used).
///   - Use-count > 1 → S0901 (overused).
///
/// - **Ordered**: same constraint as Linear (used exactly once).
///   - Use-count 0 → S0900.
///   - Use-count > 1 → S0901.
///
/// - **Affine**: may be used at most once.
///   - Use-count 0 → OK.
///   - Use-count > 1 → S0901 (overused).
///
/// - **Unrestricted**: no constraints.
///   - Any use-count is valid.
///
/// The returned diagnostics are sorted by source span for determinism.
pub fn validate_scope(scope: &HashMap<Symbol, Binding>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    for (_sym, b) in scope.iter() {
        match (b.class, b.uses) {
            (LinClass::Linear | LinClass::Ordered, 0) => {
                diags.push(
                    Diagnostic::error(s_code(S_NEVER_USED))
                        .message(format!(
                            "{:?} binding is never used; substructural lattice requires exactly one use",
                            b.class
                        ))
                        .with_span(b.bind_span)
                        .finish(),
                );
            }
            (LinClass::Linear | LinClass::Ordered, n) if n > 1 => {
                diags.push(
                    Diagnostic::error(s_code(S_OVERUSED))
                        .message(format!(
                            "{:?} binding used {n} times; substructural lattice permits exactly one use",
                            b.class
                        ))
                        .with_span(b.bind_span)
                        .finish(),
                );
            }
            (LinClass::Affine, n) if n > 1 => {
                diags.push(
                    Diagnostic::error(s_code(S_OVERUSED))
                        .message(format!(
                            "affine binding used {n} times; affine permits at most one use"
                        ))
                        .with_span(b.bind_span)
                        .finish(),
                );
            }
            _ => {}
        }
    }

    diags.sort_by_key(|d| d.primary_span().map(|s| s.byte_start()).unwrap_or(0));
    diags
}

/// Walk an AST node to maintain proper scope nesting in block expressions.
///
/// This minimal walker handles `ExprData::Block` by:
/// 1. Entering a new scope before visiting statements and tail.
/// 2. Leaving the scope after the block is processed.
///
/// All other expression kinds and statements are left for future
/// per-statement linearity tracking (phase-2+).
///
/// Returns the same scope depth as before the call (balanced push/pop).
pub fn walk_expr_for_scope(arena: &AstArena, ctx: &mut LinearityCtx, expr_id: NodeId) {
    let node = match arena.get(expr_id) {
        Some(n) => n,
        None => return,
    };

    if node.kind != NodeKind::ExprBlock {
        // Only ExprBlock needs scope tracking for now.
        return;
    }

    if let Some(ExprData::Block { stmts, tail }) = arena.expr_data(expr_id) {
        // Enter a new scope for this block
        ctx.enter_scope();

        // Walk statements (if any)
        for &stmt_id in stmts.iter() {
            // Currently no per-statement tracking; just maintain scopes.
            // Future: walk each statement to track bindings and uses.
            let _ = stmt_id;
        }

        // Walk tail expression (if any)
        if let Some(tail_id) = tail {
            // Currently no tail expression tracking; just maintain scopes.
            // Future: walk tail to track uses.
            let _ = tail_id;
        }

        // Leave the block's scope
        let _scope = ctx.leave_scope();
        // Note: we don't validate the scope here; that happens at IR lowering
        // when scopes are closed and checked against linearity constraints.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    fn binding(class: LinClass, uses: u32, bind_span: Span) -> Binding {
        Binding {
            class,
            uses,
            bind_span,
        }
    }

    #[test]
    fn linear_used_once_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 1, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn linear_used_twice_emits_s0901() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 2, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn linear_unused_emits_s0900() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 0, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn affine_unused_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Affine, 0, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn affine_used_once_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Affine, 1, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn affine_used_twice_emits_s0901() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Affine, 2, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn ordered_unused_emits_s0900() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Ordered, 0, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn ordered_used_once_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Ordered, 1, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn ordered_used_twice_emits_s0901() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Ordered, 2, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn unrestricted_used_arbitrarily_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Unrestricted, 5, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn unrestricted_unused_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Unrestricted, 0, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn multiple_bindings_in_scope() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 1, span(100)));
        scope.insert(20, binding(LinClass::Linear, 0, span(110))); // unused

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn multiple_violations_sorted_by_span() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 0, span(100))); // S0900 at span 100
        scope.insert(20, binding(LinClass::Linear, 2, span(50))); // S0901 at span 50

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 2);
        // sorted by span: 50 < 100
        assert_eq!(diags[0].primary_span().map(|s| s.byte_start()), Some(50));
        assert_eq!(diags[1].primary_span().map(|s| s.byte_start()), Some(100));
    }

    #[test]
    fn block_scope_push_pop_balanced() {
        // Walking an ExprBlock leaves LinearityCtx::depth() unchanged.
        use paideia_as_ast::{AstArena, NodeKind};

        let mut arena = AstArena::new();
        let test_span = span(0);

        // Construct a simple block: { 42 } (empty stmts, tail = literal 42)
        let lit_node = arena.alloc(NodeKind::Placeholder, test_span);
        let lit_42 = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span,
            paideia_as_ast::ExprData::Literal { lit: lit_node },
        );

        let block = arena.alloc_expr(
            NodeKind::ExprBlock,
            test_span,
            ExprData::Block {
                stmts: vec![],
                tail: Some(lit_42),
            },
        );

        let mut ctx = LinearityCtx::new();
        let initial_depth = ctx.depth();
        assert_eq!(initial_depth, 1, "root scope");

        walk_expr_for_scope(&arena, &mut ctx, block);

        let final_depth = ctx.depth();
        assert_eq!(
            final_depth, initial_depth,
            "scope depth should be unchanged after walking block"
        );
    }

    use paideia_as_diagnostics::Span;
}
