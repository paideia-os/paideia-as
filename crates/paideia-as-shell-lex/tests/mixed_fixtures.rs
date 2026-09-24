//! R221.M4 mixed / nested context lexer fixtures (10).
//!
//! Exercises stack depth 1..=3 — a Datalog block inside a Lambda body
//! inside a Pipeline stage, and vice versa. Fingerprint
//! `r221m4-ctx-mix-NN`.
//!
//! Nesting is the acceptance-criterion crux: a naïve implementation
//! that flips a single `in_datalog` flag will get the family of forms
//! `datalog { … lambda-atom … } | { |x| datalog { … } }` wrong. These
//! fixtures pin the stack semantics.

mod common;
use common::{assert_lex, ident, num, qvar, interp, op};
use paideia_as_shell_lex::{Context, TokenKind};

#[test]
fn r221m4_ctx_mix_01_lambda_inside_pipeline_stage() {
    // Depth 1 → 2 → 1. `filter` is Pipeline, the block is Lambda.
    assert_lex(
        "r221m4-ctx-mix-01",
        "ls | filter { |f| f.size > 100 }",
        &[
            (ident("ls"), Context::Pipeline),
            (TokenKind::Pipe, Context::Pipeline),
            (ident("filter"), Context::Pipeline),
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("f"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("f"), Context::Lambda),
            (TokenKind::Dot, Context::Lambda),
            (ident("size"), Context::Lambda),
            (op(">"), Context::Lambda),
            (num("100"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_mix_02_datalog_inside_pipeline_stage() {
    assert_lex(
        "r221m4-ctx-mix-02",
        "files | datalog { tagged(?f, \"research\") }",
        &[
            (ident("files"), Context::Pipeline),
            (TokenKind::Pipe, Context::Pipeline),
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("tagged"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("f"), Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (TokenKind::Str("research".into()), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_mix_03_datalog_with_pipeline_interpolation() {
    // The `$it` in Datalog is `Context::Datalog` (inside the block),
    // even though it references a pipeline value.
    assert_lex(
        "r221m4-ctx-mix-03",
        "each | datalog { pred($it, ?x) }",
        &[
            (ident("each"), Context::Pipeline),
            (TokenKind::Pipe, Context::Pipeline),
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("pred"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (interp("it"), Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_mix_04_lambda_containing_datalog_depth_3() {
    // The crux fixture: stack depth reaches 3.
    // Pipeline base ← Lambda from each's `{` ← Datalog from `datalog {`.
    assert_lex(
        "r221m4-ctx-mix-04",
        "ls | each { |x| datalog { pred($x, ?y) } }",
        &[
            (ident("ls"), Context::Pipeline),
            (TokenKind::Pipe, Context::Pipeline),
            (ident("each"), Context::Pipeline),
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (TokenKind::DatalogKw, Context::Lambda),
            (TokenKind::LBrace, Context::Datalog),
            (ident("pred"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (interp("x"), Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (qvar("y"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_mix_05_lambda_inside_datalog_body() {
    // A lambda embedded as a Datalog argument value. Depth 3.
    assert_lex(
        "r221m4-ctx-mix-05",
        "datalog { p(?x, { |y| y + 1 }) }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("p"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("y"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("y"), Context::Lambda),
            (op("+"), Context::Lambda),
            (num("1"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_mix_06_two_lambdas_in_one_pipeline() {
    // Two adjacent Lambda opens don't nest — each is opened and closed
    // before the next opens. Depth stays 1 ↔ 2, never 3.
    assert_lex(
        "r221m4-ctx-mix-06",
        "ls | filter { |f| f.size > 0 } | sort by { |f| f.name }",
        &[
            (ident("ls"), Context::Pipeline),
            (TokenKind::Pipe, Context::Pipeline),
            (ident("filter"), Context::Pipeline),
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("f"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("f"), Context::Lambda),
            (TokenKind::Dot, Context::Lambda),
            (ident("size"), Context::Lambda),
            (op(">"), Context::Lambda),
            (num("0"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Pipeline),
            (ident("sort"), Context::Pipeline),
            (ident("by"), Context::Pipeline),
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("f"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("f"), Context::Lambda),
            (TokenKind::Dot, Context::Lambda),
            (ident("name"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_mix_07_datalog_then_pipeline_then_lambda() {
    // `datalog { … } | filter { |x| x }`. Depth alternates 1↔2 twice.
    assert_lex(
        "r221m4-ctx-mix-07",
        "datalog { a(1). } | filter { |x| x }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("a"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (num("1"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::Dot, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
            (TokenKind::Pipe, Context::Pipeline),
            (ident("filter"), Context::Pipeline),
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_mix_08_datalog_keyword_only_gates_next_brace() {
    // If `datalog` is followed by NOT `{` (e.g. a pipe first), the
    // pending latch clears and the next `{` (if any) opens Lambda,
    // not Datalog. This defends against a naive implementation that
    // waits arbitrarily for the next `{`.
    assert_lex(
        "r221m4-ctx-mix-08",
        "datalog | filter { |x| x }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::Pipe, Context::Pipeline),
            (ident("filter"), Context::Pipeline),
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_mix_09_deeply_nested_stack_depth_3() {
    // Depth 3 achieved differently: Lambda outer, Lambda inner, then
    // Datalog innermost. Verifies the stack (not a max-depth-2
    // register) tracks correctly.
    assert_lex(
        "r221m4-ctx-mix-09",
        "each { |a| { |b| datalog { p(?c) } } }",
        &[
            (ident("each"), Context::Pipeline),
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("a"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("b"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (TokenKind::DatalogKw, Context::Lambda),
            (TokenKind::LBrace, Context::Datalog),
            (ident("p"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("c"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
            (TokenKind::RBrace, Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_mix_10_context_returns_to_pipeline_after_all_pops() {
    // After the last `}` in a deeply-nested form, subsequent tokens
    // must land in Context::Pipeline again. This is the invariant the
    // R229 REPL uses to know when the multi-line input is complete.
    assert_lex(
        "r221m4-ctx-mix-10",
        "each { |a| datalog { p(?a) } } sort",
        &[
            (ident("each"), Context::Pipeline),
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("a"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (TokenKind::DatalogKw, Context::Lambda),
            (TokenKind::LBrace, Context::Datalog),
            (ident("p"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("a"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
            (TokenKind::RBrace, Context::Lambda),
            (ident("sort"), Context::Pipeline),
        ],
    );
}
