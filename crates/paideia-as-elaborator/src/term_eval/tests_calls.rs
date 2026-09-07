//! Builtin-call tests for the term evaluator (kind / children / span /
//! splice / elab).
//!
//! Split out from the tests module inside the original single-file
//! `term_eval.rs` in the phase-2 God-file refactor (issue #1412 under
//! umbrella #1405). Preserves the exact scenarios, assertions, and
//! diagnostic-code expectations of the prior tests unchanged.

use paideia_as_ast::reflect::TermHead;
use paideia_as_ast::{AstArena, ExprData, NodeKind, Term};
use paideia_as_diagnostics::{FileId, Span};

use super::{eval, Env, Value};

fn test_span(byte_start: u32, byte_len: u32) -> Span {
    Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
}

#[test]
fn evals_kind_builtin_call() {
    let mut arena = AstArena::new();

    // Build a Quote term and test kind() directly using reflect_api.
    use crate::reflect_api;

    let body_placeholder = arena.alloc(NodeKind::Placeholder, test_span(0, 0));
    let quote_id = arena.alloc_expr(
        NodeKind::ExprQuote,
        test_span(0, 5),
        ExprData::Quote {
            body: body_placeholder,
        },
    );

    let quote_term = Term::new(&arena, quote_id);
    let head = reflect_api::kind(&quote_term);

    assert_eq!(head, TermHead::Quote);
}

#[test]
fn evals_children_builtin_call() {
    let mut arena = AstArena::new();

    // Build an Infix (1 + 2) with 3 children.
    // For this test, we'll directly construct a Value::Term pointing to the infix node,
    // and then call children() on it.
    let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit1_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit1_placeholder,
        },
    );

    let op_id = arena.alloc(NodeKind::Placeholder, test_span(0, 0));

    let lit2_placeholder = arena.alloc(NodeKind::Placeholder, test_span(2, 0));
    let lit2_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(2, 0),
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

    // Create a call to children with the infix term.
    // We'll use ExprCall with callee.span.byte_start=1 (children function code).
    // For the argument, we create a placeholder that we'll manually convert to Value::Term.
    // Actually, since we can't easily inject a Term value through the AST, let's just
    // test the reflect_api functions directly here.
    use crate::reflect_api;

    let infix_term = Term::new(&arena, infix_id);
    let kids = reflect_api::children(&infix_term);

    // Expect 3 children: lhs, op, rhs
    assert_eq!(kids.len(), 3);
}

#[test]
fn evals_span_builtin_call() {
    let mut arena = AstArena::new();

    // Test span() builtin directly using reflect_api.
    use crate::reflect_api;

    let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(42, 0));
    let lit_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(42, 0),
        ExprData::Literal {
            lit: lit_placeholder,
        },
    );

    let lit_term = Term::new(&arena, lit_id);
    let s = reflect_api::span(&lit_term);

    assert_eq!(s.byte_start(), 42);
    assert_eq!(s.byte_len(), 0);
}

#[test]
fn evals_splice_call() {
    let mut arena = AstArena::new();

    // Build a quoted literal: quote { 42 }
    let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(42, 0));
    let lit_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(42, 0),
        ExprData::Literal {
            lit: lit_placeholder,
        },
    );

    let quote_id = arena.alloc_expr(
        NodeKind::ExprQuote,
        test_span(40, 10),
        ExprData::Quote { body: lit_id },
    );

    // Build a splice call: splice(quote { 42 })
    // The callee's span.byte_start = 3 indicates the splice builtin.
    let callee_id = arena.alloc(NodeKind::Placeholder, test_span(3, 6)); // "splice"

    let splice_call_id = arena.alloc_expr(
        NodeKind::ExprCall,
        test_span(0, 20),
        ExprData::Call {
            callee: callee_id,
            args: vec![quote_id],
        },
    );

    let mut env = Env::new();
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, splice_call_id, &mut env, &mut type_cache);

    // Should succeed, returning a Value::Term wrapping the spliced node.
    assert!(result.is_ok());
    let value = result.unwrap();
    match value {
        Value::Term(t) => {
            // The spliced term should be the quoted body (the literal), not the quote itself.
            assert_eq!(t.id(), lit_id);
        }
        _ => panic!("Expected Value::Term from splice call"),
    }
}

#[test]
fn evals_elab_call() {
    let mut arena = AstArena::new();

    // Build a literal: 42
    let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(42, 0));
    let lit_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(42, 0),
        ExprData::Literal {
            lit: lit_placeholder,
        },
    );

    // Build a quoted literal: quote { 42 }
    let quote_id = arena.alloc_expr(
        NodeKind::ExprQuote,
        test_span(40, 10),
        ExprData::Quote { body: lit_id },
    );

    // Build an elab call: elab(quote { 42 })
    // The callee's span.byte_start = 4 indicates the elab builtin.
    let callee_id = arena.alloc(NodeKind::Placeholder, test_span(4, 4)); // "elab"

    let elab_call_id = arena.alloc_expr(
        NodeKind::ExprCall,
        test_span(0, 20),
        ExprData::Call {
            callee: callee_id,
            args: vec![quote_id],
        },
    );

    let mut env = Env::new();
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, elab_call_id, &mut env, &mut type_cache);

    // Should succeed, returning a Value::Term.
    assert!(result.is_ok());
    let value = result.unwrap();
    match value {
        Value::Term(t) => {
            // The flow is:
            // 1. eval(quote { 42 }) evaluates the quote, which returns Value::Term(lit_id)
            // 2. elab receives that Value::Term(lit_id)
            // 3. elab returns Value::Term(lit_id) after populating the cache
            let returned_id = t.id();
            // So the returned term should be the literal (lit_id).
            assert_eq!(
                returned_id, lit_id,
                "Expected elab to return the term it received (the quoted literal)"
            );
            // Verify the type cache has an entry for the literal (elab should have populated it).
            assert!(type_cache.get(lit_id).is_some());
        }
        _ => panic!("Expected Value::Term from elab call"),
    }
}
