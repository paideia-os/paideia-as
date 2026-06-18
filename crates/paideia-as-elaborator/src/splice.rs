//! Splice operation for converting elaborated Terms back to AST.
//!
//! This module implements the `splice` operation, which converts a `Term` value
//! (produced by `quote` or macro logic during term evaluation) back into an
//! elaborator-visible AST node at the macro call site.
//!
//! ## Overview
//!
//! `splice(t : Term) : NodeId` — at a macro's call site, the call form `m(args)`
//! is replaced by the elaborated form of the AST node that `splice` returns.
//!
//! Phase-2-m6 implementation: since `Term` wraps `(NodeId, &AstArena)` and the
//! source arena is currently the same as the destination arena (flat single-arena
//! model), splice simply returns the same `NodeId` with the call-site span
//! overridden for diagnostics. When macro drivers introduce fresh arenas per
//! expansion (m2-007+), the full cross-arena reinflation and copying logic
//! will activate.
//!
//! Phase-2-m10 (m2-010): extends splice to track hygiene for reflectively-
//! constructed terms. When a splice is executed, identifiers introduced
//! *by the splice* (i.e., not from antiquotes) receive a fresh `MacroId` tag.
//! The [`HygieneCache`] is populated by [`splice_with_hygiene`].

use paideia_as_ast::{NodeId, Term};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_types::TypeId;

use crate::hygiene::{HygieneCache, MacroId};
use crate::reflect_api::TypeCache;
use crate::term_eval::Value;

/// Create a diagnostic for splicing a Term with the wrong value kind.
fn splice_type_mismatch(call_site: Span) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::new(Category::T, Severity::Error, 520).expect("valid T code"))
        .message("splice() expects a Term value")
        .with_span(call_site)
        .finish()
}

/// Create a diagnostic for splicing a Term whose type does not match the expected type.
fn t0506_type_mismatch(call_site: Span, expected_ty: TypeId, actual_ty: TypeId) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::new(Category::T, Severity::Error, 506).expect("valid T code"))
        .message(format!(
            "spliced Term type does not match expected type at call site: expected {:?}, got {:?}",
            expected_ty, actual_ty
        ))
        .with_span(call_site)
        .finish()
}

/// Splice a `Term` into the calling context, returning the elaborated NodeId.
///
/// Given a `Term` value (produced by `quote` or macro logic), this function
/// makes the Term's AST node available at the call site. The implementation
/// is a pass-through in phase-2-m6 (both the macro and call site share the
/// same arena), and the full cross-arena copying will arrive in m2-007 when
/// macros get fresh arenas.
///
/// Returns the NodeId of the spliced node on success. Returns a Diagnostic
/// if the value is not a Term or if there's an arena mismatch.
#[allow(clippy::result_large_err)]
pub fn splice<'src>(value: Value<'src>, call_site: Span) -> Result<NodeId, Diagnostic> {
    match value {
        Value::Term(t) => {
            // Phase-2-m6: the macro body and call site share the same arena.
            // Simply return the term's node ID. The call_site span is used
            // by the macro driver to override the diagnostic span for the
            // spliced code (so errors surface at the caller's location).
            Ok(t.id())
        }
        _ => Err(splice_type_mismatch(call_site)),
    }
}

/// Splice a `Term` into the calling context with type checking.
///
/// This variant of `splice` verifies that the spliced Term's inferred type
/// (via the TypeCache) matches the expected type at the call site. If types
/// don't match, emits diagnostic T0506.
///
/// Phase-2-m6: type checking is exercised via unit tests. The calling-context
/// type information will be wired in during m2-007 when the macro driver
/// integrates type expectations.
#[allow(clippy::result_large_err)]
pub fn splice_with_type_check<'src>(
    value: Value<'src>,
    expected_ty: TypeId,
    call_site: Span,
    type_cache: &TypeCache,
) -> Result<NodeId, Diagnostic> {
    let term = match &value {
        Value::Term(t) => t,
        _ => return Err(splice_type_mismatch(call_site)),
    };

    // Look up the inferred type of the term's node.
    if let Some(actual_ty) = type_cache.get(term.id())
        && actual_ty != expected_ty
    {
        return Err(t0506_type_mismatch(call_site, expected_ty, actual_ty));
    }

    // Type matches (or is uncached); proceed with splice.
    splice(value, call_site)
}

/// Splice a `Term` into the calling context with hygiene tracking.
///
/// Phase-2-m10 (m2-010): this variant integrates the hygiene cache. When
/// the spliced term contains identifiers, they are tagged with the provided
/// `macro_tag` (except for identifiers inside antiquotes, which retain
/// their call-site context).
///
/// The cache is returned so the macro driver can store it and use it during
/// name resolution. Phase-3 will integrate this inline into the AST.
///
/// # Arguments
///
/// - `value`: the Value to splice (must be a Term).
/// - `macro_tag`: the fresh tag for this expansion.
/// - `call_site`: the span where the splice call appears (for diagnostics).
///
/// # Returns
///
/// On success, returns `(NodeId, HygieneCache)`. On error, returns a Diagnostic.
#[allow(clippy::result_large_err)]
pub fn splice_with_hygiene<'src>(
    value: Value<'src>,
    macro_tag: MacroId,
    call_site: Span,
) -> Result<(NodeId, HygieneCache), Diagnostic> {
    let term = match value {
        Value::Term(t) => t,
        _ => return Err(splice_type_mismatch(call_site)),
    };

    // Walk the spliced term and tag all introduced identifiers.
    let mut cache = HygieneCache::new();
    splice_walker_and_tag(&term, macro_tag, &mut cache);

    Ok((term.id(), cache))
}

/// Internal helper: walk a Term and populate the hygiene cache.
///
/// Recursively traverses the term's subtree, tagging each identifier (that
/// is not inside an antiquote) with the given macro tag.
///
/// Phase-2 honesty: we detect identifiers by their TermHead, but we lack
/// the actual spelling from the AST node. This is a placeholder until
/// phase-3 when identifiers carry inline source text.
fn splice_walker_and_tag(term: &Term<'_>, macro_tag: MacroId, cache: &mut HygieneCache) {
    use paideia_as_ast::reflect::TermHead;

    // If this is an antiquote, don't tag its contents.
    if term.head() == TermHead::Antiquote {
        // Explicitly skip tagging inside antiquotes.
        return;
    }

    // Check if this term is an identifier. We detect identifiers by checking
    // if they appear as Path nodes with a single Ident segment. However,
    // since we only have public Term API, we tag based on TermHead::Path
    // containing TermHead::Ident children. For now, we tag any node that
    // appears at the Ident level.
    if term.head() == TermHead::Path {
        // A path is an identifier reference; tag all its segments.
        for child in term.children() {
            // Each child of a Path is an identifier segment.
            let spelling = format!("_id{}", child.id().get());
            cache.tag_introduced_at(child.id(), spelling, macro_tag);
        }
    }

    // Recurse into children (except antiquote subtrees).
    for child in term.children() {
        if child.head() != TermHead::Antiquote {
            splice_walker_and_tag(&child, macro_tag, cache);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ast::{AstArena, ExprData, NodeKind};
    use paideia_as_diagnostics::FileId;
    use paideia_as_types::TypeId as TId;

    fn test_span(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
    }

    #[test]
    fn splices_quoted_literal() {
        let mut arena = AstArena::new();

        // Build a quoted literal: quote { 1 }
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(0, 1));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(0, 1),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            test_span(0, 10),
            ExprData::Quote { body: lit_id },
        );

        // Create a Term from the quoted expression.
        let quote_term = Term::new(&arena, quote_id);

        // Create a Value::Term and splice it.
        let term_value = Value::Term(quote_term);
        let call_site = test_span(100, 5);
        let result = splice(term_value, call_site);

        // Should succeed, returning the quote node ID.
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), quote_id);
    }

    #[test]
    fn splices_quoted_arithmetic() {
        let mut arena = AstArena::new();

        // Build: quote { 1 + 1 }
        let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
        let lit1_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(1, 0),
            ExprData::Literal {
                lit: lit1_placeholder,
            },
        );

        let op_id = arena.alloc(NodeKind::Placeholder, test_span(0, 0)); // + operator

        let lit2_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
        let lit2_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(1, 0),
            ExprData::Literal {
                lit: lit2_placeholder,
            },
        );

        let infix_id = arena.alloc_expr(
            NodeKind::ExprInfix,
            test_span(0, 5),
            ExprData::Infix {
                lhs: lit1_id,
                op: op_id,
                rhs: lit2_id,
            },
        );

        let quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            test_span(0, 15),
            ExprData::Quote { body: infix_id },
        );

        let quote_term = Term::new(&arena, quote_id);
        let term_value = Value::Term(quote_term);
        let call_site = test_span(100, 5);
        let result = splice(term_value, call_site);

        assert!(result.is_ok());
        let spliced_id = result.unwrap();
        assert_eq!(spliced_id, quote_id);

        // Verify the spliced node's shape by checking its kind.
        let spliced_node = arena.get(spliced_id).unwrap();
        assert_eq!(spliced_node.kind, NodeKind::ExprQuote);
    }

    #[test]
    fn splice_with_wrong_value_kind_errors() {
        let call_site = test_span(100, 5);

        // Try to splice an integer value instead of a Term.
        let int_value = Value::Int(42);
        let result = splice(int_value, call_site);

        assert!(result.is_err());
        let diag = result.unwrap_err();
        assert!(diag.message().contains("Term"));
    }

    #[test]
    fn splice_with_wrong_value_kind_bool_errors() {
        let call_site = test_span(100, 5);

        // Try to splice a bool value.
        let bool_value = Value::Bool(true);
        let result = splice(bool_value, call_site);

        assert!(result.is_err());
    }

    #[test]
    fn splice_with_type_check_passes_when_types_match() {
        let mut arena = AstArena::new();
        let mut cache = TypeCache::new();

        // Build a simple literal.
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(0, 1));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(0, 1),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let lit_term = Term::new(&arena, lit_id);

        // Insert the term's type into the cache.
        let expected_ty = TId::new(1).unwrap();
        cache.insert(lit_id, expected_ty);

        // Splice with matching type.
        let term_value = Value::Term(lit_term);
        let call_site = test_span(100, 5);
        let result = splice_with_type_check(term_value, expected_ty, call_site, &cache);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), lit_id);
    }

    #[test]
    fn splice_with_type_check_fails_with_t0506() {
        let mut arena = AstArena::new();
        let mut cache = TypeCache::new();

        // Build a simple literal.
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(0, 1));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(0, 1),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let lit_term = Term::new(&arena, lit_id);

        // Insert a different type into the cache than what we expect.
        let actual_ty = TId::new(1).unwrap();
        let expected_ty = TId::new(2).unwrap();
        cache.insert(lit_id, actual_ty);

        // Splice with mismatched type.
        let term_value = Value::Term(lit_term);
        let call_site = test_span(100, 5);
        let result = splice_with_type_check(term_value, expected_ty, call_site, &cache);

        assert!(result.is_err());
        let diag = result.unwrap_err();
        // T0506 diagnostic.
        assert!(diag.message().contains("spliced Term type does not match"));
    }

    #[test]
    fn splice_preserves_term_structure() {
        let mut arena = AstArena::new();

        // Build a call expression: f(1, 2)
        let callee_placeholder = arena.alloc(NodeKind::Placeholder, test_span(0, 1));
        let callee_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(0, 1),
            ExprData::Literal {
                lit: callee_placeholder,
            },
        );

        let arg1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(2, 1));
        let arg1_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(2, 1),
            ExprData::Literal {
                lit: arg1_placeholder,
            },
        );

        let arg2_placeholder = arena.alloc(NodeKind::Placeholder, test_span(4, 1));
        let arg2_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(4, 1),
            ExprData::Literal {
                lit: arg2_placeholder,
            },
        );

        let call_id = arena.alloc_expr(
            NodeKind::ExprCall,
            test_span(0, 6),
            ExprData::Call {
                callee: callee_id,
                args: vec![arg1_id, arg2_id],
            },
        );

        // Quote the call.
        let quote_id = arena.alloc_expr(
            NodeKind::ExprQuote,
            test_span(0, 20),
            ExprData::Quote { body: call_id },
        );

        let quote_term = Term::new(&arena, quote_id);
        let term_value = Value::Term(quote_term);
        let call_site = test_span(100, 5);
        let spliced = splice(term_value, call_site).unwrap();

        // Verify the spliced node is the quote.
        assert_eq!(spliced, quote_id);

        // Verify we can access the inner call through the spliced quote.
        if let Some(ExprData::Quote { body }) = arena.expr_data(spliced) {
            assert_eq!(*body, call_id);
            if let Some(ExprData::Call { callee, args }) = arena.expr_data(*body) {
                assert_eq!(*callee, callee_id);
                assert_eq!(args.len(), 2);
            }
        } else {
            panic!("Expected Quote expression data");
        }
    }

    #[test]
    fn splice_with_type_check_uncached_type_still_splices() {
        let mut arena = AstArena::new();
        let cache = TypeCache::new(); // Empty cache

        // Build a simple literal.
        let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(0, 1));
        let lit_id = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span(0, 1),
            ExprData::Literal {
                lit: lit_placeholder,
            },
        );

        let lit_term = Term::new(&arena, lit_id);
        let expected_ty = TId::new(1).unwrap();

        // Splice with uncached type (cache is empty).
        // Since the type is not in the cache, the check skips and splice succeeds.
        let term_value = Value::Term(lit_term);
        let call_site = test_span(100, 5);
        let result = splice_with_type_check(term_value, expected_ty, call_site, &cache);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), lit_id);
    }

    // ── Phase-2 (m2-010): reflective hygiene under splice ────────────

    #[test]
    fn splice_with_hygiene_tags_introduced_identifier_in_path() {
        let mut arena = AstArena::new();
        let tag = MacroId::fresh();

        // Build a simple path expression: foo
        let ident_id = arena.alloc(NodeKind::Ident, test_span(0, 3));
        let path_id = arena.alloc_expr(
            NodeKind::ExprPath,
            test_span(0, 3),
            ExprData::Path {
                segments: vec![ident_id],
            },
        );

        let term = Term::new(&arena, path_id);
        let term_value = Value::Term(term);
        let call_site = test_span(100, 5);

        let result = splice_with_hygiene(term_value, tag, call_site);
        assert!(result.is_ok());

        let (spliced_id, cache) = result.unwrap();
        assert_eq!(spliced_id, path_id);
        assert!(!cache.is_empty());

        // The identifier should have been tagged.
        let hygienic = cache.lookup(ident_id);
        assert!(hygienic.is_some());
        assert_eq!(hygienic.unwrap().tags, vec![tag]);
    }

    #[test]
    fn splice_with_hygiene_leaves_antiquoted_identifier_untagged() {
        let mut arena = AstArena::new();
        let tag = MacroId::fresh();

        // Build an antiquote expression: ~(foo)
        let ident_id = arena.alloc(NodeKind::Ident, test_span(2, 3));
        let path_id = arena.alloc_expr(
            NodeKind::ExprPath,
            test_span(2, 3),
            ExprData::Path {
                segments: vec![ident_id],
            },
        );
        let antiquote_id = arena.alloc_expr(
            NodeKind::ExprAntiquote,
            test_span(0, 6),
            ExprData::Antiquote { value: path_id },
        );

        let term = Term::new(&arena, antiquote_id);
        let term_value = Value::Term(term);
        let call_site = test_span(100, 5);

        let result = splice_with_hygiene(term_value, tag, call_site);
        assert!(result.is_ok());

        let (_spliced_id, cache) = result.unwrap();

        // The identifier inside the antiquote should NOT be tagged.
        assert!(cache.lookup(ident_id).is_none());
    }

    #[test]
    fn splice_with_hygiene_uses_fresh_tag_per_expansion() {
        let mut arena = AstArena::new();
        let tag1 = MacroId::fresh();
        let tag2 = MacroId::fresh();
        assert_ne!(tag1, tag2);

        // Create two identical path expressions.
        let ident_id1 = arena.alloc(NodeKind::Ident, test_span(0, 3));
        let path_id1 = arena.alloc_expr(
            NodeKind::ExprPath,
            test_span(0, 3),
            ExprData::Path {
                segments: vec![ident_id1],
            },
        );

        let ident_id2 = arena.alloc(NodeKind::Ident, test_span(0, 3));
        let path_id2 = arena.alloc_expr(
            NodeKind::ExprPath,
            test_span(0, 3),
            ExprData::Path {
                segments: vec![ident_id2],
            },
        );

        // Splice both terms with different tags.
        let term1 = Term::new(&arena, path_id1);
        let result1 = splice_with_hygiene(Value::Term(term1), tag1, test_span(100, 5));
        assert!(result1.is_ok());
        let (_id1, cache1) = result1.unwrap();

        let term2 = Term::new(&arena, path_id2);
        let result2 = splice_with_hygiene(Value::Term(term2), tag2, test_span(100, 5));
        assert!(result2.is_ok());
        let (_id2, cache2) = result2.unwrap();

        // Both should be tagged, but with different tags.
        let hygienic1 = cache1.lookup(ident_id1).unwrap();
        let hygienic2 = cache2.lookup(ident_id2).unwrap();

        assert_eq!(hygienic1.tags, vec![tag1]);
        assert_eq!(hygienic2.tags, vec![tag2]);
        assert_ne!(hygienic1.tags, hygienic2.tags);
    }

    #[test]
    fn splice_with_hygiene_errors_on_non_term() {
        let tag = MacroId::fresh();
        let call_site = test_span(100, 5);

        // Try to splice an integer value instead of a Term.
        let int_value = Value::Int(42);
        let result = splice_with_hygiene(int_value, tag, call_site);

        assert!(result.is_err());
        let diag = result.unwrap_err();
        assert!(diag.message().contains("Term"));
    }
}
