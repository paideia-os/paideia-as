//! UNI-13 (paideia-as#1600) — regression suite for the LSP
//! position-encoding negotiation at the paideia-lsp Backend boundary.
//!
//! `position_to_offset` is the choke point every `textDocument/*`
//! handler funnels through (hover, completion, goto-definition,
//! references, range-formatting, inlay hints), so a UTF-16 off-by-one
//! in a multi-byte buffer silently misreports every one of those
//! services. These tests pin the three structural cases UNI-13's
//! `utf16_offset_to_utf8` converter covers at the LSP boundary:
//!
//!   * ASCII — the historical Phase-2-m8-003 contract; UTF-16 columns
//!     and byte offsets agree.
//!   * BMP non-ASCII — a position *after* a 2-byte UTF-8 character
//!     reports a UTF-16 column that is 1, not 2.
//!   * Astral — a position *after* a 4-byte UTF-8 character reports a
//!     UTF-16 column that is 2 (surrogate pair), not 1 or 4.
//!
//! The LSP wire shape ships `Position::character` in UTF-16 code
//! units by default (LSP 3.17 §3.17). The tests exercise the
//! inverse direction (UTF-16 column → UTF-8 byte offset in the
//! document), which is the direction every server handler needs.

use paideia_lsp::document::position_to_offset;
use tower_lsp::lsp_types::Position;

#[test]
fn uni13_lsp_pos_01_ascii_single_line() {
    let text = "hello";
    assert_eq!(
        position_to_offset(
            text,
            Position {
                line: 0,
                character: 3,
            },
        ),
        3,
        "uni13-lsp-pos-01",
    );
}

#[test]
fn uni13_lsp_pos_02_bmp_non_ascii_eacute() {
    // "héllo" — "é" is 2 UTF-8 bytes but 1 UTF-16 unit.
    // A client that sends UTF-16 character=2 means "just past é" —
    // which in bytes is offset 3.
    let text = "héllo";
    assert_eq!(
        position_to_offset(
            text,
            Position {
                line: 0,
                character: 2,
            },
        ),
        3,
        "uni13-lsp-pos-02",
    );
}

#[test]
fn uni13_lsp_pos_03_astral_emoji_surrogate_pair() {
    // 😀 — U+1F600 is 4 UTF-8 bytes, 2 UTF-16 units. A client that
    // sends character=2 means "just past the emoji" (byte 4). A
    // client that sends character=1 means "inside the surrogate
    // pair" and rounds DOWN to byte 0 per LSP 3.17 §3.14.
    let text = "😀";
    assert_eq!(
        position_to_offset(
            text,
            Position {
                line: 0,
                character: 2,
            },
        ),
        4,
        "uni13-lsp-pos-03:after",
    );
    assert_eq!(
        position_to_offset(
            text,
            Position {
                line: 0,
                character: 1,
            },
        ),
        0,
        "uni13-lsp-pos-03:mid",
    );
}

#[test]
fn uni13_lsp_pos_04_multi_line_bmp() {
    // "abc\nhéllo\nxyz" — line 1 is "héllo"; UTF-16 character=2
    // means "just past é" → byte offset 4+3 = 7.
    let text = "abc\nhéllo\nxyz";
    assert_eq!(
        position_to_offset(
            text,
            Position {
                line: 1,
                character: 2,
            },
        ),
        7,
        "uni13-lsp-pos-04",
    );
}

#[test]
fn uni13_lsp_pos_05_character_past_line_end_clamps() {
    // Line "ab" has UTF-16 length 2; character=5 clamps to the line
    // end (byte 2), not into the next line. Pre-UNI-13 the inner
    // loop walked past the '\n' and reported offset 3 (into "cd");
    // UNI-13 trims at the newline.
    let text = "ab\ncd";
    assert_eq!(
        position_to_offset(
            text,
            Position {
                line: 0,
                character: 5,
            },
        ),
        2,
        "uni13-lsp-pos-05",
    );
}
