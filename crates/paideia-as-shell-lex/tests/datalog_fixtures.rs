//! R221.M4 pure-Datalog lexer fixtures (10).
//!
//! Every form is a bare `datalog { … }` block at top level. Expected
//! shape: the outer `datalog` keyword and closing whitespace are
//! `Pipeline`; the opening `{`, everything inside, and the closing `}`
//! are `Datalog`. Fingerprint `r221m4-ctx-dlg-NN`.

mod common;
use common::{assert_lex, ident, num, op, qvar, strlit};
use paideia_as_shell_lex::{Context, TokenKind};

#[test]
fn r221m4_ctx_dlg_01_empty_block() {
    assert_lex(
        "r221m4-ctx-dlg-01",
        "datalog { }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_dlg_02_single_fact() {
    assert_lex(
        "r221m4-ctx-dlg-02",
        "datalog { parent(alice, bob). }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("parent"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (ident("alice"), Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (ident("bob"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::Dot, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_dlg_03_rule_with_fat_arrow() {
    assert_lex(
        "r221m4-ctx-dlg-03",
        "datalog { parent(?X, ?Y) => ancestor(?X, ?Y) }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("parent"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("X"), Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (qvar("Y"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::FatArrow, Context::Datalog),
            (ident("ancestor"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("X"), Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (qvar("Y"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_dlg_04_logic_variables_in_query() {
    assert_lex(
        "r221m4-ctx-dlg-04",
        "datalog { author(?p, \"Knuth\"), cited_by(?p, ?q) }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("author"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("p"), Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (strlit("Knuth"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (ident("cited_by"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("p"), Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (qvar("q"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_dlg_05_multiple_atoms_on_multiple_lines() {
    assert_lex(
        "r221m4-ctx-dlg-05",
        "datalog {\n  a(1).\n  a(2).\n}",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (TokenKind::Newline, Context::Datalog),
            (ident("a"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (num("1"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::Dot, Context::Datalog),
            (TokenKind::Newline, Context::Datalog),
            (ident("a"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (num("2"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::Dot, Context::Datalog),
            (TokenKind::Newline, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_dlg_06_comparison_op_in_body() {
    assert_lex(
        "r221m4-ctx-dlg-06",
        "datalog { modified(?f, ?t), ?t > 100 }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("modified"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("f"), Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (qvar("t"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (qvar("t"), Context::Datalog),
            (op(">"), Context::Datalog),
            (num("100"), Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_dlg_07_negation_via_keyword_op() {
    assert_lex(
        "r221m4-ctx-dlg-07",
        "datalog { p(?x), not q(?x) }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("p"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::Comma, Context::Datalog),
            (op("not"), Context::Datalog),
            (ident("q"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_dlg_08_multiple_rules() {
    assert_lex(
        "r221m4-ctx-dlg-08",
        "datalog { p(?x) => q(?x); q(?x) => r(?x) }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("p"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::FatArrow, Context::Datalog),
            (ident("q"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::Semi, Context::Datalog),
            (ident("q"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::FatArrow, Context::Datalog),
            (ident("r"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_dlg_09_and_or_keyword_ops_in_body() {
    assert_lex(
        "r221m4-ctx-dlg-09",
        "datalog { a(?x) and b(?x) or c(?x) }",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("a"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (op("and"), Context::Datalog),
            (ident("b"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (op("or"), Context::Datalog),
            (ident("c"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
        ],
    );
}

#[test]
fn r221m4_ctx_dlg_10_after_block_returns_to_pipeline() {
    // Sanity check: any token after the closing `}` should carry
    // Context::Pipeline again (stack popped).
    assert_lex(
        "r221m4-ctx-dlg-10",
        "datalog { p(?x) } sort",
        &[
            (TokenKind::DatalogKw, Context::Pipeline),
            (TokenKind::LBrace, Context::Datalog),
            (ident("p"), Context::Datalog),
            (TokenKind::LParen, Context::Datalog),
            (qvar("x"), Context::Datalog),
            (TokenKind::RParen, Context::Datalog),
            (TokenKind::RBrace, Context::Datalog),
            (ident("sort"), Context::Pipeline),
        ],
    );
}
