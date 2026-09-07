//! Arithmetic + literal + type-mismatch tests for the term evaluator.
//!
//! Split out from the tests module inside the original single-file
//! `term_eval.rs` in the phase-2 God-file refactor (issue #1412 under
//! umbrella #1405). Preserves the exact scenarios, assertions, and
//! diagnostic-code expectations of the prior tests unchanged.

use paideia_as_ast::{AstArena, ExprData, NodeKind};
use paideia_as_diagnostics::{FileId, Span};

use super::{eval, Env, Value};

fn test_span(byte_start: u32, byte_len: u32) -> Span {
    Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
}

#[test]
fn evals_int_literal() {
    let mut arena = AstArena::new();
    let lit_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit_placeholder,
        },
    );

    let mut env = Env::new();
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, lit_id, &mut env, &mut type_cache);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Value::Int(1));
}

#[test]
fn evals_int_addition() {
    let mut arena = AstArena::new();

    // Build 1 + 2
    let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit1_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit1_placeholder,
        },
    );

    let op_id = arena.alloc(NodeKind::Placeholder, test_span(0, 0)); // + operator

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
        test_span(0, 3),
        ExprData::Infix {
            lhs: lit1_id,
            op: op_id,
            rhs: lit2_id,
        },
    );

    let mut env = Env::new();
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, infix_id, &mut env, &mut type_cache);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Value::Int(3));
}

#[test]
fn evals_int_subtraction() {
    let mut arena = AstArena::new();

    // Build 5 - 3
    let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(5, 0));
    let lit1_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(5, 0),
        ExprData::Literal {
            lit: lit1_placeholder,
        },
    );

    let op_id = arena.alloc(NodeKind::Placeholder, test_span(1, 0)); // - operator

    let lit2_placeholder = arena.alloc(NodeKind::Placeholder, test_span(3, 0));
    let lit2_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(3, 0),
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

    let mut env = Env::new();
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, infix_id, &mut env, &mut type_cache);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Value::Int(2));
}

#[test]
fn evals_int_multiplication() {
    let mut arena = AstArena::new();

    // Build 3 * 4
    let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(3, 0));
    let lit1_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(3, 0),
        ExprData::Literal {
            lit: lit1_placeholder,
        },
    );

    let op_id = arena.alloc(NodeKind::Placeholder, test_span(2, 0)); // * operator

    let lit2_placeholder = arena.alloc(NodeKind::Placeholder, test_span(4, 0));
    let lit2_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(4, 0),
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

    let mut env = Env::new();
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, infix_id, &mut env, &mut type_cache);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Value::Int(12));
}

#[test]
fn evals_equality_comparison() {
    let mut arena = AstArena::new();

    // Build 5 == 5
    let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(5, 0));
    let lit1_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(5, 0),
        ExprData::Literal {
            lit: lit1_placeholder,
        },
    );

    let op_id = arena.alloc(NodeKind::Placeholder, test_span(3, 0)); // == operator

    let lit2_placeholder = arena.alloc(NodeKind::Placeholder, test_span(5, 0));
    let lit2_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(5, 0),
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

    let mut env = Env::new();
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, infix_id, &mut env, &mut type_cache);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Value::Bool(true));
}

#[test]
fn evals_type_mismatch_emits_diagnostic() {
    let mut arena = AstArena::new();

    // 1 + true
    let lit1_placeholder = arena.alloc(NodeKind::Placeholder, test_span(1, 0));
    let lit1_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(1, 0),
        ExprData::Literal {
            lit: lit1_placeholder,
        },
    );

    let op_id = arena.alloc(NodeKind::Placeholder, test_span(0, 0)); // + operator

    // true literal: byte_len=1 means true
    let true_placeholder = arena.alloc(NodeKind::Placeholder, test_span(0, 1));
    let true_id = arena.alloc_expr(
        NodeKind::ExprLiteral,
        test_span(0, 1),
        ExprData::Literal {
            lit: true_placeholder,
        },
    );

    let infix_id = arena.alloc_expr(
        NodeKind::ExprInfix,
        test_span(0, 5),
        ExprData::Infix {
            lhs: lit1_id,
            op: op_id,
            rhs: true_id,
        },
    );

    let mut env = Env::new();
    let mut type_cache = crate::reflect_api::TypeCache::new();
    let result = eval(&arena, infix_id, &mut env, &mut type_cache);

    assert!(result.is_err());
    let diag = result.unwrap_err();
    assert!(diag.message().contains("type mismatch") || diag.message().contains("expected"));
}
