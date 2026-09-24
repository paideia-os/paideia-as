//! R221.M1 + R221.M2 property tests.
//!
//! Three global invariants (see `crates/paideia-as-unicode/src/lib.rs`
//! module-doc "Invariants") checked over randomly generated strings:
//!
//!  1. **NFC idempotence** — `NFC(NFC(x)) == NFC(x)` (UAX#15 §1.4).
//!  2. **Grapheme ≤ char ≤ byte count** — a physical impossibility on
//!     a UAX#29-conforming iterator; the property test catches
//!     regressions if we ever swap the underlying crate.
//!  3. **UTF-8 decoder round-trip** — for any well-formed `&str`,
//!     `Utf8Decoder(s.as_bytes()).collect() == Ok(s)`.
//!
//! Fingerprint discipline: `r221m1-nfc-prop` and `r221m2-tr29-prop`.

use paideia_as_unicode::{
    Utf8Decoder, grapheme_count, nfc_normalize,
};
use proptest::prelude::*;

/// Reasonable string generator: any well-formed Rust `String` up to
/// 128 code points. Covers ASCII, Latin-1, CJK, and the astral planes
/// (proptest's default `.*` regex generator).
fn any_string() -> impl Strategy<Value = String> {
    ".{0,128}".prop_map(|s| s)
}

proptest! {
    #[test]
    fn r221m1_nfc_prop_idempotent(input in any_string()) {
        let once = nfc_normalize(&input);
        let twice = nfc_normalize(&once);
        prop_assert_eq!(twice, once, "r221m1-nfc-prop: NFC not idempotent");
    }

    #[test]
    fn r221m2_tr29_prop_grapheme_le_char_le_byte(input in any_string()) {
        let g = grapheme_count(&input);
        let c = input.chars().count();
        let b = input.len();
        prop_assert!(g <= c, "r221m2-tr29-prop: grapheme_count > char_count");
        prop_assert!(c <= b, "r221m2-tr29-prop: char_count > byte_count");
    }

    #[test]
    fn r221m1_utf8_prop_valid_roundtrip(input in any_string()) {
        let decoded: Result<String, _> =
            Utf8Decoder::new(input.as_bytes()).collect();
        prop_assert_eq!(decoded.unwrap(), input, "r221m1-utf8-prop");
    }
}
