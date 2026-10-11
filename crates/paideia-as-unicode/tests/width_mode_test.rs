//! UNI-9 (paideia-as#1596) — `WidthMode` locale resolver conformance.
//!
//! Validates that:
//!   * UAX#11 `Ambiguous` code points render one column under
//!     [`WidthMode::Latin`] and two columns under
//!     [`WidthMode::EastAsian`].
//!   * ASCII (always `Narrow`) and CJK ideographs (always `Wide`) are
//!     invariant across both modes.
//!   * The pre-UNI-9 one-mode API (`width` / `grapheme_width` /
//!     `str_width`) is a thin wrapper over the `_in` form with
//!     [`WidthMode::default()`] (= `Latin`) and sees unchanged
//!     behavior.
//!   * The ANSI escape stripping path is preserved under both modes.
//!   * The composition identity holds in both modes:
//!     `str_width_in(s, m) == Σ grapheme_width_in(cluster, m)` over
//!     the ANSI-stripped grapheme segmentation of `s`.
//!
//! Fingerprint discipline: `uni9-widthmode-NN` on every test.

use paideia_as_unicode::{
    Width, WidthMode, grapheme_width, grapheme_width_in, str_width, str_width_in, width, width_in,
};
use unicode_segmentation::UnicodeSegmentation;

// --------------------------------------------------------------------
// 01-05: single-char width resolution
// --------------------------------------------------------------------

#[test]
fn uni9_widthmode_01_ambiguous_greek_alpha_latin_narrow() {
    // U+03B1 GREEK SMALL LETTER ALPHA — UAX#11 property `A`.
    assert_eq!(width_in('α', WidthMode::Latin), Width::Narrow, "uni9-widthmode-01");
}

#[test]
fn uni9_widthmode_02_ambiguous_greek_alpha_east_asian_wide() {
    // Same code point, East-Asian locale: two columns.
    assert_eq!(width_in('α', WidthMode::EastAsian), Width::Wide, "uni9-widthmode-02");
}

#[test]
fn uni9_widthmode_03_ascii_narrow_in_both_modes() {
    // ASCII can never be `Ambiguous`; both modes agree.
    for c in ['a', 'A', '7', ' ', '{', '!', '~'] {
        assert_eq!(width_in(c, WidthMode::Latin), Width::Narrow, "uni9-widthmode-03 lat {c:?}");
        assert_eq!(width_in(c, WidthMode::EastAsian), Width::Narrow, "uni9-widthmode-03 ea {c:?}");
    }
}

#[test]
fn uni9_widthmode_04_cjk_wide_in_both_modes() {
    // U+65E5 日 — CJK Unified Ideograph, always Wide.
    assert_eq!(width_in('日', WidthMode::Latin), Width::Wide, "uni9-widthmode-04 lat");
    assert_eq!(width_in('日', WidthMode::EastAsian), Width::Wide, "uni9-widthmode-04 ea");
}

#[test]
fn uni9_widthmode_05_zero_width_both_modes() {
    // Combining marks, ZWJ, ZWSP, controls — Zero under either mode.
    for c in ['\u{0301}', '\u{200D}', '\u{200B}', '\u{0000}', '\u{001B}'] {
        assert_eq!(width_in(c, WidthMode::Latin), Width::Zero, "uni9-widthmode-05 lat {:?}", c);
        assert_eq!(width_in(c, WidthMode::EastAsian), Width::Zero, "uni9-widthmode-05 ea {:?}", c);
    }
}

// --------------------------------------------------------------------
// 06-10: str_width_in column counts
// --------------------------------------------------------------------

#[test]
fn uni9_widthmode_06_ambiguous_string_latin_counts_narrow() {
    // αβγ — three Greek letters, each `Ambiguous` → 1 col under Latin.
    assert_eq!(str_width_in("αβγ", WidthMode::Latin), 3, "uni9-widthmode-06");
}

#[test]
fn uni9_widthmode_07_ambiguous_string_east_asian_counts_wide() {
    // Same three letters, East-Asian → 2 cols each.
    assert_eq!(str_width_in("αβγ", WidthMode::EastAsian), 6, "uni9-widthmode-07");
}

#[test]
fn uni9_widthmode_08_ascii_string_mode_invariant() {
    // ASCII string: both modes should agree exactly.
    let s = "hello world!";
    assert_eq!(
        str_width_in(s, WidthMode::Latin),
        str_width_in(s, WidthMode::EastAsian),
        "uni9-widthmode-08",
    );
    assert_eq!(str_width_in(s, WidthMode::Latin), 12, "uni9-widthmode-08 abs");
}

#[test]
fn uni9_widthmode_09_cjk_string_mode_invariant() {
    // 日本語 — three CJK ideographs; always Wide (6 cols) in both modes.
    assert_eq!(str_width_in("日本語", WidthMode::Latin), 6, "uni9-widthmode-09 lat");
    assert_eq!(str_width_in("日本語", WidthMode::EastAsian), 6, "uni9-widthmode-09 ea");
}

#[test]
fn uni9_widthmode_10_mixed_latin_cjk_ambiguous() {
    // "a日α": ASCII (1) + CJK (2) + ambiguous → 1 under Latin, 2 under EA.
    assert_eq!(str_width_in("a日α", WidthMode::Latin), 1 + 2 + 1, "uni9-widthmode-10 lat");
    assert_eq!(str_width_in("a日α", WidthMode::EastAsian), 1 + 2 + 2, "uni9-widthmode-10 ea");
}

// --------------------------------------------------------------------
// 11-13: grapheme_width_in semantics
// --------------------------------------------------------------------

#[test]
fn uni9_widthmode_11_precomposed_e_acute_narrow_in_both() {
    // U+00E9 é is UAX#11 `Narrow` (property N) — not Ambiguous.
    // Both modes should see one column.
    assert_eq!(grapheme_width_in("é", WidthMode::Latin), 1, "uni9-widthmode-11 lat");
    assert_eq!(grapheme_width_in("é", WidthMode::EastAsian), 1, "uni9-widthmode-11 ea");
}

#[test]
fn uni9_widthmode_12_decomposed_e_combining_acute_narrow_both() {
    // "e" + U+0301 combining acute — one cluster, Narrow base → 1 col
    // in both modes (combining mark is Zero).
    let cluster = "e\u{0301}";
    assert_eq!(grapheme_width_in(cluster, WidthMode::Latin), 1, "uni9-widthmode-12 lat");
    assert_eq!(grapheme_width_in(cluster, WidthMode::EastAsian), 1, "uni9-widthmode-12 ea");
}

#[test]
fn uni9_widthmode_13_ambiguous_grapheme_differs_by_mode() {
    // A single Greek alpha as a one-char cluster: 1 col Latin, 2 EA.
    assert_eq!(grapheme_width_in("α", WidthMode::Latin), 1, "uni9-widthmode-13 lat");
    assert_eq!(grapheme_width_in("α", WidthMode::EastAsian), 2, "uni9-widthmode-13 ea");
}

// --------------------------------------------------------------------
// 14-16: default-mode wrappers preserve pre-UNI-9 behavior
// --------------------------------------------------------------------

#[test]
fn uni9_widthmode_14_default_is_latin() {
    // Explicit sanity: `WidthMode::default()` is `Latin`.
    assert_eq!(WidthMode::default(), WidthMode::Latin, "uni9-widthmode-14");
}

#[test]
fn uni9_widthmode_15_str_width_matches_default_in() {
    // The wrapper `str_width` must dispatch exactly to the `_in`
    // form with `WidthMode::default()`. Pick an Ambiguous-rich string
    // so a regression in the wrapper surfaces as a column diff.
    let s = "αβγ box─drawing";
    assert_eq!(
        str_width(s),
        str_width_in(s, WidthMode::default()),
        "uni9-widthmode-15",
    );
    // And under Latin resolution ambiguous counts as 1 — αβγ = 3 cols
    // + " box" = 4 + "─" (ambiguous → 1) + "drawing" = 7 → total 15.
    assert_eq!(str_width(s), 15, "uni9-widthmode-15 abs");
}

#[test]
fn uni9_widthmode_16_width_wrapper_matches_default_in() {
    for c in ['a', 'α', '日', '\u{0301}', '\u{001B}', ' ', '7'] {
        assert_eq!(
            width(c),
            width_in(c, WidthMode::default()),
            "uni9-widthmode-16 char={:?}",
            c,
        );
    }
}

// --------------------------------------------------------------------
// 17-18: ANSI escape stripping preserved under new API
// --------------------------------------------------------------------

#[test]
fn uni9_widthmode_17_ansi_strip_latin() {
    // CSI SGR red + reset wrapping one Ambiguous char → 1 col Latin.
    assert_eq!(
        str_width_in("\x1b[31mα\x1b[0m", WidthMode::Latin),
        1,
        "uni9-widthmode-17",
    );
}

#[test]
fn uni9_widthmode_18_ansi_strip_east_asian() {
    // Same bytes, East-Asian — CSI still stripped; ambiguous → 2 cols.
    assert_eq!(
        str_width_in("\x1b[31mα\x1b[0m", WidthMode::EastAsian),
        2,
        "uni9-widthmode-18",
    );
}

// --------------------------------------------------------------------
// 19-20: composition identity + grapheme wrapper default
// --------------------------------------------------------------------

#[test]
fn uni9_widthmode_19_composition_identity_both_modes() {
    // str_width_in(s, m) must equal the sum of grapheme_width_in over
    // the ANSI-stripped grapheme segmentation of s. We cannot call
    // `strip_ansi_escapes` (private), so pick strings without ESC.
    let samples: &[&str] = &[
        "a日α",
        "αβγ",
        "日本語αβ",
        "box ─ drawing α",
        "e\u{0301} naïve",
        "",
        "pure-ascii-string",
    ];
    for s in samples {
        for m in [WidthMode::Latin, WidthMode::EastAsian] {
            let whole = str_width_in(s, m);
            let summed: usize = s.graphemes(true).map(|g| grapheme_width_in(g, m)).sum();
            assert_eq!(
                whole, summed,
                "uni9-widthmode-19 s={:?} mode={:?}",
                s, m,
            );
        }
    }
}

#[test]
fn uni9_widthmode_20_grapheme_width_wrapper_matches_default() {
    for g in ["a", "日", "α", "e\u{0301}", "é", "\u{001B}", ""] {
        assert_eq!(
            grapheme_width(g),
            grapheme_width_in(g, WidthMode::default()),
            "uni9-widthmode-20 g={:?}",
            g,
        );
    }
}
