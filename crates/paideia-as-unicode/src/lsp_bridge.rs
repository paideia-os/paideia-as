//! UNI-13 (paideia-as#1600) — LSP position-encoding bridge.
//!
//! The Language Server Protocol encodes a `Position::character` field
//! as a count of code units in some *client-negotiated* encoding
//! (LSP 3.17 §3.17 "Position Encoding Kind"):
//!
//! * `utf-8`  — count of UTF-8 code units (bytes).
//! * `utf-16` — count of UTF-16 code units (the LSP default; VSCode's
//!   wire shape).
//! * `utf-32` — count of Unicode scalar values (one per Rust `char`).
//!
//! Rust's native `&str` indexing speaks only UTF-8 byte offsets, so the
//! LSP server has to convert at every edge. This module owns the five
//! converters paideia-lsp needs:
//!
//! * [`utf8_offset_to_utf16`] / [`utf16_offset_to_utf8`]
//! * [`utf8_offset_to_utf32`] / [`utf32_offset_to_utf8`]
//! * [`stream_is_nfc`] — the streaming NFC predicate (companion to
//!   [`crate::is_nfc`]) that accepts any `Iterator<Item = char>`
//!   rather than an owned `&str`. The server uses it to validate
//!   `textDocument/didChange` edits as they are decoded, before an
//!   owning `String` is formed.
//!
//! # Mid-char rounding
//!
//! All four byte/code-unit converters are *total*: a UTF-8 offset that
//! falls strictly inside a multi-byte sequence (or a UTF-16 offset that
//! falls between the high and low surrogate of an astral pair) rounds
//! DOWN to the preceding char boundary. This matches LSP 3.17 §3.14's
//! "position between characters" convention — a position pointing into
//! the middle of a code unit sequence is interpreted as the position
//! immediately before that sequence.
//!
//! The round-down discipline keeps the two symmetric converters
//! idempotent on well-formed boundaries:
//!
//! ```text
//! utf16_offset_to_utf8(s, utf8_offset_to_utf16(s, i)) == i
//! ```
//!
//! for every `i` that is itself a UTF-8 char boundary. For a mid-char
//! `i`, the composition instead returns the preceding char boundary —
//! intentionally lossy, but total.
//!
//! # ASCII fast path
//!
//! When `s` is pure 7-bit ASCII every encoding agrees on code-unit
//! counts, so each converter short-circuits to `offset.min(s.len())`
//! without consulting the Unicode tables. Guarded by `s.is_ascii()`
//! (shares the same fast-path discipline as [`crate::is_nfc`]).

use unicode_normalization::{IsNormalized, is_nfc_quick};

/// Convert a UTF-8 byte offset into `s` to the equivalent LSP UTF-16
/// code-unit offset.
///
/// The LSP 3.17 default position encoding is UTF-16 (`Position::character`
/// counts 16-bit code units). This converter is the server-side bridge
/// from Rust's native UTF-8 indexing to that wire format.
///
/// Mid-char offsets round DOWN to the preceding char boundary per the
/// module-level convention. Offsets beyond `s.len()` saturate to
/// `s.encode_utf16().count()` (the full UTF-16 length).
///
/// ASCII fast path: when `s` is pure 7-bit ASCII the two encodings
/// agree, so the function returns `utf8_offset.min(s.len())` without
/// scanning the string.
pub fn utf8_offset_to_utf16(s: &str, utf8_offset: usize) -> usize {
    if s.is_ascii() {
        return utf8_offset.min(s.len());
    }
    // Walk char boundaries, accumulating UTF-16 code units until the
    // byte cursor would exceed `utf8_offset`. Rounding down is
    // automatic: a mid-char `utf8_offset` lands strictly between two
    // boundaries, so the loop stops at the preceding one.
    let mut utf16_units = 0usize;
    for (i, ch) in s.char_indices() {
        if i + ch.len_utf8() > utf8_offset {
            return utf16_units;
        }
        utf16_units += ch.len_utf16();
    }
    utf16_units
}

/// Convert an LSP UTF-16 code-unit offset into `s` to the equivalent
/// UTF-8 byte offset.
///
/// The inverse of [`utf8_offset_to_utf16`]. Mid-surrogate offsets (a
/// `utf16_offset` that lands between the high and low surrogate of an
/// astral pair) round DOWN to the preceding char boundary. Offsets
/// beyond the UTF-16 length saturate to `s.len()`.
///
/// ASCII fast path: when `s` is pure 7-bit ASCII the two encodings
/// agree, so the function returns `utf16_offset.min(s.len())`.
pub fn utf16_offset_to_utf8(s: &str, utf16_offset: usize) -> usize {
    if s.is_ascii() {
        return utf16_offset.min(s.len());
    }
    let mut utf16_units = 0usize;
    for (i, ch) in s.char_indices() {
        if utf16_units >= utf16_offset {
            return i;
        }
        let next = utf16_units + ch.len_utf16();
        if next > utf16_offset {
            // The requested offset falls strictly inside this char's
            // surrogate pair — round DOWN to the preceding char
            // boundary (`i`), the LSP 3.17 §3.14 convention.
            return i;
        }
        utf16_units = next;
    }
    s.len()
}

/// Convert a UTF-8 byte offset into `s` to the equivalent UTF-32
/// (codepoint) offset.
///
/// Semantically a count of `char`s preceding the byte offset. Clients
/// that negotiate `utf-32` as the position encoding (uncommon but
/// permitted by LSP 3.17) consume this converter.
///
/// Mid-char offsets round DOWN. Offsets beyond `s.len()` saturate to
/// `s.chars().count()`.
///
/// ASCII fast path: identical to the UTF-16 case.
pub fn utf8_offset_to_utf32(s: &str, utf8_offset: usize) -> usize {
    if s.is_ascii() {
        return utf8_offset.min(s.len());
    }
    let mut count = 0usize;
    for (i, ch) in s.char_indices() {
        if i + ch.len_utf8() > utf8_offset {
            return count;
        }
        count += 1;
    }
    count
}

/// Convert a UTF-32 (codepoint) offset into `s` to the equivalent
/// UTF-8 byte offset.
///
/// The inverse of [`utf8_offset_to_utf32`]. Offsets beyond
/// `s.chars().count()` saturate to `s.len()`.
///
/// ASCII fast path: identical to the UTF-16 case.
pub fn utf32_offset_to_utf8(s: &str, utf32_offset: usize) -> usize {
    if s.is_ascii() {
        return utf32_offset.min(s.len());
    }
    let mut count = 0usize;
    for (i, _ch) in s.char_indices() {
        if count >= utf32_offset {
            return i;
        }
        count += 1;
    }
    s.len()
}

/// Streaming NFC predicate: whether the character sequence produced by
/// `iter` is already in Unicode Normalization Form C.
///
/// Mirrors [`crate::is_nfc`] in semantics but accepts any
/// `Iterator<Item = char>` rather than requiring an owned `&str`. The
/// LSP server consumes this on freshly decoded `didChange` edits so it
/// can diagnose non-NFC inputs *during* decode (R221 SH-D9.4 — every
/// string crossing an IPC boundary is NFC-normalized at the parse
/// boundary).
///
/// Semantic parity with the `&str` form:
///
/// ```ignore
/// stream_is_nfc(s.chars()) == is_nfc(s)
/// ```
///
/// for every `s: &str`. An empty iterator yields `true` (vacuously NFC;
/// matches `is_nfc("")`).
///
/// `IsNormalized::Maybe` is treated conservatively as "not yet known"
/// (not NFC), matching [`crate::is_nfc`]'s policy.
pub fn stream_is_nfc<I: Iterator<Item = char>>(iter: I) -> bool {
    matches!(is_nfc_quick(iter), IsNormalized::Yes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_round_trip_hello() {
        assert_eq!(utf8_offset_to_utf16("hello", 3), 3);
        assert_eq!(utf16_offset_to_utf8("hello", 3), 3);
        assert_eq!(utf8_offset_to_utf32("hello", 3), 3);
        assert_eq!(utf32_offset_to_utf8("hello", 3), 3);
    }

    #[test]
    fn bmp_non_ascii_e_acute() {
        // é is 2 UTF-8 bytes but 1 UTF-16 unit and 1 codepoint.
        assert_eq!(utf8_offset_to_utf16("héllo", 3), 2);
        assert_eq!(utf8_offset_to_utf32("héllo", 3), 2);
    }

    #[test]
    fn astral_emoji_surrogate_pair() {
        // 😀 is U+1F600 — 4 UTF-8 bytes, 2 UTF-16 units (surrogate
        // pair), 1 codepoint.
        assert_eq!(utf8_offset_to_utf16("😀", 4), 2);
        assert_eq!(utf8_offset_to_utf32("😀", 4), 1);
        assert_eq!(utf16_offset_to_utf8("😀", 2), 4);
        assert_eq!(utf32_offset_to_utf8("😀", 1), 4);
    }

    #[test]
    fn stream_is_nfc_matches_is_nfc_for_strings() {
        for s in ["", "hello", "héllo", "cafe\u{0301}", "\u{212B}", "😀"] {
            assert_eq!(stream_is_nfc(s.chars()), crate::is_nfc(s), "{s:?}");
        }
    }
}
