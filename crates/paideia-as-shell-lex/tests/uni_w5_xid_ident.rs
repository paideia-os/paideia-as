//! UNI-W5 wire-up fixtures (paideia-as#1599) for the lexer's
//! identifier predicates.
//!
//! Each fixture asserts a difference between the UAX #31 XID surface
//! (UNI-4 — `is_xid_start` / `is_xid_continue`) and the pre-UNI-W5
//! `is_alphabetic` / `is_alphanumeric` approximation. The lexer's
//! post-wire predicates admit identifiers UAX #31 says are valid
//! (notably the Catalan `l·l` ligature) that the pre-wire predicates
//! truncated.

mod common;

use common::{assert_lex, ident};
use paideia_as_shell_lex::{Context, Lexer, TokenKind};

/// uni-w5-lex-01 — Catalan `l·l` + ASCII suffix is one identifier.
///
/// U+00B7 `MIDDLE DOT` is `XID_Continue` per UAX #31 (Catalan uses it
/// in the `l·l` digraph) but is NOT alphanumeric per
/// `char::is_alphanumeric`. The pre-UNI-W5 lexer therefore stopped at
/// the first `l`, emitted it as an `Ident`, then crashed into the
/// middle dot as `UnexpectedChar`. The post-wire lexer takes the
/// full ident.
#[test]
fn uni_w5_lex_01_middle_dot_in_ident() {
    let src = "l\u{00B7}lana";
    assert_lex(
        "uni-w5-lex-01",
        src,
        &[(ident("l\u{00B7}lana"), Context::Pipeline)],
    );
}

/// uni-w5-lex-02 — Greek letter identifier regression guard.
///
/// `αβγ` is accepted by both the pre- and post-wire predicates; this
/// fixture guards against a tightening-gone-wrong (XID_Start IS a
/// strict subset of ID_Start, and Greek letters sit in both).
#[test]
fn uni_w5_lex_02_greek_ident() {
    assert_lex(
        "uni-w5-lex-02",
        "αβγ",
        &[(ident("αβγ"), Context::Pipeline)],
    );
}

/// uni-w5-lex-03 — digit-first names are not idents.
///
/// `1abc` tokenises as `Number("1")` followed by `Ident("abc")` under
/// both the pre- and post-wire predicates. XID_Start explicitly
/// excludes ASCII digits (the UAX #31 "program identifier" shape), so
/// the regression here is "the manual `_` branch does not accidentally
/// admit digits as starts". Covers that corner.
#[test]
fn uni_w5_lex_03_digit_first_is_number_then_ident() {
    let tokens: Vec<TokenKind> = Lexer::new("1abc")
        .map(|r| r.expect("uni-w5-lex-03: lex error on `1abc`"))
        .map(|t| t.kind)
        .collect();
    assert_eq!(
        tokens,
        vec![TokenKind::Number("1".to_string()), ident("abc")],
        "digit-first run splits as Number then Ident"
    );
}
