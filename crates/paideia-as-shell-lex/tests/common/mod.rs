//! Shared test helpers for the R221.M4 lexer fixture corpus.
//!
//! Each fixture invokes [`assert_lex`] with a fingerprint tag, the
//! source string, and the exact `(TokenKind, Context)` sequence the
//! lexer should emit. Spans are elided from the assertion — the R221.M4
//! acceptance is context-tagging correctness; span accuracy is
//! R221.M6's own test corpus.

#![allow(dead_code)] // some helpers are used by only a subset of files

use paideia_as_shell_lex::{Context, Lexer, TokenKind};

/// Emit-order comparison of `(kind, context)` pairs. On mismatch, prints
/// both sequences side-by-side under the fingerprint tag for a
/// debuggable failure.
pub fn assert_lex(
    fingerprint: &str,
    src: &str,
    expected: &[(TokenKind, Context)],
) {
    let got: Vec<(TokenKind, Context)> = Lexer::new(src)
        .map(|r| {
            let t = r.unwrap_or_else(|e| {
                panic!("{fingerprint}: unexpected lex error {e:?} on src {src:?}")
            });
            (t.kind, t.context)
        })
        .collect();
    if got != expected {
        panic!(
            "{fingerprint}: token mismatch on src {src:?}\n  expected: {expected:#?}\n  got: {got:#?}"
        );
    }
    assert!(!fingerprint.is_empty());
}

/// Quick constructors so fixtures stay compact.
pub fn ident(s: &str) -> TokenKind {
    TokenKind::Ident(s.to_owned())
}
pub fn num(s: &str) -> TokenKind {
    TokenKind::Number(s.to_owned())
}
pub fn qvar(s: &str) -> TokenKind {
    TokenKind::QVar(s.to_owned())
}
pub fn interp(s: &str) -> TokenKind {
    TokenKind::InterpVar(s.to_owned())
}
pub fn strlit(s: &str) -> TokenKind {
    TokenKind::Str(s.to_owned())
}
pub fn op(s: &str) -> TokenKind {
    TokenKind::Op(s.to_owned())
}
