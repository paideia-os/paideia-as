//! R221.M4 pure-Pipeline lexer fixtures (10).
//!
//! Every token in these forms must carry `Context::Pipeline` — no
//! `{` opens a Datalog or Lambda block. Fingerprint `r221m4-ctx-pipe-NN`.

mod common;
use common::{assert_lex, ident, num, op, strlit};
use paideia_as_shell_lex::{Context, TokenKind};

#[test]
fn r221m4_ctx_pipe_01_bare_command() {
    assert_lex(
        "r221m4-ctx-pipe-01",
        "ls",
        &[(ident("ls"), Context::Pipeline)],
    );
}

#[test]
fn r221m4_ctx_pipe_02_two_stage_pipeline() {
    assert_lex(
        "r221m4-ctx-pipe-02",
        "ls | grep foo",
        &[
            (ident("ls"), Context::Pipeline),
            (TokenKind::Pipe, Context::Pipeline),
            (ident("grep"), Context::Pipeline),
            (ident("foo"), Context::Pipeline),
        ],
    );
}

#[test]
fn r221m4_ctx_pipe_03_three_stage_pipeline() {
    assert_lex(
        "r221m4-ctx-pipe-03",
        "cat file.txt | sort | uniq",
        &[
            (ident("cat"), Context::Pipeline),
            (ident("file"), Context::Pipeline),
            (TokenKind::Dot, Context::Pipeline),
            (ident("txt"), Context::Pipeline),
            (TokenKind::Pipe, Context::Pipeline),
            (ident("sort"), Context::Pipeline),
            (TokenKind::Pipe, Context::Pipeline),
            (ident("uniq"), Context::Pipeline),
        ],
    );
}

#[test]
fn r221m4_ctx_pipe_04_string_argument() {
    assert_lex(
        "r221m4-ctx-pipe-04",
        r#"echo "hello world""#,
        &[
            (ident("echo"), Context::Pipeline),
            (strlit("hello world"), Context::Pipeline),
        ],
    );
}

#[test]
fn r221m4_ctx_pipe_05_numeric_argument() {
    assert_lex(
        "r221m4-ctx-pipe-05",
        "head 10",
        &[
            (ident("head"), Context::Pipeline),
            (num("10"), Context::Pipeline),
        ],
    );
}

#[test]
fn r221m4_ctx_pipe_06_semicolon_separator() {
    assert_lex(
        "r221m4-ctx-pipe-06",
        "cd /tmp; ls",
        &[
            (ident("cd"), Context::Pipeline),
            (op("/"), Context::Pipeline),
            (ident("tmp"), Context::Pipeline),
            (TokenKind::Semi, Context::Pipeline),
            (ident("ls"), Context::Pipeline),
        ],
    );
}

#[test]
fn r221m4_ctx_pipe_07_newline_separator() {
    assert_lex(
        "r221m4-ctx-pipe-07",
        "ls\ncd",
        &[
            (ident("ls"), Context::Pipeline),
            (TokenKind::Newline, Context::Pipeline),
            (ident("cd"), Context::Pipeline),
        ],
    );
}

#[test]
fn r221m4_ctx_pipe_08_paren_grouping() {
    assert_lex(
        "r221m4-ctx-pipe-08",
        "sum (1 + 2)",
        &[
            (ident("sum"), Context::Pipeline),
            (TokenKind::LParen, Context::Pipeline),
            (num("1"), Context::Pipeline),
            (op("+"), Context::Pipeline),
            (num("2"), Context::Pipeline),
            (TokenKind::RParen, Context::Pipeline),
        ],
    );
}

#[test]
fn r221m4_ctx_pipe_09_comparison_operators() {
    assert_lex(
        "r221m4-ctx-pipe-09",
        "filter x >= 10",
        &[
            (ident("filter"), Context::Pipeline),
            (ident("x"), Context::Pipeline),
            (op(">="), Context::Pipeline),
            (num("10"), Context::Pipeline),
        ],
    );
}

#[test]
fn r221m4_ctx_pipe_10_field_access_dot_stays_pipeline() {
    // `f.size` in Pipeline context: two idents joined by a Dot; the
    // parser at R221.M5 disambiguates field-access from Datalog fact-
    // terminator by context. Here context stays Pipeline throughout.
    assert_lex(
        "r221m4-ctx-pipe-10",
        "sort by f.size desc",
        &[
            (ident("sort"), Context::Pipeline),
            (ident("by"), Context::Pipeline),
            (ident("f"), Context::Pipeline),
            (TokenKind::Dot, Context::Pipeline),
            (ident("size"), Context::Pipeline),
            (ident("desc"), Context::Pipeline),
        ],
    );
}
