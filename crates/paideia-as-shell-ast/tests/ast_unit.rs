//! R221.M5/M6 direct unit tests over the `SyntaxNode` and `NfcMap`
//! primitives — no parser in the loop, so a regression in AST-side
//! code fails a small, targeted test rather than a fixture-corpus one.
//!
//! Fingerprint `r221m5-unit-NN` / `r221m6-unit-NN`.

use paideia_as_shell_ast::{Context, NfcMap, NodeSpan, SyntaxNode};

#[test]
fn r221m5_unit_01_syntheticspan_zero() {
    let s = NodeSpan::synthetic(Context::Pipeline);
    assert_eq!(s.original, (0, 0));
    assert_eq!(s.nfc, (0, 0));
}

#[test]
fn r221m5_unit_02_span_union() {
    let a = NodeSpan::new((0, 3), (0, 3), Context::Pipeline);
    let b = NodeSpan::new((5, 8), (5, 8), Context::Pipeline);
    let u = a.union(b);
    assert_eq!(u.original, (0, 8));
    assert_eq!(u.nfc, (0, 8));
}

#[test]
fn r221m5_unit_03_ident_span_accessor() {
    let s = NodeSpan::new((2, 5), (2, 5), Context::Pipeline);
    let n = SyntaxNode::Ident {
        name: "foo".into(),
        span: s,
    };
    assert_eq!(n.span(), s);
}

#[test]
fn r221m5_unit_04_litint_span_accessor() {
    let s = NodeSpan::new((0, 2), (0, 2), Context::Pipeline);
    let n = SyntaxNode::LitInt { value: 42, span: s };
    assert_eq!(n.span(), s);
}

#[test]
fn r221m6_unit_01_identity_map_ascii() {
    let (out, map) = NfcMap::build("hello");
    assert_eq!(out, "hello");
    assert!(map.is_identity());
    assert_eq!(map.to_original((0, 5)), (0, 5));
    assert_eq!(map.to_original((1, 3)), (1, 3));
}

#[test]
fn r221m6_unit_02_empty_input() {
    let (out, map) = NfcMap::build("");
    assert_eq!(out, "");
    assert_eq!(map.original_len(), 0);
    assert_eq!(map.nfc_len(), 0);
    assert_eq!(map.to_original((0, 0)), (0, 0));
}

#[test]
fn r221m6_unit_03_non_ascii_map_widens() {
    // é as e + combining acute → NFC "é"
    let src = "e\u{0301}";
    let (out, map) = NfcMap::build(src);
    assert!(!map.is_identity());
    assert_eq!(out, "\u{00e9}"); // NFC composed form
    // Full range translates to the full original.
    let full = map.to_original((0, out.len()));
    assert_eq!(full, (0, src.len()));
}

#[test]
fn r221m6_unit_04_map_returns_bounded_range() {
    let (out, map) = NfcMap::build("café"); // already NFC (composed)
    let mid = out.len() / 2;
    let (o0, o1) = map.to_original((mid, mid));
    assert!(o0 <= "café".len());
    assert!(o1 <= "café".len());
}

#[test]
fn r221m6_unit_05_len_accessors_match() {
    let src = "abc";
    let (_, map) = NfcMap::build(src);
    assert_eq!(map.original_len(), src.len());
    assert_eq!(map.nfc_len(), src.len());
}
