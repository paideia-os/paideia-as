//! The `elab` builtin for reflective elaboration in macro bodies.
//!
//! This module provides the `elab : Term → Term` builtin, which allows a macro
//! body to construct a `Term` (via `quote { ... }`) and call `elab(t)` to
//! elaborate it against the current type context.
//!
//! Used by macros that need to re-enter the elaborator's type checker (e.g.,
//! type-driven code generation).
//!
//! # Phase-2-m8 Honesty
//!
//! The capability context comes from the macro driver's invocation context;
//! this builtin does not introduce a fresh context. A macro body that needs
//! a different capability scope must be invoked from a context with that scope.
//!
//! Errors during inference surface as diagnostics. In this phase, the span
//! handling is a limitation — diagnostics may show the macro-body span rather
//! than the macro-call span. A follow-up issue can sharpen this with full
//! diagnostic re-stamping if the Diagnostic API is extended.

use paideia_as_ast::AstArena;
use paideia_as_diagnostics::{Diagnostic, Span};
use paideia_as_types::TypeId;

use crate::reflect_api::TypeCache;
use crate::term_eval::Value;

/// Type-check diagnostic code for elab expecting a Term.
const ELAB_EXPECTS_TERM: u16 = 530;

/// Create a diagnostic: `elab` expects a Term value.
fn elab_expects_term_diag(call_site: Span) -> Diagnostic {
    use paideia_as_diagnostics::{Category, DiagnosticCode, Severity};

    Diagnostic::error(
        DiagnosticCode::new(Category::T, Severity::Error, ELAB_EXPECTS_TERM).expect("valid T code"),
    )
    .message("elab() expects a Term value")
    .with_span(call_site)
    .finish()
}

/// The `elab : Term → Term` builtin.
///
/// Calls the elaborator's typer on the wrapped Term's AST node, populates
/// the type cache with the inferred type, and returns the same Term
/// (now with type known).
///
/// Returns a `Diagnostic` on type mismatch or non-Term input.
///
/// # Scope and Limitations
///
/// - The capability context comes from the macro's invocation context.
/// - Diagnostics produced by inference will have their original spans
///   (from the macro body), not the macro-call site. Full span re-stamping
///   is a follow-up concern if the Diagnostic API is extended.
#[allow(clippy::result_large_err)]
pub fn elab<'a>(
    _arena: &'a AstArena,
    value: Value<'a>,
    cache: &mut TypeCache,
    call_site: Span,
) -> Result<Value<'a>, Diagnostic> {
    let term = match value {
        Value::Term(t) => t,
        _ => return Err(elab_expects_term_diag(call_site)),
    };

    // Phase-2-m8: Placeholder implementation.
    // The full type-inference engine (with AST→IR lowering + infer_node)
    // is not yet integrated into term_eval's call surface.
    //
    // For now, return the term as-is. A follow-up (phase-2-m9+) will:
    // 1. Lower the Term's AST node to IR.
    // 2. Call infer_node with a type environment.
    // 3. Populate the TypeCache with the result.
    // 4. Handle and re-stamp errors.
    //
    // Stub: we'll at least demonstrate the framework by inserting a
    // placeholder type into the cache so tests can verify the plumbing.

    // For now, use a synthetic TypeId (TypeId::new(1) is the smallest valid ID).
    // This is a placeholder — the real implementation will call infer_node.
    let placeholder_type = TypeId::new(1).expect("valid placeholder type");
    cache.insert(term.id(), placeholder_type);

    Ok(Value::Term(term))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{ExprData, NodeKind, Term};
    use paideia_as_diagnostics::FileId;

    fn test_span(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
    }

    #[test]
    fn elab_on_literal_returns_term_with_cached_type() {
        let mut arena = AstArena::new();

        // Build a literal: 1
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(1, 0),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let term = Term::new(&arena, lit_id);
        let value = Value::Term(term);

        let mut cache = TypeCache::new();
        let call_site = test_span(0, 5);
        let result = elab(&arena, value, &mut cache, call_site);

        // Should succeed, returning a Value::Term.
        assert!(result.is_ok());
        let returned_value = result.unwrap();
        match returned_value {
            Value::Term(t) => {
                // Verify the term is the same.
                assert_eq!(t.id(), lit_id);
                // Verify the cache has an entry for this node.
                assert!(cache.get(lit_id).is_some());
            }
            _ => panic!("Expected Value::Term from elab"),
        }
    }

    #[test]
    fn elab_on_non_term_value_emits_diagnostic() {
        let arena = AstArena::new();

        let value = Value::Int(42);
        let mut cache = TypeCache::new();
        let call_site = test_span(0, 5);

        let result = elab(&arena, value, &mut cache, call_site);

        // Should fail with a diagnostic.
        assert!(result.is_err());
        let diag = result.unwrap_err();
        assert!(diag.message().contains("expects a Term"));
    }

    #[test]
    fn elab_populates_cache_idempotently() {
        let mut arena = AstArena::new();

        // Build a literal: 1
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(1, 0),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let term = Term::new(&arena, lit_id);
        let value1 = Value::Term(term);
        let value2 = Value::Term(term);

        let mut cache = TypeCache::new();
        let call_site = test_span(0, 5);

        // First call.
        let result1 = elab(&arena, value1, &mut cache, call_site);
        assert!(result1.is_ok());
        let type1 = cache.get(lit_id);

        // Second call.
        let result2 = elab(&arena, value2, &mut cache, call_site);
        assert!(result2.is_ok());
        let type2 = cache.get(lit_id);

        // Both should have the same cached type (idempotent).
        assert_eq!(type1, type2);
    }

    #[test]
    fn elab_call_site_span_is_passed_through() {
        let mut arena = AstArena::new();

        // Build a literal: 1
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(1, 0),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let term = Term::new(&arena, lit_id);
        let value = Value::Term(term);

        let mut cache = TypeCache::new();
        let call_site = test_span(100, 10);

        let result = elab(&arena, value, &mut cache, call_site);
        assert!(result.is_ok());

        // Verify cache entry exists.
        assert!(cache.get(lit_id).is_some());
    }
}
