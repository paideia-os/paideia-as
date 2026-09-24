//! R221.M2 UAX#29 extended-grapheme-cluster conformance corpus.
//!
//! 30 vectors distilled from UCD 15.1's `GraphemeBreakTest.txt`
//! covering every UAX#29 §3.1 rule (GB1..GB13):
//!
//!  * GB1/GB2: string boundaries.
//!  * GB3: CR × LF fuses.
//!  * GB4/GB5: control characters break on both sides.
//!  * GB6/GB7/GB8: Hangul L/V/T sequences fuse.
//!  * GB9: extends and ZWJ fuse to the preceding cluster.
//!  * GB9a/GB9b: spacing marks and prepends.
//!  * GB11: extended-pictographic × ZWJ × extended-pictographic fuses
//!    (family emoji, profession sequences).
//!  * GB12/GB13: paired regional-indicator symbols fuse (flags).
//!
//! Fingerprint discipline: `r221m2-tr29-NN` on every test.
//! Cursor-math helpers (`grapheme_advance` / `grapheme_retreat`) also
//! get their own tests since the R229 REPL line editor consumes them
//! directly.

use paideia_as_unicode::{
    grapheme_advance, grapheme_boundaries, grapheme_count, grapheme_retreat,
};

fn boundaries(s: &str) -> Vec<usize> {
    grapheme_boundaries(s).collect()
}

/// Assert grapheme count and record the fingerprint tag in the binary.
fn assert_graphemes(s: &str, expected: usize, fingerprint: &str) {
    let got = grapheme_count(s);
    assert_eq!(got, expected, "{fingerprint}: grapheme count");
    // Boundary list is always `expected + 1` (start + one per cluster
    // end, with duplicates removed by definition).
    let bounds = boundaries(s);
    assert_eq!(bounds.len(), expected + 1, "{fingerprint}: boundary count");
    assert_eq!(bounds[0], 0, "{fingerprint}: first boundary");
    assert_eq!(*bounds.last().unwrap(), s.len(), "{fingerprint}: last boundary");
    assert!(!fingerprint.is_empty());
}

#[test]
fn r221m2_tr29_01_empty_string_has_one_boundary() {
    // GB1/GB2: start-of-string and end-of-string are both boundaries;
    // for the empty string those coincide.
    assert_eq!(boundaries(""), vec![0], "r221m2-tr29-01");
    assert_eq!(grapheme_count(""), 0);
}

#[test]
fn r221m2_tr29_02_ascii_one_cluster_per_char() {
    assert_graphemes("hello", 5, "r221m2-tr29-02");
}

#[test]
fn r221m2_tr29_03_ascii_with_spaces() {
    assert_graphemes("a b c", 5, "r221m2-tr29-03");
}

#[test]
fn r221m2_tr29_04_cr_lf_fuses_gb3() {
    // GB3: CR × LF is one cluster.
    assert_graphemes("\r\n", 1, "r221m2-tr29-04");
}

#[test]
fn r221m2_tr29_05_lf_cr_does_not_fuse_gb4_gb5() {
    // GB4/GB5: LF then CR are two clusters (order matters).
    assert_graphemes("\n\r", 2, "r221m2-tr29-05");
}

#[test]
fn r221m2_tr29_06_ascii_then_control() {
    // GB4: control breaks after preceding non-control.
    assert_graphemes("a\x07", 2, "r221m2-tr29-06");
}

#[test]
fn r221m2_tr29_07_precomposed_eacute_one_cluster() {
    assert_graphemes("\u{00E9}", 1, "r221m2-tr29-07");
}

#[test]
fn r221m2_tr29_08_decomposed_eacute_still_one_cluster() {
    // GB9: extend (U+0301) fuses to preceding base.
    assert_graphemes("e\u{0301}", 1, "r221m2-tr29-08");
}

#[test]
fn r221m2_tr29_09_two_decomposed_eacutes_are_two_clusters() {
    assert_graphemes("e\u{0301}e\u{0301}", 2, "r221m2-tr29-09");
}

#[test]
fn r221m2_tr29_10_multi_diacritic_one_cluster() {
    // 'a' + circumflex + acute → one cluster (GB9 twice).
    assert_graphemes("a\u{0302}\u{0301}", 1, "r221m2-tr29-10");
}

#[test]
fn r221m2_tr29_11_hangul_L_V_one_cluster_gb6_gb7() {
    // GB6/GB7: L × V.
    assert_graphemes("\u{1100}\u{1161}", 1, "r221m2-tr29-11");
}

#[test]
fn r221m2_tr29_12_hangul_L_V_T_one_cluster_gb8() {
    // GB8: LVT × T.
    assert_graphemes("\u{1100}\u{1161}\u{11A8}", 1, "r221m2-tr29-12");
}

#[test]
fn r221m2_tr29_13_precomposed_hangul_syllable() {
    assert_graphemes("\u{AC00}", 1, "r221m2-tr29-13");
}

#[test]
fn r221m2_tr29_14_devanagari_spacing_mark_gb9a() {
    // 'क' + spacing mark 'ि' (U+093F) → one cluster (GB9a).
    assert_graphemes("\u{0915}\u{093F}", 1, "r221m2-tr29-14");
}

#[test]
fn r221m2_tr29_15_devanagari_kshi_cluster() {
    // KA + VIRAMA + SSA + I: complex Indic cluster is one grapheme.
    assert_graphemes("\u{0915}\u{094D}\u{0937}\u{093F}", 1, "r221m2-tr29-15");
}

#[test]
fn r221m2_tr29_16_emoji_single() {
    // 😀 = U+1F600. One cluster, 4 bytes.
    let s = "\u{1F600}";
    assert_graphemes(s, 1, "r221m2-tr29-16");
    assert_eq!(s.len(), 4);
}

#[test]
fn r221m2_tr29_17_emoji_with_skin_tone_modifier() {
    // 👍🏽 = U+1F44D U+1F3FD. One cluster (extend fuses).
    assert_graphemes("\u{1F44D}\u{1F3FD}", 1, "r221m2-tr29-17");
}

#[test]
fn r221m2_tr29_18_emoji_zwj_family() {
    // 👨‍👩‍👧‍👦 = MAN ZWJ WOMAN ZWJ GIRL ZWJ BOY. One cluster (GB11).
    let s = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    assert_graphemes(s, 1, "r221m2-tr29-18");
}

#[test]
fn r221m2_tr29_19_regional_indicator_flag() {
    // 🇺🇸 = U+1F1FA U+1F1F8. One cluster (GB12/GB13, first RI pair).
    assert_graphemes("\u{1F1FA}\u{1F1F8}", 1, "r221m2-tr29-19");
}

#[test]
fn r221m2_tr29_20_two_flags_are_two_clusters() {
    // 🇺🇸🇯🇵. GB12/GB13: only *odd-indexed* RIs pair, so the second
    // flag starts a new cluster.
    let s = "\u{1F1FA}\u{1F1F8}\u{1F1EF}\u{1F1F5}";
    assert_graphemes(s, 2, "r221m2-tr29-20");
}

#[test]
fn r221m2_tr29_21_zwj_between_non_pictographic_breaks() {
    // GB11 requires *extended pictographic* on both sides of ZWJ. A
    // plain letter + ZWJ + letter does not fuse — the ZWJ attaches to
    // the letter (as an extend), then the next letter starts a new
    // cluster.
    assert_graphemes("a\u{200D}b", 2, "r221m2-tr29-21");
}

#[test]
fn r221m2_tr29_22_prepend_gb9b() {
    // Arabic number sign U+0600 is Prepend; fuses to the next base.
    assert_graphemes("\u{0600}1", 1, "r221m2-tr29-22");
}

#[test]
fn r221m2_tr29_23_ascii_then_emoji() {
    // "a😀" — two clusters, different byte widths.
    assert_graphemes("a\u{1F600}", 2, "r221m2-tr29-23");
}

#[test]
fn r221m2_tr29_24_boundary_offsets_ascii() {
    assert_eq!(boundaries("abc"), vec![0, 1, 2, 3], "r221m2-tr29-24");
}

#[test]
fn r221m2_tr29_25_boundary_offsets_emoji_zwj_family() {
    // Whole family emoji is one cluster: boundaries are just [0, len].
    let s = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    assert_eq!(boundaries(s), vec![0, s.len()], "r221m2-tr29-25");
}

#[test]
fn r221m2_tr29_26_grapheme_advance_ascii() {
    // 'abc': from 0 advance to 1, from 1 to 2, from 2 to 3, from 3 to 3.
    let s = "abc";
    assert_eq!(grapheme_advance(s, 0), 1, "r221m2-tr29-26 step 0");
    assert_eq!(grapheme_advance(s, 1), 2, "r221m2-tr29-26 step 1");
    assert_eq!(grapheme_advance(s, 2), 3, "r221m2-tr29-26 step 2");
    assert_eq!(grapheme_advance(s, 3), 3, "r221m2-tr29-26 step 3 (clamp)");
}

#[test]
fn r221m2_tr29_27_grapheme_advance_family_emoji_leaps_full_cluster() {
    // The R229 REPL cursor over the family emoji must move past all 25
    // bytes in one right-arrow keystroke.
    let s = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    assert_eq!(grapheme_advance(s, 0), s.len(), "r221m2-tr29-27");
}

#[test]
fn r221m2_tr29_28_grapheme_retreat_family_emoji_leaps_full_cluster() {
    let s = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    assert_eq!(grapheme_retreat(s, s.len()), 0, "r221m2-tr29-28");
}

#[test]
fn r221m2_tr29_29_grapheme_retreat_from_zero_clamps() {
    assert_eq!(grapheme_retreat("hello", 0), 0, "r221m2-tr29-29");
}

#[test]
fn r221m2_tr29_30_advance_from_mid_cluster_rounds_down() {
    // Cursor lands mid-cluster (e.g. after a partial paste). Advance
    // must round down to the cluster start and then move forward one
    // cluster, not corrupt the string.
    let s = "e\u{0301}x"; // decomposed é + 'x': two clusters, 4 bytes.
    // Offset 2 is between the base 'e' (1 byte) and the combining
    // acute (2 bytes: U+0301 = 0xCC 0x81) — mid-cluster. Round down
    // to 0, advance one cluster → 3 (end of é).
    assert_eq!(grapheme_advance(s, 2), 3, "r221m2-tr29-30");
}
