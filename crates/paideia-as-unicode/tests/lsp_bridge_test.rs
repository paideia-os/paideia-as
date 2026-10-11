//! UNI-13 (paideia-as#1600) — LSP position-encoding bridge conformance.
//!
//! Each test is fingerprinted with a `uni13-lsp-NN` tag so a failing
//! binary can be correlated to the UNI-13 ticket line in the dispatch
//! map, matching the discipline established by `uni1-norm-NN` /
//! `r221m1-nfc-NN`.
//!
//! Covers:
//!   * ASCII round-trip (01, 02)
//!   * BMP non-ASCII (03, 04)
//!   * Astral surrogate-pair (05, 06, 07)
//!   * Inverse symmetry at char boundaries (08, 09)
//!   * UTF-32 codepoint count (10)
//!   * Boundary cases — offset 0, offset == s.len(), mid-char
//!     round-down (11, 12, 13)
//!   * Streaming NFC predicate — hello / cafe-combining / empty (14,
//!     15, 16, 17)
//!   * `stream_is_nfc(s.chars()) == is_nfc(s)` parity across the
//!     cross-section (18)

use paideia_as_unicode::{
    is_nfc, stream_is_nfc, utf16_offset_to_utf8, utf32_offset_to_utf8, utf8_offset_to_utf16,
    utf8_offset_to_utf32,
};

#[test]
fn uni13_lsp_01_ascii_utf8_to_utf16_hello() {
    assert_eq!(utf8_offset_to_utf16("hello", 3), 3, "uni13-lsp-01");
}

#[test]
fn uni13_lsp_02_ascii_round_trip_all_offsets() {
    let s = "hello";
    for i in 0..=s.len() {
        let u16 = utf8_offset_to_utf16(s, i);
        let back = utf16_offset_to_utf8(s, u16);
        assert_eq!(back, i, "uni13-lsp-02:{i}");
    }
}

#[test]
fn uni13_lsp_03_bmp_utf8_to_utf16_eacute() {
    // "héllo" — the "é" (U+00E9) sits at bytes 1..3 (2 UTF-8 bytes).
    // Byte offset 3 is just past "é"; in UTF-16 that is 2 code units
    // (h + é).
    assert_eq!(utf8_offset_to_utf16("héllo", 3), 2, "uni13-lsp-03");
    assert_eq!(utf8_offset_to_utf16("héllo", 1), 1, "uni13-lsp-03:pre");
}

#[test]
fn uni13_lsp_04_bmp_round_trip_eacute() {
    let s = "héllo";
    // Char boundaries in UTF-8: 0, 1, 3, 4, 5, 6.
    for &i in &[0usize, 1, 3, 4, 5, 6] {
        let u16 = utf8_offset_to_utf16(s, i);
        assert_eq!(utf16_offset_to_utf8(s, u16), i, "uni13-lsp-04:{i}");
    }
}

#[test]
fn uni13_lsp_05_astral_utf8_to_utf16_emoji() {
    // 😀 — U+1F600 is 4 UTF-8 bytes, 2 UTF-16 units (surrogate pair).
    assert_eq!(utf8_offset_to_utf16("😀", 4), 2, "uni13-lsp-05");
    assert_eq!(utf8_offset_to_utf16("😀", 0), 0, "uni13-lsp-05:pre");
}

#[test]
fn uni13_lsp_06_astral_inverse_emoji() {
    assert_eq!(utf16_offset_to_utf8("😀", 2), 4, "uni13-lsp-06");
    assert_eq!(utf16_offset_to_utf8("😀", 0), 0, "uni13-lsp-06:pre");
}

#[test]
fn uni13_lsp_07_astral_mid_surrogate_rounds_down() {
    // A UTF-16 offset of 1 falls inside the surrogate pair of 😀 —
    // LSP 3.17 §3.14 convention: round DOWN to the preceding char
    // boundary (byte offset 0).
    assert_eq!(utf16_offset_to_utf8("😀", 1), 0, "uni13-lsp-07");
}

#[test]
fn uni13_lsp_08_inverse_symmetry_mixed_bmp_astral() {
    // "a😀b" — char boundaries at UTF-8 bytes 0, 1, 5, 6.
    let s = "a😀b";
    for &i in &[0usize, 1, 5, 6] {
        let u16 = utf8_offset_to_utf16(s, i);
        assert_eq!(utf16_offset_to_utf8(s, u16), i, "uni13-lsp-08:{i}");
    }
}

#[test]
fn uni13_lsp_09_inverse_symmetry_utf32() {
    let s = "héllo😀";
    // Char boundaries in UTF-8: 0, 1, 3, 4, 5, 6, 10.
    for &i in &[0usize, 1, 3, 4, 5, 6, 10] {
        let u32 = utf8_offset_to_utf32(s, i);
        assert_eq!(utf32_offset_to_utf8(s, u32), i, "uni13-lsp-09:{i}");
    }
}

#[test]
fn uni13_lsp_10_utf32_count_eacute() {
    // "héllo" — byte offset 3 is just past "é"; 2 codepoints precede.
    assert_eq!(utf8_offset_to_utf32("héllo", 3), 2, "uni13-lsp-10");
    // The whole string is 5 codepoints.
    assert_eq!(utf8_offset_to_utf32("héllo", "héllo".len()), 5, "uni13-lsp-10:end");
}

#[test]
fn uni13_lsp_11_offset_zero_is_zero() {
    // Zero in → zero out across every converter and every input shape.
    assert_eq!(utf8_offset_to_utf16("", 0), 0, "uni13-lsp-11:ascii-empty");
    assert_eq!(utf8_offset_to_utf16("héllo", 0), 0, "uni13-lsp-11:bmp");
    assert_eq!(utf8_offset_to_utf16("😀", 0), 0, "uni13-lsp-11:astral");
    assert_eq!(utf16_offset_to_utf8("😀", 0), 0, "uni13-lsp-11:inv-astral");
    assert_eq!(utf8_offset_to_utf32("😀", 0), 0, "uni13-lsp-11:cp");
    assert_eq!(utf32_offset_to_utf8("😀", 0), 0, "uni13-lsp-11:inv-cp");
}

#[test]
fn uni13_lsp_12_offset_at_s_len_is_length() {
    let s = "héllo😀";
    // UTF-8 len 10; UTF-16 len 7 (emoji is a surrogate pair); UTF-32 len 6.
    assert_eq!(utf8_offset_to_utf16(s, s.len()), 7, "uni13-lsp-12:u16");
    assert_eq!(utf8_offset_to_utf32(s, s.len()), 6, "uni13-lsp-12:u32");
    assert_eq!(utf16_offset_to_utf8(s, 7), s.len(), "uni13-lsp-12:inv-u16");
    assert_eq!(utf32_offset_to_utf8(s, 6), s.len(), "uni13-lsp-12:inv-u32");
}

#[test]
fn uni13_lsp_13_mid_char_utf8_rounds_down() {
    // "héllo" — byte offset 2 sits mid-way through the 2-byte "é".
    // LSP 3.17 §3.14: round DOWN to the preceding char boundary
    // (byte 1 → UTF-16 unit 1).
    assert_eq!(utf8_offset_to_utf16("héllo", 2), 1, "uni13-lsp-13:u16");
    assert_eq!(utf8_offset_to_utf32("héllo", 2), 1, "uni13-lsp-13:u32");
}

#[test]
fn uni13_lsp_14_stream_is_nfc_hello_true() {
    assert!(stream_is_nfc("hello".chars()), "uni13-lsp-14");
}

#[test]
fn uni13_lsp_15_stream_is_nfc_decomposed_cafe_false() {
    // "cafe" + combining acute (U+0301) is NFD — the NFC form is
    // "café" with the precomposed U+00E9. Quick-check returns `No`.
    assert!(!stream_is_nfc("cafe\u{0301}".chars()), "uni13-lsp-15");
}

#[test]
fn uni13_lsp_16_stream_is_nfc_empty_true() {
    let empty: std::iter::Empty<char> = std::iter::empty();
    assert!(stream_is_nfc(empty), "uni13-lsp-16");
}

#[test]
fn uni13_lsp_17_stream_is_nfc_precomposed_cafe_true() {
    // "café" with precomposed U+00E9 is already NFC.
    assert!(stream_is_nfc("caf\u{00E9}".chars()), "uni13-lsp-17");
}

#[test]
fn uni13_lsp_18_stream_is_nfc_matches_is_nfc_parity() {
    // Semantic parity across the full cross-section of inputs the
    // module docs promise: `stream_is_nfc(s.chars()) == is_nfc(s)`.
    for s in [
        "",
        "hello",
        "héllo",
        "caf\u{00E9}",
        "cafe\u{0301}",
        "\u{212B}",      // ANGSTROM SIGN — quick-check returns Maybe.
        "A\u{030A}",     // A + combining ring — ditto.
        "😀",
        "ls 中文",
    ] {
        assert_eq!(stream_is_nfc(s.chars()), is_nfc(s), "uni13-lsp-18:{s:?}");
    }
}
