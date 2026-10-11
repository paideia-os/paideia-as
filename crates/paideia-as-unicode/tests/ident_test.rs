//! UNI-4 (paideia-as#1591) — UAX #31 identifier + UTS #39 profile
//! conformance.
//!
//! Each test is tagged with a fingerprint matching the pattern
//! `uni4-ident-<two-digit index>`; the full catalogue below covers:
//!
//! * `is_xid_start` / `is_xid_continue` — ASCII, non-ASCII letter,
//!   digit, connector, whitespace, middle-dot (XID vs ID difference).
//! * `is_identifier` — happy path, empty, leading digit, whitespace in
//!   the middle, Greek-only identifier, mixed-script (UAX #31 permits).
//! * `check_ident(Ascii)` — ASCII happy path, leading digit, non-ASCII
//!   first char, non-ASCII continue.
//! * `check_ident(Default)` — same as `is_identifier` but surfacing the
//!   structured error kind + offset.
//! * `check_ident(AllowedChars)` — Default pass through + the UTS #39
//!   §5 single-script restriction (`Σgma`, `日本hello` both fail with
//!   `MixedScript`).
//! * `mixed_script` — single-script ASCII, single-script Greek,
//!   mixed Greek+Latin, mixed CJK+Hiragana+Latin, empty input, Common /
//!   Inherited filtering.
//! * NFC-stability — composed form passes, decomposed form fails with
//!   `NotNfc` (paideia-as policy strictly stronger than UAX #31).
//! * `IdentProfile::default()` → `Default`.

use paideia_as_unicode::{
    IdentError, IdentErrorKind, IdentProfile, Script, check_ident, is_identifier, is_xid_continue,
    is_xid_start, mixed_script,
};

// ---------------------------------------------------------------------
// 1. `is_xid_start` — character-level UAX #31 start predicate.
// ---------------------------------------------------------------------

#[test]
fn uni4_ident_01_xid_start_ascii_letter() {
    assert!(is_xid_start('a'));
    assert!(is_xid_start('A'));
    assert!(is_xid_start('z'));
    assert!(is_xid_start('Z'));
}

#[test]
fn uni4_ident_02_xid_start_rejects_digit_and_connector() {
    // Digits are XID_Continue but NOT XID_Start.
    assert!(!is_xid_start('0'));
    assert!(!is_xid_start('5'));
    assert!(!is_xid_start('9'));
    // `_` is XID_Continue but not XID_Start per UAX #31 §2 Table 1.
    assert!(!is_xid_start('_'));
}

#[test]
fn uni4_ident_03_xid_start_non_ascii_letters() {
    // Greek lowercase alpha.
    assert!(is_xid_start('α'));
    // Greek uppercase sigma.
    assert!(is_xid_start('Σ'));
    // CJK ideograph.
    assert!(is_xid_start('日'));
    // Hiragana.
    assert!(is_xid_start('あ'));
}

#[test]
fn uni4_ident_04_xid_start_rejects_middle_dot_and_whitespace() {
    // U+00B7 MIDDLE DOT is in XID_Continue but not XID_Start
    // and from XID_Start.
    assert!(!is_xid_start('\u{00B7}'));
    // Whitespace.
    assert!(!is_xid_start(' '));
    assert!(!is_xid_start('\t'));
    // Operator.
    assert!(!is_xid_start('-'));
    assert!(!is_xid_start('+'));
}

// ---------------------------------------------------------------------
// 2. `is_xid_continue` — character-level UAX #31 continue predicate.
// ---------------------------------------------------------------------

#[test]
fn uni4_ident_05_xid_continue_digit_and_connector() {
    assert!(is_xid_continue('5'));
    assert!(is_xid_continue('0'));
    assert!(is_xid_continue('9'));
    // U+005F LOW LINE: XID_Continue.
    assert!(is_xid_continue('_'));
}

#[test]
fn uni4_ident_06_xid_continue_letter_and_combining_mark() {
    assert!(is_xid_continue('a'));
    // U+0301 COMBINING ACUTE ACCENT — a Mn mark that joins to a base.
    assert!(is_xid_continue('\u{0301}'));
}

#[test]
fn uni4_ident_07_xid_continue_rejects_whitespace_and_operators() {
    assert!(!is_xid_continue(' '));
    assert!(!is_xid_continue('\n'));
    assert!(!is_xid_continue('-'));
    assert!(!is_xid_continue('+'));
    assert!(!is_xid_continue('('));
    assert!(!is_xid_continue(')'));
}

// ---------------------------------------------------------------------
// 3. `is_identifier` — string-level predicate (UAX #31 + NFC-stable).
// ---------------------------------------------------------------------

#[test]
fn uni4_ident_08_is_identifier_happy_path() {
    assert!(is_identifier("hello"));
    assert!(is_identifier("hello_world"));
    assert!(is_identifier("x1"));
    assert!(is_identifier("CamelCase"));
    // `_` is XID_Continue only — valid in the middle, not at the start.
    assert!(is_identifier("x_"));
}

#[test]
fn uni4_ident_09_is_identifier_rejects_empty_and_leading_underscore_or_digit() {
    assert!(!is_identifier(""));
    assert!(!is_identifier("5foo"));
    assert!(!is_identifier("0"));
    // U+005F LOW LINE has XID_Start = No per UCD; strict UAX #31
    // identifiers do not admit a leading `_`. The ASCII profile (which
    // follows the Rust-style identifier convention) does allow it; see
    // uni4_ident_15.
    assert!(!is_identifier("_underscore"));
}

#[test]
fn uni4_ident_10_is_identifier_rejects_whitespace_and_operators() {
    assert!(!is_identifier("hello world"));
    assert!(!is_identifier("a-b"));
    assert!(!is_identifier("x+y"));
    assert!(!is_identifier("foo "));
    assert!(!is_identifier(" foo"));
}

#[test]
fn uni4_ident_11_is_identifier_non_ascii_single_script() {
    // Greek Sigma — single-codepoint identifier.
    assert!(is_identifier("Σ"));
    // Pure Greek.
    assert!(is_identifier("αβγ"));
    // Pure CJK.
    assert!(is_identifier("日本"));
}

#[test]
fn uni4_ident_12_is_identifier_uax31_permits_mixed_script() {
    // UAX #31 is script-blind: `Σgma` (Greek Sigma + Latin `gma`) is
    // syntactically a valid identifier. UTS #39 flags it; `is_identifier`
    // does not.
    assert!(is_identifier("Σgma"));
}

// ---------------------------------------------------------------------
// 4. `is_identifier` — NFC-stability (paideia-as extension to UAX #31).
// ---------------------------------------------------------------------

#[test]
fn uni4_ident_13_is_identifier_nfc_composed_passes() {
    // "café" with é = U+00E9 (single composed codepoint). NFC-stable.
    assert!(is_identifier("café"));
}

#[test]
fn uni4_ident_14_is_identifier_decomposed_fails_strict_nfc() {
    // "cafe" + U+0301 COMBINING ACUTE ACCENT. Syntactically a valid
    // UAX #31 identifier (e, 0301 both XID_Continue) but NOT in NFC —
    // the composition rule turns `e\u{0301}` into U+00E9. paideia-as
    // policy (SH-D9.4) rejects decomposed identifiers at the
    // capability-boundary layer; see `src/ident.rs` module header.
    assert!(!is_identifier("cafe\u{0301}"));
}

// ---------------------------------------------------------------------
// 5. `check_ident(Ascii)` — strict ASCII identifier profile.
// ---------------------------------------------------------------------

#[test]
fn uni4_ident_15_check_ascii_happy_and_leading_digit() {
    assert_eq!(check_ident("hello", IdentProfile::Ascii), Ok(()));
    assert_eq!(check_ident("x1", IdentProfile::Ascii), Ok(()));
    assert_eq!(check_ident("_", IdentProfile::Ascii), Ok(()));

    let err = check_ident("5foo", IdentProfile::Ascii).unwrap_err();
    assert_eq!(err.offset, 0);
    assert_eq!(err.kind, IdentErrorKind::BadStart);
}

#[test]
fn uni4_ident_16_check_ascii_rejects_non_ascii_first_char() {
    // Σ fails `[A-Za-z_]` even though it would be a valid UAX #31 start.
    let err = check_ident("Σ", IdentProfile::Ascii).unwrap_err();
    assert_eq!(err.offset, 0);
    assert_eq!(err.kind, IdentErrorKind::BadStart);
}

#[test]
fn uni4_ident_17_check_ascii_rejects_non_ascii_continue() {
    // `x` is a valid ASCII start; `α` (2 bytes in UTF-8 at offset 1) is
    // not `[A-Za-z0-9_]`.
    let err = check_ident("xα", IdentProfile::Ascii).unwrap_err();
    assert_eq!(err.offset, 1);
    assert_eq!(err.kind, IdentErrorKind::BadContinue);

    // Empty input is reported as BadStart at offset 0.
    let err = check_ident("", IdentProfile::Ascii).unwrap_err();
    assert_eq!(err.offset, 0);
    assert_eq!(err.kind, IdentErrorKind::BadStart);
}

// ---------------------------------------------------------------------
// 6. `check_ident(Default)` — UAX #31 + NFC-stable with error offsets.
// ---------------------------------------------------------------------

#[test]
fn uni4_ident_18_check_default_happy_and_bad_start() {
    assert_eq!(check_ident("hello", IdentProfile::Default), Ok(()));
    assert_eq!(check_ident("Σgma", IdentProfile::Default), Ok(()));
    assert_eq!(check_ident("日本", IdentProfile::Default), Ok(()));

    let err = check_ident("5foo", IdentProfile::Default).unwrap_err();
    assert_eq!(err.offset, 0);
    assert_eq!(err.kind, IdentErrorKind::BadStart);
}

#[test]
fn uni4_ident_19_check_default_bad_continue_offset() {
    // 'x' at byte 0, '-' at byte 1 — '-' is not XID_Continue.
    let err = check_ident("x-y", IdentProfile::Default).unwrap_err();
    assert_eq!(err.offset, 1);
    assert_eq!(err.kind, IdentErrorKind::BadContinue);

    // Multi-byte prefix: "αβ-γ" — α and β are each 2 UTF-8 bytes, so
    // '-' lies at byte offset 4.
    let err = check_ident("αβ-γ", IdentProfile::Default).unwrap_err();
    assert_eq!(err.offset, 4);
    assert_eq!(err.kind, IdentErrorKind::BadContinue);
}

#[test]
fn uni4_ident_20_check_default_rejects_decomposed_nfc() {
    // "cafe" + combining acute accent — syntactically fine, not NFC.
    let err = check_ident("cafe\u{0301}", IdentProfile::Default).unwrap_err();
    assert_eq!(err.offset, 0);
    assert_eq!(err.kind, IdentErrorKind::NotNfc);

    // The composed form passes.
    assert_eq!(check_ident("café", IdentProfile::Default), Ok(()));
}

// ---------------------------------------------------------------------
// 7. `check_ident(AllowedChars)` — UTS #39 §5 single-script restriction.
// ---------------------------------------------------------------------

#[test]
fn uni4_ident_21_check_allowed_chars_single_script_passes() {
    assert_eq!(check_ident("hello", IdentProfile::AllowedChars), Ok(()));
    assert_eq!(check_ident("αβγ", IdentProfile::AllowedChars), Ok(()));
    assert_eq!(check_ident("日本", IdentProfile::AllowedChars), Ok(()));
    // Common / Inherited characters alongside a single real script do
    // NOT trip the restriction (`_` is Common; `é`'s combining form
    // would be Inherited after NFD, but we require NFC).
    assert_eq!(
        check_ident("hello_world", IdentProfile::AllowedChars),
        Ok(())
    );
}

#[test]
fn uni4_ident_22_check_allowed_chars_rejects_greek_plus_latin() {
    // Σ (Greek) + gma (Latin) — UAX #31 passes, UTS #39 rejects.
    let err = check_ident("Σgma", IdentProfile::AllowedChars).unwrap_err();
    assert_eq!(
        err,
        IdentError {
            offset: 0,
            kind: IdentErrorKind::MixedScript {
                scripts: vec![Script::Greek, Script::Latin],
            },
        }
    );
}

#[test]
fn uni4_ident_23_check_allowed_chars_rejects_cjk_plus_latin() {
    // 日本 (Han) + hello (Latin). Deterministic sort by Debug-name:
    // "Han" < "Latin".
    let err = check_ident("日本hello", IdentProfile::AllowedChars).unwrap_err();
    assert_eq!(err.offset, 0);
    match err.kind {
        IdentErrorKind::MixedScript { scripts } => {
            assert_eq!(scripts, vec![Script::Han, Script::Latin]);
        }
        other => panic!("expected MixedScript, got {:?}", other),
    }
}

// ---------------------------------------------------------------------
// 8. `mixed_script` — the script-set helper on its own.
// ---------------------------------------------------------------------

#[test]
fn uni4_ident_24_mixed_script_empty_and_single_script() {
    assert_eq!(mixed_script(""), Vec::<Script>::new());
    assert_eq!(mixed_script("hello"), vec![Script::Latin]);
    assert_eq!(mixed_script("αβγ"), vec![Script::Greek]);
    assert_eq!(mixed_script("日本"), vec![Script::Han]);
}

#[test]
fn uni4_ident_25_mixed_script_greek_plus_latin_sorted() {
    // Sort by Script's Debug name: "Greek" < "Latin".
    assert_eq!(mixed_script("Σgma"), vec![Script::Greek, Script::Latin]);
    // Order of first appearance in input does not matter.
    assert_eq!(mixed_script("gmaΣ"), vec![Script::Greek, Script::Latin]);
}

#[test]
fn uni4_ident_26_mixed_script_three_scripts() {
    // 日 Han + 本 Han + ひ Hiragana + hello Latin.
    // Debug-name sort: "Han" < "Hiragana" < "Latin".
    assert_eq!(
        mixed_script("日本ひhello"),
        vec![Script::Han, Script::Hiragana, Script::Latin]
    );
}

#[test]
fn uni4_ident_27_mixed_script_filters_common_and_inherited() {
    // '_' is Common; '0' is Common (Nd with script Common per
    // Scripts.txt); combining marks (U+0300) are Inherited. None show
    // up in the output. "hello_123" → only Latin.
    assert_eq!(mixed_script("hello_123"), vec![Script::Latin]);
    // U+0300 COMBINING GRAVE ACCENT is Inherited — filtered.
    // "a\u{0300}" has characters {Latin, Inherited} → script set
    // excluding Inherited = {Latin}.
    assert_eq!(mixed_script("a\u{0300}"), vec![Script::Latin]);
    // Pure Common/Inherited input: no scripts.
    assert_eq!(mixed_script("___"), Vec::<Script>::new());
}

// ---------------------------------------------------------------------
// 9. Profile / struct invariants.
// ---------------------------------------------------------------------

#[test]
fn uni4_ident_28_profile_default_is_uax31_default() {
    // IdentProfile::default() must be the UAX #31 + NFC-stable level.
    assert_eq!(IdentProfile::default(), IdentProfile::Default);
    // Round-trip: default profile accepts the empty-safe happy case.
    assert_eq!(check_ident("hello", IdentProfile::default()), Ok(()));
}

#[test]
fn uni4_ident_29_ident_error_debug_and_clone() {
    // IdentError derives Debug + Clone + Eq — all four variants are
    // reachable and distinguishable.
    let bad_start = IdentError {
        offset: 0,
        kind: IdentErrorKind::BadStart,
    };
    let bad_continue = IdentError {
        offset: 3,
        kind: IdentErrorKind::BadContinue,
    };
    let not_nfc = IdentError {
        offset: 0,
        kind: IdentErrorKind::NotNfc,
    };
    let mixed = IdentError {
        offset: 0,
        kind: IdentErrorKind::MixedScript {
            scripts: vec![Script::Greek, Script::Latin],
        },
    };
    assert_ne!(bad_start, bad_continue);
    assert_ne!(bad_start, not_nfc);
    assert_ne!(bad_start, mixed);
    assert_eq!(bad_start.clone(), bad_start);
    // Debug formatting must not panic.
    let _ = format!("{:?}", mixed);
}

#[test]
fn uni4_ident_30_check_default_accepts_middle_dot_in_continue() {
    // U+00B7 MIDDLE DOT: UCD 15.1 DerivedCoreProperties lists it in
    // XID_Continue (retained for legacy Catalan `l·l`). It is NOT in
    // XID_Start, but an identifier `x·y` IS a valid UAX #31 identifier.
    // (The pre-X ID_Continue variant also included it, so no exclusion.)
    assert!(check_ident("x\u{00B7}y", IdentProfile::Default).is_ok());
    assert!(!is_xid_start('\u{00B7}'));
    assert!(is_xid_continue('\u{00B7}'));
}
