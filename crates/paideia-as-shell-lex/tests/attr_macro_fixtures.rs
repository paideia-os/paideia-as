//! PAS-DEBT-B2-019 attribute-macro sigil fixtures.
//!
//! Pre-fix, `@` was rejected as `LexErrorKind::UnexpectedChar` (the
//! R221.M4 comment on `LexErrorKind::UnexpectedChar` explicitly cited
//! it as an unhandled sigil). `#` remained a line comment (unchanged
//! by this fix) — retained here as a defence-in-depth fixture so a
//! future regression that turned `#foo` into `Hash + Ident` is caught.
//! Fingerprint `r221m4-attr-NN` / `pas-debt-b2-019-NN`.

mod common;
use common::{assert_lex, ident, strlit};
use paideia_as_shell_lex::{Context, Lexer, TokenKind};

#[test]
fn pas_debt_b2_019_01_at_bare_ident() {
    // `@foo` — two tokens, sigil then ident.
    assert_lex(
        "pas-debt-b2-019-01",
        "@foo",
        &[
            (TokenKind::At, Context::Pipeline),
            (ident("foo"), Context::Pipeline),
        ],
    );
}

#[test]
fn pas_debt_b2_019_02_at_fingerprint_call() {
    // The canonical `@fingerprint("...")` shape from R220.M10.
    assert_lex(
        "pas-debt-b2-019-02",
        r#"@fingerprint("r221m4-ctx-pipe-01")"#,
        &[
            (TokenKind::At, Context::Pipeline),
            (ident("fingerprint"), Context::Pipeline),
            (TokenKind::LParen, Context::Pipeline),
            (strlit("r221m4-ctx-pipe-01"), Context::Pipeline),
            (TokenKind::RParen, Context::Pipeline),
        ],
    );
}

#[test]
fn pas_debt_b2_019_03_at_include_str_call() {
    // A second attribute shape with a string arg — confirms the token
    // stream is not `@fingerprint`-specific.
    assert_lex(
        "pas-debt-b2-019-03",
        r#"@include_str("script.pds")"#,
        &[
            (TokenKind::At, Context::Pipeline),
            (ident("include_str"), Context::Pipeline),
            (TokenKind::LParen, Context::Pipeline),
            (strlit("script.pds"), Context::Pipeline),
            (TokenKind::RParen, Context::Pipeline),
        ],
    );
}

#[test]
fn pas_debt_b2_019_04_at_inside_lambda_context() {
    // Attribute inside a lambda body — `At` context tracks the stack.
    assert_lex(
        "pas-debt-b2-019-04",
        "{ |x| @tag x }",
        &[
            (TokenKind::LBrace, Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::Pipe, Context::Lambda),
            (TokenKind::At, Context::Lambda),
            (ident("tag"), Context::Lambda),
            (ident("x"), Context::Lambda),
            (TokenKind::RBrace, Context::Lambda),
        ],
    );
}

#[test]
fn pas_debt_b2_019_05_at_bare_no_ident_yields_at_only() {
    // Trailing `@` (no ident after) emits `At` and nothing else — the
    // parser at R221.M5 flags the missing name as its own diagnostic.
    // Confirms `@` never re-enters the error path.
    assert_lex(
        "pas-debt-b2-019-05",
        "@",
        &[(TokenKind::At, Context::Pipeline)],
    );
}

#[test]
fn pas_debt_b2_019_06_hash_remains_line_comment() {
    // `#` still starts a line comment: no tokens emitted from the `#`
    // onward, but tokens before/after (across a newline) survive. A
    // regression that turned `# foo` into `Hash + Ident` would fail
    // here.
    assert_lex(
        "pas-debt-b2-019-06",
        "ls # this is a comment\ncd",
        &[
            (ident("ls"), Context::Pipeline),
            (TokenKind::Newline, Context::Pipeline),
            (ident("cd"), Context::Pipeline),
        ],
    );
}

#[test]
fn pas_debt_b2_019_07_at_no_longer_lex_errors() {
    // Direct assertion on the iterator: pre-fix this produced
    // `Err(UnexpectedChar)`. Post-fix every item is `Ok`.
    let toks: Vec<_> = Lexer::new("@a @b").collect();
    assert!(
        toks.iter().all(|r| r.is_ok()),
        "pas-debt-b2-019-07: expected all-Ok, got {toks:?}",
    );
}
