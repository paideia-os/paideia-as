//! R221.M4 pure-Lambda lexer fixtures (10).
//!
//! Every form is a bare `{ |args| body }` (or `\args -> body`) lambda
//! at top level. Inside the braces every token carries `Context::Lambda`.
//! Fingerprint `r221m4-ctx-lam-NN`.

mod common;
use common::{assert_lex, ident, num, op, strlit};
use paideia_as_shell_lex::{Context, TokenKind};

#[test]
fn r221m4_ctx_lam_01_identity_lambda() {
    assert_lex(
        "r221m4-ctx-lam-01",
        "{ |x| x }",
        &[
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
fn r221m4_ctx_lam_02_increment() {
    assert_lex(
        "r221m4-ctx-lam-02",
        "{ |x| x + 1 }",
        &[
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (op("+"), Context::Lambda),
            (num("1"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_lam_03_two_args() {
    assert_lex(
        "r221m4-ctx-lam-03",
        "{ |x, y| x + y }",
        &[
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::Comma, Context::Lambda),
            (ident("y"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (op("+"), Context::Lambda),
            (ident("y"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_lam_04_comparison_body() {
    assert_lex(
        "r221m4-ctx-lam-04",
        "{ |f| f.size > 100 }",
        &[
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
fn r221m4_ctx_lam_05_string_body() {
    assert_lex(
        "r221m4-ctx-lam-05",
        r#"{ |x| "prefix" }"#,
        &[
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (strlit("prefix"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_lam_06_backslash_arrow_form() {
    // The `\args -> body` alternative surface from SH-D5 §2.1. The
    // lexer does not open a Lambda context here (no `{`); this fixture
    // documents that the top-level surface stays `Pipeline` until an
    // enclosing block groups it. R221.M5 will decide how to parse the
    // Backslash + ThinArrow into a lambda; the lexer just delivers the
    // tokens.
    assert_lex(
        "r221m4-ctx-lam-06",
        "\\x -> x + 1",
        &[
            (TokenKind::Backslash, Context::Pipeline),
            (ident("x"), Context::Pipeline),
            (TokenKind::ThinArrow, Context::Pipeline),
            (ident("x"), Context::Pipeline),
            (op("+"), Context::Pipeline),
            (num("1"), Context::Pipeline),
        ],
    );
}

#[test]
fn r221m4_ctx_lam_07_multiline_body() {
    assert_lex(
        "r221m4-ctx-lam-07",
        "{ |x|\n  x + 1\n}",
        &[
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (TokenKind::Newline, Context::Lambda),
            (ident("x"), Context::Lambda),
            (op("+"), Context::Lambda),
            (num("1"), Context::Lambda),
            (TokenKind::Newline, Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_lam_08_and_or_ops() {
    assert_lex(
        "r221m4-ctx-lam-08",
        "{ |f| f.size > 100 and f.ext == 1 }",
        &[
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("f"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("f"), Context::Lambda),
            (TokenKind::Dot, Context::Lambda),
            (ident("size"), Context::Lambda),
            (op(">"), Context::Lambda),
            (num("100"), Context::Lambda),
            (op("and"), Context::Lambda),
            (ident("f"), Context::Lambda),
            (TokenKind::Dot, Context::Lambda),
            (ident("ext"), Context::Lambda),
            (op("=="), Context::Lambda),
            (num("1"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn r221m4_ctx_lam_09_after_lambda_returns_to_pipeline() {
    assert_lex(
        "r221m4-ctx-lam-09",
        "{ |x| x } sort",
        &[
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
            (ident("sort"), Context::Pipeline),
        ],
    );
}

#[test]
fn r221m4_ctx_lam_10_paren_group_in_body() {
    assert_lex(
        "r221m4-ctx-lam-10",
        "{ |x| (x + 1) * 2 }",
        &[
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (TokenKind::LParen, Context::Lambda),
            (ident("x"), Context::Lambda),
            (op("+"), Context::Lambda),
            (num("1"), Context::Lambda),
            (TokenKind::RParen, Context::Lambda),
            (op("*"), Context::Lambda),
            (num("2"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}
