//! R221.M3 UAX#11 East-Asian-width conformance corpus.
//!
//! 25 vectors covering the four rendered widths (Zero / Narrow /
//! Ambiguous / Wide), the grapheme-cluster fold-in convention (family
//! emoji → one wide cell), the ANSI-escape strip in `str_width`, and
//! the R229 cursor-column invariant
//! `str_width(s) == Σ grapheme_width(cluster)`.
//!
//! Fingerprint discipline: `r221m3-w-NN` on every test.

use paideia_as_unicode::{Width, grapheme_width, str_width, width};
use unicode_segmentation::UnicodeSegmentation;

#[test]
fn r221m3_w_01_ascii_letter_is_narrow() {
    assert_eq!(width('a'), Width::Narrow, "r221m3-w-01");
    assert_eq!(width('A').columns_latin(), 1);
}

#[test]
fn r221m3_w_02_ascii_digit_is_narrow() {
    assert_eq!(width('7'), Width::Narrow, "r221m3-w-02");
}

#[test]
fn r221m3_w_03_ascii_punctuation_is_narrow() {
    for c in ['{', '}', '|', '(', ')', ',', '.', ';', '?', '$', '!'] {
        assert_eq!(width(c), Width::Narrow, "r221m3-w-03 char={c:?}");
    }
}

#[test]
fn r221m3_w_04_space_is_narrow() {
    // Space is not zero-width — it advances the cursor.
    assert_eq!(width(' '), Width::Narrow, "r221m3-w-04");
}

#[test]
fn r221m3_w_05_control_char_is_zero() {
    // NUL / BEL / ESC / DEL all render as zero columns.
    for c in ['\u{0000}', '\u{0007}', '\u{001B}', '\u{007F}'] {
        assert_eq!(width(c), Width::Zero, "r221m3-w-05 char={:?}", c);
    }
}

#[test]
fn r221m3_w_06_combining_acute_is_zero() {
    // U+0301 combining acute: a combining mark, zero columns.
    assert_eq!(width('\u{0301}'), Width::Zero, "r221m3-w-06");
}

#[test]
fn r221m3_w_07_zwj_is_zero() {
    // U+200D zero-width joiner: format-control, zero columns.
    assert_eq!(width('\u{200D}'), Width::Zero, "r221m3-w-07");
}

#[test]
fn r221m3_w_08_zero_width_space_is_zero() {
    assert_eq!(width('\u{200B}'), Width::Zero, "r221m3-w-08");
}

#[test]
fn r221m3_w_09_cjk_ideograph_is_wide() {
    // U+4E2D 中 — CJK Unified Ideograph.
    assert_eq!(width('\u{4E2D}'), Width::Wide, "r221m3-w-09");
    assert_eq!(width('\u{4E2D}').columns_latin(), 2);
}

#[test]
fn r221m3_w_10_fullwidth_ascii_is_wide() {
    // U+FF21 Ａ FULLWIDTH LATIN CAPITAL LETTER A.
    assert_eq!(width('\u{FF21}'), Width::Wide, "r221m3-w-10");
}

#[test]
fn r221m3_w_11_hangul_syllable_is_wide() {
    // U+AC00 가.
    assert_eq!(width('\u{AC00}'), Width::Wide, "r221m3-w-11");
}

#[test]
fn r221m3_w_12_emoji_is_wide() {
    // 😀 U+1F600.
    assert_eq!(width('\u{1F600}'), Width::Wide, "r221m3-w-12");
}

#[test]
fn r221m3_w_13_ambiguous_greek() {
    // U+03B1 α — UAX#11 property `A` (ambiguous).
    // Depending on `unicode-width`'s exact UCD revision, α is either
    // Ambiguous or Narrow; assert it is one of those two (never Wide
    // or Zero) so the test is stable across minor UCD bumps.
    let w = width('\u{03B1}');
    assert!(
        matches!(w, Width::Ambiguous | Width::Narrow),
        "r221m3-w-13 got {:?}",
        w
    );
}

#[test]
fn r221m3_w_14_ambiguous_variant_column_split() {
    // For any character whose width is `Ambiguous`, latin-locale is 1
    // and east-asian-locale is 2. This is the SH-D9 locale-switch
    // contract R229 will consume.
    assert_eq!(Width::Ambiguous.columns_latin(), 1, "r221m3-w-14 latin");
    assert_eq!(
        Width::Ambiguous.columns_east_asian(),
        2,
        "r221m3-w-14 east-asian"
    );
    assert_eq!(Width::Zero.columns_east_asian(), 0);
    assert_eq!(Width::Narrow.columns_east_asian(), 1);
    assert_eq!(Width::Wide.columns_east_asian(), 2);
}

#[test]
fn r221m3_w_15_grapheme_width_ascii() {
    assert_eq!(grapheme_width("a"), 1, "r221m3-w-15");
}

#[test]
fn r221m3_w_16_grapheme_width_decomposed_eacute() {
    // "e" + U+0301 combining acute → one cluster, one column (the
    // combining mark folds into the base cell).
    assert_eq!(grapheme_width("e\u{0301}"), 1, "r221m3-w-16");
}

#[test]
fn r221m3_w_17_grapheme_width_family_emoji_is_two_columns() {
    // 👨‍👩‍👧‍👦 = MAN ZWJ WOMAN ZWJ GIRL ZWJ BOY (7 code points, 25 bytes).
    // Rendered as ONE terminal cell of width 2 — not 8 (4 wide bases).
    let cluster = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    assert_eq!(grapheme_width(cluster), 2, "r221m3-w-17");
}

#[test]
fn r221m3_w_18_grapheme_width_skin_tone_emoji_is_two_columns() {
    // 👍🏽 = THUMBS UP + MEDIUM SKIN TONE. One cluster, 2 columns.
    let cluster = "\u{1F44D}\u{1F3FD}";
    assert_eq!(grapheme_width(cluster), 2, "r221m3-w-18");
}

#[test]
fn r221m3_w_19_grapheme_width_flag_is_two_columns() {
    // 🇺🇸 = REGIONAL INDICATOR U + S. One cluster. Terminal column-count
    // varies: some emit 2 (wide flag glyph), others 1 (Servo unicode-width
    // treats the pair as narrow because the individual RI code points are
    // classified Neutral). Test accepts either — r221m3-w-19 documents
    // the cluster-vs-ambiguity boundary rather than pinning one number.
    let cluster = "\u{1F1FA}\u{1F1F8}";
    let w = grapheme_width(cluster);
    assert!(w == 1 || w == 2, "r221m3-w-19: got {w}, expected 1 or 2");
}

#[test]
fn r221m3_w_20_grapheme_width_multi_diacritic_stays_one() {
    // 'a' + circumflex + acute → one cluster, one column.
    assert_eq!(grapheme_width("a\u{0302}\u{0301}"), 1, "r221m3-w-20");
}

#[test]
fn r221m3_w_21_grapheme_width_bare_combining_mark() {
    // A stray combining mark rendered on its own (rare, but possible
    // in a paste): must return 0, not 1.
    assert_eq!(grapheme_width("\u{0301}"), 0, "r221m3-w-21");
}

#[test]
fn r221m3_w_22_grapheme_width_bare_esc_is_zero() {
    // A lone ESC surviving upstream stripping still returns 0.
    assert_eq!(grapheme_width("\u{001B}"), 0, "r221m3-w-22");
}

#[test]
fn r221m3_w_23_grapheme_width_empty_string_is_zero() {
    assert_eq!(grapheme_width(""), 0, "r221m3-w-23");
}

#[test]
fn r221m3_w_24_str_width_ascii_word() {
    assert_eq!(str_width("hello"), 5, "r221m3-w-24");
}

#[test]
fn r221m3_w_25_str_width_mixed_ascii_and_cjk() {
    // "a中b" — 1 + 2 + 1 = 4.
    assert_eq!(str_width("a\u{4E2D}b"), 4, "r221m3-w-25");
}

#[test]
fn r221m3_w_26_str_width_strips_csi_color_escape() {
    // Red ANSI wrapper around "hello" — the escape bytes contribute 0.
    let s = "\u{001B}[31mhello\u{001B}[0m";
    assert_eq!(str_width(s), 5, "r221m3-w-26");
}

#[test]
fn r221m3_w_27_str_width_strips_osc_hyperlink_escape() {
    // ESC ] 8 ; ; https://example.com/ BEL text ESC ] 8 ; ; BEL — OSC
    // 8 hyperlink form (`hyperlink` in `less -R`). Only "text" counts.
    let s = "\u{001B}]8;;https://example.com/\u{0007}text\u{001B}]8;;\u{0007}";
    assert_eq!(str_width(s), 4, "r221m3-w-27");
}

#[test]
fn r221m3_w_28_str_width_family_emoji_and_ascii() {
    // "hi 👨‍👩‍👧‍👦!" → 2 (hi) + 1 (space) + 2 (family) + 1 (!) = 6.
    let s = "hi \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}!";
    assert_eq!(str_width(s), 6, "r221m3-w-28");
}

#[test]
fn r221m3_w_29_str_width_equals_sum_of_grapheme_widths_invariant() {
    // The R229 cursor-column invariant. Verified on a heterogeneous
    // string spanning ASCII, CJK, precomposed accent, decomposed
    // accent, and a family emoji.
    let s = "a\u{4E2D}b\u{00E9}e\u{0301}!\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    let via_str = str_width(s);
    let via_clusters: usize = s.graphemes(true).map(grapheme_width).sum();
    assert_eq!(
        via_str, via_clusters,
        "r221m3-w-29 str_width={via_str} vs Σgrapheme_width={via_clusters}"
    );
}

#[test]
fn r221m3_w_30_str_width_empty_is_zero() {
    assert_eq!(str_width(""), 0, "r221m3-w-30");
}
