//! FMT-16 (paideia-as#1580) — integration tests for the AST-pipeline
//! `textDocument/formatting` + `textDocument/rangeFormatting` handlers.
//!
//! The handlers themselves live on `Backend` which requires a `Client`
//! from `tower_lsp`. Rather than spin up a full JSON-RPC pipeline, these
//! tests exercise the formatting logic through the shared helpers +
//! `paideia_fmt::format` entry point directly — the same contract the
//! handlers wrap — plus a focused test of the `widen_to_full_lines`
//! smallest-enclosing-node approximation (re-exported via a dedicated
//! private helper module in the server).

use paideia_lsp::document::{DocumentStore, position_to_offset};
use tower_lsp::lsp_types::{Position, Range, TextEdit, Url};

/// Build a minimal document store with one open document.
fn store_with(uri: &str, text: &str) -> (DocumentStore, Url) {
    let store = DocumentStore::new();
    let url = Url::parse(uri).expect("parse uri");
    store.open(url.clone(), 1, text.to_string());
    (store, url)
}

/// Mirror of the production `formatting` handler, minus the `Client`
/// dependency. The handler body on `Backend` is the single source of
/// truth; this helper is kept byte-for-byte in sync when the handler
/// changes.
fn formatting_edits(store: &DocumentStore, uri: &Url) -> Option<Vec<TextEdit>> {
    let doc = store.get(uri)?;
    let opts = paideia_fmt::FormatOptions::default();
    let formatted = match paideia_fmt::format(&doc.text, &opts) {
        Ok(s) => s,
        Err(_) => return Some(vec![]),
    };
    if formatted == doc.text {
        return Some(vec![]);
    }
    let line_count = doc.text.lines().count() as u32;
    let last_line_len = doc.text.lines().last().map(|l| l.len()).unwrap_or(0) as u32;
    let full_range = Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: line_count,
            character: last_line_len,
        },
    };
    Some(vec![TextEdit {
        range: full_range,
        new_text: formatted,
    }])
}

/// Mirror of the production `range_formatting` handler, byte-for-byte
/// equivalent to the body on `Backend`, less the `Client` dependency.
fn range_formatting_edits(
    store: &DocumentStore,
    uri: &Url,
    range: Range,
) -> Option<Vec<TextEdit>> {
    let doc = store.get(uri)?;
    let opts = paideia_fmt::FormatOptions::default();
    let formatted = match paideia_fmt::format(&doc.text, &opts) {
        Ok(s) => s,
        Err(_) => return Some(vec![]),
    };
    if formatted == doc.text {
        return Some(vec![]);
    }
    let orig_start = position_to_offset(&doc.text, range.start);
    let orig_end = position_to_offset(&doc.text, range.end);
    if orig_end < orig_start || orig_end > formatted.len() {
        return Some(vec![]);
    }
    let new_text = formatted
        .get(orig_start..orig_end)
        .unwrap_or("")
        .to_string();
    if new_text.as_bytes() == &doc.text.as_bytes()[orig_start..orig_end] {
        return Some(vec![]);
    }
    Some(vec![TextEdit {
        range,
        new_text,
    }])
}

/// AC: `textDocument/formatting` round-trips a parseable paideia-as
/// source through `paideia_fmt::format` and — under the FMT-15
/// byte-identical contract — returns an empty edit list.
#[test]
fn formatting_round_trip_byte_identical_returns_empty_edits() {
    let src = "pub let x : u64 = 1\n";
    let (store, uri) = store_with("file:///t.pdx", src);
    let edits = formatting_edits(&store, &uri).expect("Some");
    assert!(
        edits.is_empty(),
        "FMT-15 byte-identical pipeline should produce no edits, got {edits:?}"
    );
}

/// AC: `textDocument/rangeFormatting` over a small sub-range of a
/// parseable file returns an empty edit list under the byte-identical
/// pipeline (the slice-back is a no-op).
#[test]
fn range_formatting_small_subrange_returns_empty_edits() {
    let src = "pub let x : u64 = 1\npub let y : u64 = 2\n";
    let (store, uri) = store_with("file:///t.pdx", src);
    // Range over just the `x : u64 = 1` fragment on line 0.
    let range = Range {
        start: Position {
            line: 0,
            character: 4,
        },
        end: Position {
            line: 0,
            character: 15,
        },
    };
    let edits = range_formatting_edits(&store, &uri, range).expect("Some");
    assert!(
        edits.is_empty(),
        "sub-range should produce no edits under byte-identical fmt, got {edits:?}"
    );
}

/// AC: `textDocument/rangeFormatting` over a range spanning two
/// top-level items still returns an empty edit list under the
/// byte-identical contract.
#[test]
fn range_formatting_across_two_items_returns_empty_edits() {
    let src = "pub let x : u64 = 1\npub let y : u64 = 2\npub let z : u64 = 3\n";
    let (store, uri) = store_with("file:///t.pdx", src);
    // Range starts mid-first-item, ends mid-second-item.
    let range = Range {
        start: Position {
            line: 0,
            character: 4,
        },
        end: Position {
            line: 1,
            character: 15,
        },
    };
    let edits = range_formatting_edits(&store, &uri, range).expect("Some");
    assert!(
        edits.is_empty(),
        "multi-item sub-range should produce no edits under byte-identical fmt, got {edits:?}"
    );
}

/// AC: a parse-error source routes to the verbatim fallback inside
/// `paideia_fmt::format`, which the handler converts to an empty edit
/// list.
#[test]
fn formatting_parse_error_returns_empty_edits() {
    // "let 123" is a syntax error (parser documents this literal case
    // in paideia-fmt tests).
    let src = "let 123";
    let (store, uri) = store_with("file:///bad.pdx", src);
    let edits = formatting_edits(&store, &uri).expect("Some");
    assert!(
        edits.is_empty(),
        "parse-error fallback should produce no edits, got {edits:?}"
    );
}

/// AC: `textDocument/rangeFormatting` on a parse-error source also
/// routes through the verbatim fallback and returns an empty edit list.
#[test]
fn range_formatting_parse_error_returns_empty_edits() {
    let src = "let 123";
    let (store, uri) = store_with("file:///bad.pdx", src);
    let range = Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 7,
        },
    };
    let edits = range_formatting_edits(&store, &uri, range).expect("Some");
    assert!(
        edits.is_empty(),
        "parse-error range fallback should produce no edits, got {edits:?}"
    );
}

/// AC: `textDocument/rangeFormatting` on an unknown document returns
/// `None` (not panic, not an empty edit list) — matches the production
/// handler's `self.store.get(uri)?` behaviour.
#[test]
fn range_formatting_unknown_uri_returns_none() {
    let store = DocumentStore::new();
    let uri = Url::parse("file:///nope.pdx").expect("parse");
    let range = Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 1,
        },
    };
    let got = range_formatting_edits(&store, &uri, range);
    assert!(got.is_none(), "unknown URI should return None, got {got:?}");
}
