//! UNI-2 (paideia-as#1589) — case-folding + Unicode casing conformance.
//!
//! Fixtures distilled from UCD 15.1 `CaseFolding.txt` and
//! `SpecialCasing.txt`. Each test carries a `uni2-case-NN` fingerprint
//! so the debugger can correlate test binaries against the UNI-2
//! acceptance line item per `feedback_workerbee_verify_claims.md`.
//!
//! The ASCII fast path is a byte-local lowercase (`c | 0x20`) that
//! never touches the UCD tables — see `src/case.rs` module header for
//! the invariant. We do not benchmark it here (UNI-8 owns the bench
//! surface); the fast path is covered by the pure-ASCII fixtures
//! below.

use paideia_as_unicode::{
    CaseFoldLocale, eq_ignore_case_unicode, fold, fold_default, lowercase, uppercase,
};

// ---------------------------------------------------------------------
// 1. Known-value fold fixtures (CaseFolding.txt rows)
// ---------------------------------------------------------------------

#[test]
fn uni2_case_01_fold_default_ascii_hello() {
    // ASCII fast path: `c | 0x20` for A..=Z. Must bypass the UCD
    // tables entirely — correctness is byte-local.
    assert_eq!(fold("Hello", CaseFoldLocale::Default), "hello");
    assert_eq!(fold_default("Hello"), "hello");
}

#[test]
fn uni2_case_02_fold_default_sharp_s_expands() {
    // CaseFolding.txt row:
    //   00DF; F; 0073 0073; # LATIN SMALL LETTER SHARP S
    // German `ß` folds to the two-codepoint `ss`.
    assert_eq!(fold("ß", CaseFoldLocale::Default), "ss");
    assert_eq!(fold("straße", CaseFoldLocale::Default), "strasse");
}

#[test]
fn uni2_case_03_fold_default_dotted_capital_i() {
    // CaseFolding.txt row:
    //   0130; F; 0069 0307; # LATIN CAPITAL LETTER I WITH DOT ABOVE
    // Default + Full fold emits `i` + U+0307 COMBINING DOT ABOVE.
    let folded = fold("İ", CaseFoldLocale::Default);
    assert_eq!(folded, "i\u{0307}");
    // The fold output must be independently valid UTF-8.
    assert!(folded.is_char_boundary(0));
    assert!(folded.is_char_boundary(folded.len()));
    assert_eq!(folded.chars().count(), 2);
}

#[test]
fn uni2_case_04_fold_turkic_capital_i_to_dotless() {
    // CaseFolding.txt row:
    //   0049; T; 0131; # LATIN CAPITAL LETTER I
    // Turkic locale pre-empts the default `I -> i` with `I -> ı`.
    assert_eq!(fold("I", CaseFoldLocale::Turkic), "\u{0131}");
    assert_eq!(fold("I", CaseFoldLocale::Turkic), "ı");
}

#[test]
fn uni2_case_05_fold_turkic_dotted_capital_i_to_i() {
    // CaseFolding.txt row:
    //   0130; T; 0069; # LATIN CAPITAL LETTER I WITH DOT ABOVE
    // Turkic locale emits plain `i` (no trailing combining dot).
    assert_eq!(fold("İ", CaseFoldLocale::Turkic), "i");
}

#[test]
fn uni2_case_06_fold_turkic_mixed_still_default_elsewhere() {
    // In Turkic locale, only I/İ are redirected; every other
    // codepoint uses the default C+F fold. `ß` must still expand to
    // `ss`, and the dotted-I case must still collapse to plain `i`.
    assert_eq!(fold("İstanbul", CaseFoldLocale::Turkic), "istanbul");
    // Uppercase S is not I, so it folds via the default C row to `s`;
    // ß expands via the default F row to `ss`.
    assert_eq!(fold("Straße", CaseFoldLocale::Turkic), "strasse");
}

// ---------------------------------------------------------------------
// 2. Known-value Unicode uppercase / lowercase fixtures
// ---------------------------------------------------------------------

#[test]
fn uni2_case_07_uppercase_sharp_s_expands_to_ss() {
    // SpecialCasing.txt row:
    //   00DF; 00DF; 0053 0053; 0053 0053; # LATIN SMALL LETTER SHARP S
    // Full uppercase (not just ASCII) must emit `SS`.
    assert_eq!(uppercase("straße"), "STRASSE");
    assert_eq!(uppercase("ß"), "SS");
}

#[test]
fn uni2_case_08_lowercase_basic() {
    assert_eq!(lowercase("HELLO"), "hello");
    assert_eq!(lowercase("STRASSE"), "strasse");
}

#[test]
fn uni2_case_09_uppercase_preserves_already_upper() {
    // Idempotence-like property for uppercase (noting ß ↔ SS is
    // genuinely asymmetric: `SS` lowercases to `ss`, not `ß`).
    assert_eq!(uppercase("HELLO"), "HELLO");
}

// ---------------------------------------------------------------------
// 3. Equality (eq_ignore_case_unicode) fixtures
// ---------------------------------------------------------------------

#[test]
fn uni2_case_10_eq_default_hello_hello() {
    assert!(eq_ignore_case_unicode(
        "Hello",
        "HELLO",
        CaseFoldLocale::Default
    ));
    assert!(eq_ignore_case_unicode(
        "HELLO",
        "hello",
        CaseFoldLocale::Default
    ));
}

#[test]
fn uni2_case_11_eq_turkic_capital_i_matches_dotless() {
    // Under Turkic: `I` folds to `ı`, so the two match.
    assert!(eq_ignore_case_unicode(
        "I",
        "\u{0131}",
        CaseFoldLocale::Turkic
    ));
    assert!(eq_ignore_case_unicode("I", "ı", CaseFoldLocale::Turkic));
}

#[test]
fn uni2_case_12_eq_default_capital_i_mismatches_dotless() {
    // Under Default: `I` folds to `i` (U+0069), not `ı` (U+0131).
    assert!(!eq_ignore_case_unicode(
        "I",
        "\u{0131}",
        CaseFoldLocale::Default
    ));
    assert!(!eq_ignore_case_unicode("I", "ı", CaseFoldLocale::Default));
}

#[test]
fn uni2_case_13_eq_default_ascii_fast_path() {
    // Pure-ASCII inputs under Default take the
    // `str::eq_ignore_ascii_case` branch (no allocation). We cannot
    // observe that directly from a test, but we can verify the
    // outcome matches the slow path.
    assert!(eq_ignore_case_unicode(
        "find",
        "FIND",
        CaseFoldLocale::Default
    ));
    assert!(eq_ignore_case_unicode(
        "cd ..",
        "CD ..",
        CaseFoldLocale::Default
    ));
    assert!(!eq_ignore_case_unicode(
        "find",
        "grep",
        CaseFoldLocale::Default
    ));
}

#[test]
fn uni2_case_14_eq_default_sharp_s_matches_ss() {
    // Classic Unicode case equality: ß ≡ ss ≡ SS under default fold.
    assert!(eq_ignore_case_unicode(
        "straße",
        "strasse",
        CaseFoldLocale::Default
    ));
    assert!(eq_ignore_case_unicode(
        "STRASSE",
        "straße",
        CaseFoldLocale::Default
    ));
}

#[test]
fn uni2_case_15_eq_empty_strings() {
    assert!(eq_ignore_case_unicode("", "", CaseFoldLocale::Default));
    assert!(eq_ignore_case_unicode("", "", CaseFoldLocale::Turkic));
    assert!(!eq_ignore_case_unicode(
        "",
        "x",
        CaseFoldLocale::Default
    ));
}

// ---------------------------------------------------------------------
// 4. Idempotence: fold(fold(x, l), l) == fold(x, l)
// ---------------------------------------------------------------------

/// Corpus of 20 strings exercising ASCII, Latin-1 supplement,
/// combining-mark sequences, Turkic-sensitive codepoints, Hangul,
/// astral-plane emoji, and defective combining sequences.
const IDEMPOTENCE_CORPUS: &[&str] = &[
    "",
    "a",
    "Hello",
    "HELLO WORLD",
    "ß",
    "straße",
    "İstanbul",
    "I",
    "\u{0131}",
    "\u{0130}",
    "café",
    "cafe\u{0301}",
    "ñ",
    "Σ",
    "σ",
    "ς",
    "한국어",
    "🙂",
    "\u{1F600}\u{1F601}",
    "mixed-ASCII+Σ+🙂",
];

#[test]
fn uni2_case_16_idempotence_default_locale() {
    assert_eq!(IDEMPOTENCE_CORPUS.len(), 20);
    for &s in IDEMPOTENCE_CORPUS {
        let once = fold(s, CaseFoldLocale::Default);
        let twice = fold(&once, CaseFoldLocale::Default);
        assert_eq!(
            once, twice,
            "default-locale fold not idempotent for {:?}",
            s
        );
    }
}

#[test]
fn uni2_case_17_idempotence_turkic_locale() {
    for &s in IDEMPOTENCE_CORPUS {
        let once = fold(s, CaseFoldLocale::Turkic);
        let twice = fold(&once, CaseFoldLocale::Turkic);
        assert_eq!(once, twice, "turkic fold not idempotent for {:?}", s);
    }
}

// ---------------------------------------------------------------------
// 5. Round-trip: fold(uppercase(x), Default) == fold(x, Default)
// ---------------------------------------------------------------------

/// 15 strings for the fold-uppercase round-trip. We avoid pure-Turkic
/// codepoints here because `uppercase` is not locale-sensitive and
/// `uppercase("ı")` emits `I`, which the default fold then rolls to
/// `i`, matching the default fold of the lowercase dotless-i only
/// after NFC. We stick to inputs where the default round-trip is
/// unambiguous.
const ROUNDTRIP_CORPUS: &[&str] = &[
    "",
    "a",
    "A",
    "Hello",
    "hello",
    "HELLO",
    "straße",
    "STRASSE",
    "café",
    "CAFÉ",
    "ñoño",
    "ÑOÑO",
    "Σοφία",
    "ΣΟΦΊΑ",
    "mixed CASE 123",
];

#[test]
fn uni2_case_18_roundtrip_fold_uppercase() {
    assert_eq!(ROUNDTRIP_CORPUS.len(), 15);
    for &s in ROUNDTRIP_CORPUS {
        let via_upper = fold(&uppercase(s), CaseFoldLocale::Default);
        let direct = fold(s, CaseFoldLocale::Default);
        assert_eq!(
            via_upper, direct,
            "fold(uppercase({:?})) != fold({:?})",
            s, s
        );
    }
}

// ---------------------------------------------------------------------
// 6. CaseFoldLocale type-level sanity
// ---------------------------------------------------------------------

#[test]
fn uni2_case_19_default_locale_derives_default_variant() {
    // `#[derive(Default)]` with `#[default]` on `Default`.
    assert_eq!(CaseFoldLocale::default(), CaseFoldLocale::Default);
}

#[test]
fn uni2_case_20_fold_default_matches_locale_default() {
    // `fold_default` ≡ `fold(_, CaseFoldLocale::Default)`.
    for &s in IDEMPOTENCE_CORPUS {
        assert_eq!(fold_default(s), fold(s, CaseFoldLocale::Default));
    }
}
