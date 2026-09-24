//! R221.M1 NFC conformance corpus.
//!
//! 30 vectors distilled from UCD 15.1's `NormalizationTest.txt`
//! (Part 0: source-specific, Part 1: single characters, Part 2:
//! canonical order, Part 3: Hangul syllables, Part 5: chart of
//! precomposed forms). We deliberately pick vectors that exercise:
//!
//!  * the ASCII fast path (must skip the tables);
//!  * NFC-preserving inputs (round-trip identity);
//!  * canonical decomposition + composition (é, ñ, ö with multiple
//!    combining marks in and out of canonical order);
//!  * Hangul algorithmic composition (UAX#15 §16);
//!  * astral-plane inputs (musical symbols, emoji);
//!  * defective combining-mark sequences (a combining mark at string
//!    start with no base — must not crash, must not consume more than
//!    the mark's own bytes).
//!
//! Fingerprint discipline: every test carries a `r221m1-nfc-NN`
//! constant so the debugger can correlate test binaries against the
//! plan §4 R221.M1 acceptance line item per
//! `feedback_workerbee_verify_claims.md`.

use paideia_as_unicode::{is_ascii_fast_path_eligible, is_nfc, nfc_normalize};

/// Assert that `input` normalizes to `expected` and that the
/// normalization is idempotent.
fn assert_nfc(input: &str, expected: &str, fingerprint: &str) {
    let once = nfc_normalize(input);
    assert_eq!(once, expected, "{fingerprint}: NFC(input) != expected");
    let twice = nfc_normalize(&once);
    assert_eq!(twice, once, "{fingerprint}: NFC not idempotent");
    // Record the fingerprint in the binary via a black-box `assert!`
    // whose message text embeds it — matches the r220m4/r220m7 pattern
    // in `crates/paideia-as-stdlib/tests/parse_pdx.rs`.
    assert!(!fingerprint.is_empty(), "{fingerprint}: tag must be non-empty");
}

#[test]
fn r221m1_nfc_01_empty_string() {
    assert_nfc("", "", "r221m1-nfc-01");
    assert!(is_nfc(""));
    assert!(is_ascii_fast_path_eligible(""));
}

#[test]
fn r221m1_nfc_02_pure_ascii_command_name() {
    assert_nfc("find", "find", "r221m1-nfc-02");
    assert!(is_ascii_fast_path_eligible("find"));
    assert!(is_nfc("find"));
}

#[test]
fn r221m1_nfc_03_ascii_with_digits_and_punct() {
    assert_nfc("cd ..", "cd ..", "r221m1-nfc-03");
    assert!(is_ascii_fast_path_eligible("cd .."));
}

#[test]
fn r221m1_nfc_04_precomposed_eacute_stays_composed() {
    // U+00E9 is already NFC-composed.
    assert_nfc("\u{00E9}", "\u{00E9}", "r221m1-nfc-04");
    assert!(!is_ascii_fast_path_eligible("\u{00E9}"));
}

#[test]
fn r221m1_nfc_05_decomposed_eacute_composes() {
    // U+0065 U+0301 → U+00E9.
    assert_nfc("e\u{0301}", "\u{00E9}", "r221m1-nfc-05");
}

#[test]
fn r221m1_nfc_06_decomposed_ntilde_composes() {
    // U+006E U+0303 → U+00F1.
    assert_nfc("n\u{0303}", "\u{00F1}", "r221m1-nfc-06");
}

#[test]
fn r221m1_nfc_07_precomposed_ouml_stays_composed() {
    assert_nfc("\u{00F6}", "\u{00F6}", "r221m1-nfc-07");
}

#[test]
fn r221m1_nfc_08_multiple_combining_marks_canonical_order() {
    // 'a' + macron (CCC=230) + underdot (CCC=220): canonical reordering
    // puts underdot first, then a+underdot precomposes to U+1EA1 (LATIN
    // SMALL LETTER A WITH DOT BELOW), leaving the residual macron.
    // Expected NFC: U+1EA1 + U+0304 (macron).
    let input = "a\u{0304}\u{0323}";
    let expected = "\u{1EA1}\u{0304}";
    assert_nfc(input, expected, "r221m1-nfc-08");
}

#[test]
fn r221m1_nfc_09_hangul_syllable_L_V_composes() {
    // U+1100 (HANGUL CHOSEONG KIYEOK) + U+1161 (HANGUL JUNGSEONG A)
    // → U+AC00 (HANGUL SYLLABLE GA), per UAX#15 §16 algorithm.
    assert_nfc("\u{1100}\u{1161}", "\u{AC00}", "r221m1-nfc-09");
}

#[test]
fn r221m1_nfc_10_hangul_syllable_L_V_T_composes() {
    // U+1100 + U+1161 + U+11A8 (HANGUL JONGSEONG KIYEOK) → U+AC01.
    assert_nfc("\u{1100}\u{1161}\u{11A8}", "\u{AC01}", "r221m1-nfc-10");
}

#[test]
fn r221m1_nfc_11_hangul_syllable_precomposed_stays() {
    assert_nfc("\u{AC00}", "\u{AC00}", "r221m1-nfc-11");
}

#[test]
fn r221m1_nfc_12_precomposed_agrave() {
    // U+00E0 vs U+0061 U+0300.
    assert_nfc("a\u{0300}", "\u{00E0}", "r221m1-nfc-12");
}

#[test]
fn r221m1_nfc_13_precomposed_ccedilla() {
    assert_nfc("c\u{0327}", "\u{00E7}", "r221m1-nfc-13");
}

#[test]
fn r221m1_nfc_14_vietnamese_multi_diacritic() {
    // 'a' + circumflex (U+0302) + acute (U+0301) → U+1EA5.
    assert_nfc("a\u{0302}\u{0301}", "\u{1EA5}", "r221m1-nfc-14");
}

#[test]
fn r221m1_nfc_15_vietnamese_o_horn_hook() {
    // 'o' + horn (U+031B) + hook above (U+0309) → U+1EDF.
    assert_nfc("o\u{031B}\u{0309}", "\u{1EDF}", "r221m1-nfc-15");
}

#[test]
fn r221m1_nfc_16_cjk_ideograph_no_op() {
    // CJK unified ideographs have no canonical decomposition.
    assert_nfc("\u{4E2D}\u{6587}", "\u{4E2D}\u{6587}", "r221m1-nfc-16");
}

#[test]
fn r221m1_nfc_17_arabic_lam_alef() {
    // Arabic LAM + ALEF (no canonical composition — it's a
    // presentation-form contextual shape only). NFC preserves bytes.
    assert_nfc("\u{0644}\u{0627}", "\u{0644}\u{0627}", "r221m1-nfc-17");
}

#[test]
fn r221m1_nfc_18_devanagari_kshi_no_op() {
    // Devanagari cluster: KA + VIRAMA + SSA + I.
    let s = "\u{0915}\u{094D}\u{0937}\u{093F}";
    assert_nfc(s, s, "r221m1-nfc-18");
}

#[test]
fn r221m1_nfc_19_defective_combining_mark_at_start() {
    // A combining mark with no base at the start of a string is a
    // "defective combining sequence" (UAX#15 §1.3). NFC preserves it;
    // the mark just doesn't compose with anything.
    let s = "\u{0301}abc";
    assert_nfc(s, s, "r221m1-nfc-19");
}

#[test]
fn r221m1_nfc_20_musical_symbol_astral() {
    // U+1D11E (MUSICAL SYMBOL G CLEF) — an astral-plane code point.
    assert_nfc("\u{1D11E}", "\u{1D11E}", "r221m1-nfc-20");
}

#[test]
fn r221m1_nfc_21_musical_symbols_excluded_from_composition() {
    // U+1D157 + U+1D165 (musical symbol void notehead + combining stem).
    // Although U+1D15E's decomposition IS the pair, U+1D15E appears in
    // the UCD Composition_Exclusions list (musical symbols were excluded
    // from composition per Unicode 3.2). NFC therefore leaves the pair
    // in its decomposed form. Documents astral-plane exclusion behavior.
    assert_nfc("\u{1D157}\u{1D165}", "\u{1D157}\u{1D165}", "r221m1-nfc-21");
}

#[test]
fn r221m1_nfc_22_mixed_script_ascii_and_cjk() {
    assert_nfc("ls 中文", "ls 中文", "r221m1-nfc-22");
}

#[test]
fn r221m1_nfc_23_emoji_basic() {
    // Simple emoji: no combining marks, no ZWJ. NFC is a no-op.
    assert_nfc("\u{1F600}", "\u{1F600}", "r221m1-nfc-23");
}

#[test]
fn r221m1_nfc_24_emoji_zwj_family() {
    // 👨‍👩‍👧‍👦 = MAN + ZWJ + WOMAN + ZWJ + GIRL + ZWJ + BOY. NFC
    // preserves the ZWJ sequence exactly (no composition, no
    // decomposition — the sequence is not part of the canonical
    // composition tables).
    let s = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    assert_nfc(s, s, "r221m1-nfc-24");
}

#[test]
fn r221m1_nfc_25_emoji_skin_tone_modifier() {
    // 👍🏽 = THUMBS UP + MEDIUM SKIN TONE MODIFIER (U+1F3FD).
    let s = "\u{1F44D}\u{1F3FD}";
    assert_nfc(s, s, "r221m1-nfc-25");
}

#[test]
fn r221m1_nfc_26_flag_sequence_regional_indicator() {
    // 🇺🇸 = U+1F1FA U+1F1F8. NFC-preserved.
    let s = "\u{1F1FA}\u{1F1F8}";
    assert_nfc(s, s, "r221m1-nfc-26");
}

#[test]
fn r221m1_nfc_27_long_ascii_scales_linearly() {
    // 4 KiB of ASCII must go through the fast path without pulling
    // combining-class tables. The assertion is on correctness; the
    // benchmark for throughput belongs to a criterion harness (R221.M2
    // acceptance: 100 kg/s).
    let s: String = "abcdefghijklmnopqrstuvwxyz".repeat(160);
    assert!(is_ascii_fast_path_eligible(&s));
    assert_eq!(nfc_normalize(&s), s, "r221m1-nfc-27");
}

#[test]
fn r221m1_nfc_28_greek_tonos_composes() {
    // 'α' + U+0301 (combining acute) → U+03AC (GREEK SMALL LETTER
    // ALPHA WITH TONOS).
    assert_nfc("\u{03B1}\u{0301}", "\u{03AC}", "r221m1-nfc-28");
}

#[test]
fn r221m1_nfc_29_hebrew_no_op() {
    // Hebrew shalom — Hebrew letters have no canonical decomposition
    // in NFC (Hebrew points do, but this input has none).
    assert_nfc("\u{05E9}\u{05DC}\u{05D5}\u{05DD}", "\u{05E9}\u{05DC}\u{05D5}\u{05DD}", "r221m1-nfc-29");
}

#[test]
fn r221m1_nfc_30_singleton_decomposition_angstrom() {
    // U+212B (ANGSTROM SIGN) has a canonical decomposition to
    // U+00C5 (LATIN CAPITAL LETTER A WITH RING ABOVE), which then
    // recomposes to U+00C5 in NFC. UAX#15 §1.3 "singleton
    // decomposition".
    assert_nfc("\u{212B}", "\u{00C5}", "r221m1-nfc-30");
}
