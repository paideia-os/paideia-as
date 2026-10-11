//! UNI-14 (paideia-as#1601) — libFuzzer target for the NFC round-trip.
//!
//! Drives `nfc_normalize` + `is_nfc` with arbitrary valid UTF-8 (the
//! libFuzzer `|s: String|` form constructs well-formed strings via
//! `Arbitrary`, so this target does NOT re-exercise the decoder — the
//! sibling `utf8_decoder` target already covers malformed input).
//!
//! Invariants asserted:
//!
//! 1. `is_nfc(nfc_normalize(s))` — the normalizer's output is in NFC.
//! 2. `nfc_normalize(nfc_normalize(s)) == nfc_normalize(s)` — NFC is
//!    idempotent (UAX#15 §1.4). The paideia-as `Str::eq` contract at
//!    R220.M4 depends on this: two values that compare equal after one
//!    normalization must still compare equal after any number of
//!    further normalizations.
//! 3. `nfc_normalize(s)` never panics on any valid UTF-8 input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paideia_as_unicode::{is_nfc, nfc_normalize};

fuzz_target!(|s: String| {
    let normalized = nfc_normalize(&s);
    assert!(
        is_nfc(&normalized),
        "nfc_normalize output rejected by is_nfc",
    );
    let twice = nfc_normalize(&normalized);
    assert_eq!(
        normalized, twice,
        "nfc_normalize is not idempotent on the normalized form",
    );
});
