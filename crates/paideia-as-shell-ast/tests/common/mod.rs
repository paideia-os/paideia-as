//! Shared test helpers for the R221.M5/M6/M7 test corpus.

#![allow(dead_code)]

use paideia_as_shell_ast::{SyntaxNode, parse, pretty_print};

/// Parse `src` and assert success, returning the AST.
pub fn parse_ok(fingerprint: &str, src: &str) -> SyntaxNode {
    parse(src).unwrap_or_else(|e| {
        panic!("{fingerprint}: parse failed on {src:?}: {e:?}")
    })
}

/// Parse `src` and assert failure with the given fingerprint context.
pub fn parse_err(fingerprint: &str, src: &str) {
    if let Ok(node) = parse(src) {
        panic!("{fingerprint}: expected parse error on {src:?}, got {node:?}");
    }
}

/// Round-trip: parse → pretty → parse; the second parse must succeed
/// and pretty-print to byte-identical output.
pub fn assert_roundtrip(fingerprint: &str, src: &str) {
    let a = parse_ok(fingerprint, src);
    let pp1 = pretty_print(&a);
    let b = parse(&pp1).unwrap_or_else(|e| {
        panic!(
            "{fingerprint}: re-parse of pretty output failed. \
             src: {src:?} pp: {pp1:?} err: {e:?}"
        )
    });
    let pp2 = pretty_print(&b);
    if pp1 != pp2 {
        panic!(
            "{fingerprint}: pretty-print not idempotent. \
             src: {src:?}\n  pp1: {pp1:?}\n  pp2: {pp2:?}"
        );
    }
}
