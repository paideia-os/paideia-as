//! R221.M1 UTF-8 decoder corpus.
//!
//! 15 vectors covering:
//!  * Legal 1/2/3/4-byte sequences per RFC 3629 §3;
//!  * Overlong encodings (must be rejected);
//!  * Surrogates U+D800..=U+DFFF as bytes (must be rejected);
//!  * Code points above U+10FFFF (must be rejected);
//!  * Truncated tails;
//!  * Bare continuation bytes;
//!  * The specific "byte offset" reported by `Utf8Error` — hosted DSLs
//!    downstream feed that offset into `paideia-as-diagnostics::Span`.
//!
//! Fingerprint discipline: `r221m1-utf8-NN` on every test.

use paideia_as_unicode::{Utf8Decoder, error::Utf8ErrorKind};

fn decode_all(bytes: &[u8]) -> Vec<Result<char, paideia_as_unicode::Utf8Error>> {
    Utf8Decoder::new(bytes).collect()
}

#[test]
fn r221m1_utf8_01_ascii_roundtrip() {
    let out: Result<String, _> = Utf8Decoder::new(b"hello").collect();
    assert_eq!(out.unwrap(), "hello", "r221m1-utf8-01");
}

#[test]
fn r221m1_utf8_02_two_byte_latin1() {
    // U+00E9 é = 0xC3 0xA9.
    let bytes = [0xC3, 0xA9];
    let out: Result<String, _> = Utf8Decoder::new(&bytes).collect();
    assert_eq!(out.unwrap(), "é", "r221m1-utf8-02");
}

#[test]
fn r221m1_utf8_03_three_byte_cjk() {
    // U+4E2D 中 = 0xE4 0xB8 0xAD.
    let bytes = [0xE4, 0xB8, 0xAD];
    let out: Result<String, _> = Utf8Decoder::new(&bytes).collect();
    assert_eq!(out.unwrap(), "中", "r221m1-utf8-03");
}

#[test]
fn r221m1_utf8_04_four_byte_astral() {
    // U+1F600 😀 = 0xF0 0x9F 0x98 0x80.
    let bytes = [0xF0, 0x9F, 0x98, 0x80];
    let out: Result<String, _> = Utf8Decoder::new(&bytes).collect();
    assert_eq!(out.unwrap(), "😀", "r221m1-utf8-04");
}

#[test]
fn r221m1_utf8_05_overlong_zero_rejected() {
    // 0xC0 0x80 encodes U+0000 in 2 bytes — overlong. RFC 3629 §3
    // requires rejection. The lead byte 0xC0 is not in the legal
    // 0xC2..=0xDF range, so we reject as InvalidLeadByte at offset 0.
    let errs = decode_all(&[0xC0, 0x80]);
    assert!(matches!(errs[0], Err(e) if e.offset == 0 && e.reason == Utf8ErrorKind::InvalidLeadByte),
        "r221m1-utf8-05: got {:?}", errs);
}

#[test]
fn r221m1_utf8_06_overlong_three_byte_rejected() {
    // 0xE0 0x80 0x80 would encode U+0000 in 3 bytes — overlong. The
    // second byte must be 0xA0..=0xBF for lead 0xE0. We reject the
    // continuation byte.
    let errs = decode_all(&[0xE0, 0x80, 0x80]);
    assert!(matches!(errs[0], Err(e) if e.reason == Utf8ErrorKind::InvalidContinuationByte),
        "r221m1-utf8-06: got {:?}", errs);
}

#[test]
fn r221m1_utf8_07_surrogate_rejected() {
    // U+D800 in 3-byte UTF-8 would be 0xED 0xA0 0x80. For lead 0xED,
    // second byte must be 0x80..=0x9F, so 0xA0 fails the continuation
    // check.
    let errs = decode_all(&[0xED, 0xA0, 0x80]);
    assert!(matches!(errs[0], Err(e) if e.reason == Utf8ErrorKind::InvalidContinuationByte),
        "r221m1-utf8-07: got {:?}", errs);
}

#[test]
fn r221m1_utf8_08_out_of_range_rejected() {
    // U+110000 in 4-byte UTF-8 would be 0xF4 0x90 0x80 0x80. For lead
    // 0xF4, second byte must be 0x80..=0x8F, so 0x90 fails.
    let errs = decode_all(&[0xF4, 0x90, 0x80, 0x80]);
    assert!(matches!(errs[0], Err(e) if e.reason == Utf8ErrorKind::InvalidContinuationByte),
        "r221m1-utf8-08: got {:?}", errs);
}

#[test]
fn r221m1_utf8_09_out_of_range_five_byte_lead_rejected() {
    // 0xF5 is never a legal UTF-8 lead byte (would encode U+140000+).
    let errs = decode_all(&[0xF5, 0x80, 0x80, 0x80]);
    assert!(matches!(errs[0], Err(e) if e.offset == 0 && e.reason == Utf8ErrorKind::InvalidLeadByte),
        "r221m1-utf8-09: got {:?}", errs);
}

#[test]
fn r221m1_utf8_10_truncated_two_byte() {
    // Lead byte says 2, only 1 byte remains.
    let errs = decode_all(&[0xC3]);
    assert!(matches!(errs[0], Err(e) if e.reason == Utf8ErrorKind::UnexpectedEndOfInput),
        "r221m1-utf8-10: got {:?}", errs);
}

#[test]
fn r221m1_utf8_11_truncated_four_byte() {
    // Lead byte says 4, only 2 bytes remain.
    let errs = decode_all(&[0xF0, 0x9F]);
    assert!(matches!(errs[0], Err(e) if e.reason == Utf8ErrorKind::UnexpectedEndOfInput),
        "r221m1-utf8-11: got {:?}", errs);
}

#[test]
fn r221m1_utf8_12_bare_continuation_rejected() {
    // 0x80 alone is a continuation byte with no preceding lead.
    let errs = decode_all(&[0x80]);
    assert!(matches!(errs[0], Err(e) if e.offset == 0 && e.reason == Utf8ErrorKind::InvalidLeadByte),
        "r221m1-utf8-12: got {:?}", errs);
}

#[test]
fn r221m1_utf8_13_valid_after_error_resynchronizes() {
    // A bad byte followed by valid input: the decoder must yield one
    // error, then continue decoding. This is the "recover past a
    // malformed byte" behavior R221.M4's lexer relies on.
    let bytes = [b'a', 0x80, b'b'];
    let out: Vec<_> = Utf8Decoder::new(&bytes).collect();
    assert_eq!(out.len(), 3, "r221m1-utf8-13: expected 3 items, got {:?}", out);
    assert_eq!(out[0], Ok('a'));
    assert!(out[1].is_err());
    assert_eq!(out[2], Ok('b'));
}

#[test]
fn r221m1_utf8_14_position_tracks_after_success() {
    let bytes = "aé中".as_bytes();
    let mut d = Utf8Decoder::new(bytes);
    assert_eq!(d.position(), 0);
    d.next();
    assert_eq!(d.position(), 1); // 'a' is 1 byte
    d.next();
    assert_eq!(d.position(), 3); // 'é' is 2 bytes → cumulative 3
    d.next();
    assert_eq!(d.position(), 6); // '中' is 3 bytes → cumulative 6
    assert!(d.is_exhausted(), "r221m1-utf8-14");
}

#[test]
fn r221m1_utf8_15_high_bit_lead_but_continuation_range() {
    // 0xBF: continuation byte range as a lead. Reject as bad lead.
    let errs = decode_all(&[0xBF]);
    assert!(matches!(errs[0], Err(e) if e.reason == Utf8ErrorKind::InvalidLeadByte),
        "r221m1-utf8-15: got {:?}", errs);
}
