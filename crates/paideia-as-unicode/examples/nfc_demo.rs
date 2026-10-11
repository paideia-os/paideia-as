//! `nfc_demo` — round-trip a few canonical NFC cases.
//!
//! Shows the three contracts R220.M4 `Str::eq` leans on:
//!   1. precomposed input is already NFC and `is_nfc` returns true;
//!   2. decomposed input (base + combining mark) is NOT NFC, and
//!      `nfc_normalize` produces the precomposed form byte-for-byte;
//!   3. normalization is idempotent (UAX#15 §1.4).
//!
//! Run with: `cargo run --example nfc_demo -p paideia-as-unicode`.

use paideia_as_unicode::{is_nfc, nfc_normalize};

fn main() {
    // U+00E9 LATIN SMALL LETTER E WITH ACUTE (precomposed, 2 bytes UTF-8).
    let precomposed = "café";
    // U+0065 LATIN SMALL LETTER E + U+0301 COMBINING ACUTE ACCENT.
    let decomposed = "cafe\u{0301}";

    println!("precomposed = {:?}  ({} bytes)", precomposed, precomposed.len());
    println!("decomposed  = {:?}  ({} bytes)", decomposed, decomposed.len());

    assert!(is_nfc(precomposed), "precomposed string should report NFC");
    assert!(!is_nfc(decomposed), "decomposed string should NOT report NFC");

    let normalized = nfc_normalize(decomposed);
    println!("normalize(decomposed) = {:?}  ({} bytes)", normalized, normalized.len());

    assert_eq!(normalized, precomposed, "NFC must collapse to the precomposed form");
    assert!(is_nfc(&normalized));

    // Idempotence: nfc_normalize(nfc_normalize(x)) == nfc_normalize(x).
    let twice = nfc_normalize(&normalized);
    assert_eq!(twice, normalized, "NFC must be idempotent");

    // ASCII fast path: all-ASCII strings are trivially NFC.
    let ascii = "paideia-as";
    assert!(is_nfc(ascii));
    assert_eq!(nfc_normalize(ascii), ascii);

    println!("\nall NFC round-trip assertions passed.");
}
