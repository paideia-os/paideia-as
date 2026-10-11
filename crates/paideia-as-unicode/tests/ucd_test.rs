//! UNI-5 (paideia-as#1592) — UCD property adapter conformance.
//!
//! The three queries ([`general_category`], [`script`], [`block`]) are
//! table-driven lookups backed by the Servo `unicode-properties`,
//! `unicode-script`, and `unicode-blocks` crates. This file pins the
//! externally-visible mapping with fingerprinted `uni5-ucd-NN` tests
//! so the debugger can correlate regressions against the UNI-5
//! acceptance line item per `feedback_workerbee_verify_claims.md`.
//!
//! Each test is tagged with a fingerprint matching the pattern
//! `uni5-ucd-<two-digit index>`; the full catalogue below covers:
//!
//! * General_Category: the five letter cases plus Nd / Zs / Mn / Po
//!   / Sc / Sm / Pd / Ps / Pe / Cc / Cf.
//! * Script: Latin / Greek / Hiragana / Common / Inherited +
//!   Han / Hangul / Arabic / Hebrew / Cyrillic / Devanagari.
//! * Block: BasicLatin / Latin1Supplement / GreekAndCoptic / Hiragana
//!   / CjkUnifiedIdeographs / HangulSyllables / Arabic / Cyrillic.
//! * Group predicates: full 10-char matrix across the seven groups
//!   (`is_letter`, `is_mark`, `is_number`, `is_punctuation`,
//!   `is_symbol`, `is_separator`, `is_other`).
//! * Mutual exclusivity of the seven group predicates (invariant).

use paideia_as_unicode::{
    Block, GeneralCategory, Script, block, general_category, is_letter, is_mark, is_number,
    is_other, is_punctuation, is_separator, is_symbol, script,
};

// ---------------------------------------------------------------------
// 1. General_Category — the 30-variant short-form enum.
// ---------------------------------------------------------------------

#[test]
fn uni5_ucd_01_general_category_lowercase_latin() {
    // UCD DerivedGeneralCategory row:
    //   0061..007A  ; Ll # L&  [26] LATIN SMALL LETTER A..LATIN SMALL LETTER Z
    assert_eq!(general_category('a'), GeneralCategory::Ll);
    assert_eq!(general_category('z'), GeneralCategory::Ll);
}

#[test]
fn uni5_ucd_02_general_category_uppercase_latin() {
    // 0041..005A  ; Lu
    assert_eq!(general_category('A'), GeneralCategory::Lu);
    assert_eq!(general_category('Z'), GeneralCategory::Lu);
}

#[test]
fn uni5_ucd_03_general_category_decimal_digit() {
    // 0030..0039  ; Nd
    assert_eq!(general_category('0'), GeneralCategory::Nd);
    assert_eq!(general_category('5'), GeneralCategory::Nd);
    assert_eq!(general_category('9'), GeneralCategory::Nd);
}

#[test]
fn uni5_ucd_04_general_category_space_separator() {
    // 0020  ; Zs # Zs       SPACE
    assert_eq!(general_category(' '), GeneralCategory::Zs);
    // 00A0  ; Zs # Zs       NO-BREAK SPACE
    assert_eq!(general_category('\u{00A0}'), GeneralCategory::Zs);
}

#[test]
fn uni5_ucd_05_general_category_nonspacing_mark() {
    // 0300  ; Mn # Mn       COMBINING GRAVE ACCENT
    assert_eq!(general_category('\u{0300}'), GeneralCategory::Mn);
    // 0301  ; Mn # Mn       COMBINING ACUTE ACCENT
    assert_eq!(general_category('\u{0301}'), GeneralCategory::Mn);
}

#[test]
fn uni5_ucd_06_general_category_titlecase_letter() {
    // 01C5  ; Lt # L&       LATIN CAPITAL LETTER D WITH SMALL LETTER Z WITH CARON
    assert_eq!(general_category('\u{01C5}'), GeneralCategory::Lt);
    // 01C8  ; Lt
    assert_eq!(general_category('\u{01C8}'), GeneralCategory::Lt);
}

#[test]
fn uni5_ucd_07_general_category_other_letter() {
    // CJK Unified Ideograph: 4E00  ; Lo
    assert_eq!(general_category('\u{4E00}'), GeneralCategory::Lo);
    // Hiragana A: 3042  ; Lo
    assert_eq!(general_category('あ'), GeneralCategory::Lo);
}

#[test]
fn uni5_ucd_08_general_category_currency_symbol() {
    // 0024  ; Sc  DOLLAR SIGN
    assert_eq!(general_category('$'), GeneralCategory::Sc);
    // 00A3  ; Sc  POUND SIGN
    assert_eq!(general_category('£'), GeneralCategory::Sc);
}

#[test]
fn uni5_ucd_09_general_category_math_symbol() {
    // 002B  ; Sm  PLUS SIGN
    assert_eq!(general_category('+'), GeneralCategory::Sm);
    // 00D7  ; Sm  MULTIPLICATION SIGN
    assert_eq!(general_category('×'), GeneralCategory::Sm);
}

#[test]
fn uni5_ucd_10_general_category_punctuation_variants() {
    // 002D  ; Pd  HYPHEN-MINUS
    assert_eq!(general_category('-'), GeneralCategory::Pd);
    // 0028  ; Ps  LEFT PARENTHESIS
    assert_eq!(general_category('('), GeneralCategory::Ps);
    // 0029  ; Pe  RIGHT PARENTHESIS
    assert_eq!(general_category(')'), GeneralCategory::Pe);
    // 002E  ; Po  FULL STOP
    assert_eq!(general_category('.'), GeneralCategory::Po);
}

#[test]
fn uni5_ucd_11_general_category_control_and_format() {
    // 0000..001F ; Cc
    assert_eq!(general_category('\u{0000}'), GeneralCategory::Cc);
    assert_eq!(general_category('\n'), GeneralCategory::Cc);
    // 00AD  ; Cf  SOFT HYPHEN
    assert_eq!(general_category('\u{00AD}'), GeneralCategory::Cf);
}

// ---------------------------------------------------------------------
// 2. Script — re-exported from `unicode-script`.
// ---------------------------------------------------------------------

#[test]
fn uni5_ucd_12_script_latin_greek_hiragana() {
    // Scripts.txt rows:
    //   0061..007A  ; Latin
    //   03B1..03C9  ; Greek
    //   3042..3096  ; Hiragana
    assert_eq!(script('a'), Script::Latin);
    assert_eq!(script('α'), Script::Greek);
    assert_eq!(script('あ'), Script::Hiragana);
}

#[test]
fn uni5_ucd_13_script_common_and_inherited() {
    // Scripts.txt:
    //   3002         ; Common   # Po  IDEOGRAPHIC FULL STOP
    //   0300..036F   ; Inherited (combining marks)
    assert_eq!(script('。'), Script::Common);
    assert_eq!(script('\u{0300}'), Script::Inherited);
    // Also: SPACE is Common.
    assert_eq!(script(' '), Script::Common);
}

#[test]
fn uni5_ucd_14_script_cjk_and_hangul_and_arabic() {
    // 4E00  ; Han
    // AC00  ; Hangul
    // 0627  ; Arabic (ALEF)
    assert_eq!(script('\u{4E00}'), Script::Han);
    assert_eq!(script('\u{AC00}'), Script::Hangul);
    assert_eq!(script('ا'), Script::Arabic);
}

#[test]
fn uni5_ucd_15_script_cyrillic_hebrew_devanagari() {
    // 0410  ; Cyrillic (CAPITAL A)
    // 05D0  ; Hebrew (ALEF)
    // 0905  ; Devanagari (A)
    assert_eq!(script('\u{0410}'), Script::Cyrillic);
    assert_eq!(script('\u{05D0}'), Script::Hebrew);
    assert_eq!(script('\u{0905}'), Script::Devanagari);
}

// ---------------------------------------------------------------------
// 3. Block — 344-variant hand-rolled enum.
// ---------------------------------------------------------------------

#[test]
fn uni5_ucd_16_block_basic_latin() {
    // 0000..007F
    assert_eq!(block('a'), Block::BasicLatin);
    assert_eq!(block('A'), Block::BasicLatin);
    assert_eq!(block('\u{0000}'), Block::BasicLatin);
    assert_eq!(block('\u{007F}'), Block::BasicLatin);
}

#[test]
fn uni5_ucd_17_block_latin1_supplement() {
    // 0080..00FF
    assert_eq!(block('à'), Block::Latin1Supplement);
    assert_eq!(block('ß'), Block::Latin1Supplement);
    assert_eq!(block('\u{00FF}'), Block::Latin1Supplement);
}

#[test]
fn uni5_ucd_18_block_greek_and_hiragana() {
    // 0370..03FF  Greek and Coptic
    // 3040..309F  Hiragana
    assert_eq!(block('α'), Block::GreekAndCoptic);
    assert_eq!(block('Α'), Block::GreekAndCoptic);
    assert_eq!(block('あ'), Block::Hiragana);
    assert_eq!(block('ゖ'), Block::Hiragana);
}

#[test]
fn uni5_ucd_19_block_cjk_hangul_arabic_cyrillic() {
    // 4E00..9FFF  CJK Unified Ideographs
    // AC00..D7AF  Hangul Syllables
    // 0600..06FF  Arabic
    // 0400..04FF  Cyrillic
    assert_eq!(block('\u{4E00}'), Block::CjkUnifiedIdeographs);
    assert_eq!(block('\u{9FFF}'), Block::CjkUnifiedIdeographs);
    assert_eq!(block('\u{AC00}'), Block::HangulSyllables);
    assert_eq!(block('ا'), Block::Arabic);
    assert_eq!(block('\u{0410}'), Block::Cyrillic);
}

// ---------------------------------------------------------------------
// 4. Convenience group predicates — 10-character coverage matrix.
// ---------------------------------------------------------------------

#[test]
fn uni5_ucd_20_predicates_letter_matrix() {
    // Letters: 'A' Lu, 'a' Ll, 'あ' Lo, '\u{01C5}' Lt, 'ʰ' Lm.
    for c in ['A', 'a', 'あ', '\u{01C5}', '\u{02B0}'] {
        assert!(is_letter(c), "is_letter({:?}) must be true", c);
    }
    // Non-letters: digit, mark, space, dash, dollar, control.
    for c in ['5', '\u{0300}', ' ', '-', '$', '\n'] {
        assert!(!is_letter(c), "is_letter({:?}) must be false", c);
    }
}

#[test]
fn uni5_ucd_21_predicates_mark_matrix() {
    // Marks: Mn, Mc, Me samples.
    //   0300 Mn, 0903 Mc (DEVANAGARI SIGN VISARGA), 0488 Me (COMBINING CYRILLIC HUNDRED THOUSANDS SIGN)
    for c in ['\u{0300}', '\u{0903}', '\u{0488}'] {
        assert!(is_mark(c), "is_mark({:?}) must be true", c);
    }
    for c in ['a', '5', ' ', '-', '$'] {
        assert!(!is_mark(c), "is_mark({:?}) must be false", c);
    }
}

#[test]
fn uni5_ucd_22_predicates_number_matrix() {
    // Numbers: Nd '5', Nl 'Ⅷ' (U+2167 ROMAN NUMERAL EIGHT), No '½' (U+00BD VULGAR FRACTION ONE HALF).
    for c in ['5', '\u{2167}', '\u{00BD}'] {
        assert!(is_number(c), "is_number({:?}) must be true", c);
    }
    for c in ['a', '\u{0300}', ' ', '-'] {
        assert!(!is_number(c), "is_number({:?}) must be false", c);
    }
}

#[test]
fn uni5_ucd_23_predicates_punctuation_matrix() {
    // Punctuation: Pd '-', Ps '(', Pe ')', Po '.', Pi '«', Pf '»', Pc '_'.
    for c in ['-', '(', ')', '.', '«', '»', '_'] {
        assert!(is_punctuation(c), "is_punctuation({:?}) must be true", c);
    }
    for c in ['a', '5', '$', '+'] {
        assert!(!is_punctuation(c), "is_punctuation({:?}) must be false", c);
    }
}

#[test]
fn uni5_ucd_24_predicates_symbol_matrix() {
    // Symbol: Sm '+', Sc '$', Sk '^' (U+005E CIRCUMFLEX ACCENT is Sk), So '©'.
    for c in ['+', '$', '^', '©'] {
        assert!(is_symbol(c), "is_symbol({:?}) must be true", c);
    }
    for c in ['a', '5', '-', ' '] {
        assert!(!is_symbol(c), "is_symbol({:?}) must be false", c);
    }
}

#[test]
fn uni5_ucd_25_predicates_separator_matrix() {
    // Separators: Zs ' ', Zl U+2028, Zp U+2029.
    for c in [' ', '\u{2028}', '\u{2029}'] {
        assert!(is_separator(c), "is_separator({:?}) must be true", c);
    }
    for c in ['a', '5', '-', '\n'] {
        assert!(!is_separator(c), "is_separator({:?}) must be false", c);
    }
}

#[test]
fn uni5_ucd_26_predicates_other_matrix() {
    // Other: Cc '\0', Cf U+00AD, Cs U+D800 (surrogate — not a valid char via
    // `'\u{...}'` literal), Co U+E000 (private use), Cn U+FDD0 (noncharacter).
    for c in ['\u{0000}', '\u{00AD}', '\u{E000}', '\u{FDD0}'] {
        assert!(is_other(c), "is_other({:?}) must be true", c);
    }
    for c in ['a', '5', '-', ' ', '+'] {
        assert!(!is_other(c), "is_other({:?}) must be false", c);
    }
}

#[test]
fn uni5_ucd_27_predicate_groups_are_disjoint() {
    // Invariant: for every char, exactly one of the seven group
    // predicates returns true. This is the formal top-level partition
    // the Unicode Standard §4.5 defines.
    let samples: &[char] = &[
        'A', 'a', 'あ', '\u{01C5}', '\u{02B0}', // Letter
        '\u{0300}', '\u{0903}', '\u{0488}', // Mark
        '5', '\u{2167}', '\u{00BD}',    // Number
        '-', '(', ')', '.', '«', '»', '_', // Punctuation
        '+', '$', '^', '©', // Symbol
        ' ', '\u{2028}', '\u{2029}', // Separator
        '\u{0000}', '\u{00AD}', '\u{E000}', '\u{FDD0}', // Other
    ];
    for &c in samples {
        let flags = [
            is_letter(c),
            is_mark(c),
            is_number(c),
            is_punctuation(c),
            is_symbol(c),
            is_separator(c),
            is_other(c),
        ];
        let true_count = flags.iter().filter(|b| **b).count();
        assert_eq!(
            true_count, 1,
            "exactly one group predicate must match for {:?}, got {:?}",
            c, flags
        );
    }
}

#[test]
fn uni5_ucd_28_block_unassigned_in_gap() {
    // U+2FFF is unassigned in UCD 15.x (in a gap just before CJK Symbols
    // and Punctuation at U+3000). The upstream `find_unicode_block`
    // returns None for codepoints in gaps between blocks.
    //
    // Note: UCD 17 may have populated U+2FFF; this test asserts that if
    // the character is in a gap, `block()` returns `Block::Unassigned`,
    // never a wrong block. We pick a codepoint well inside a known
    // historical gap: U+0870 is Arabic Extended-B (if present in UCD
    // 17 — unicode-blocks 0.1.10 does list it). Use a definite gap:
    // U+2FFE / U+2FFF — gap between Ideographic Description Characters
    // (U+2FF0..U+2FFF per UCD 17 — note: 17.0 added U+2FFC..U+2FFF to
    // IDC, so we switch to a stable gap). U+1CFF0 — mid-gap in
    // supplementary plane region we have not populated: safe.
    //
    // Actual chosen gap: U+10FFFD lies in Supplementary Private Use
    // Area-B (an assigned block), not a gap. We use U+2E80..U+2EFF
    // boundary at U+2FEF: U+2FEF is in Ideographic Description Chars.
    //
    // Simpler: use U+0DF5 — Sinhala block runs 0D80..0DFF but 0DF5 is
    // unassigned WITHIN Sinhala (so `find_unicode_block` DOES return
    // Sinhala). That is not a gap. We need between-block gaps.
    //
    // Confirmed between-block gap in Blocks.txt 17.0: between Arabic
    // Supplement (0750..077F) and Thaana (0780..07BF) — no gap, they
    // touch. Between Arabic Extended-A (08A0..08FF) and Devanagari
    // (0900..097F) — no gap, they touch.
    //
    // A real cross-block gap: U+0860..U+086F is "Syriac Supplement".
    // U+0870..U+089F is "Arabic Extended-B". No gap. All of 0..FFFF is
    // nearly contiguous. Pick a SMP gap: between Variation Selectors
    // Supplement (E0100..E01EF) and Supplementary Private Use Area-A
    // (F0000..). Codepoint U+E0200 falls in that gap.
    assert_eq!(block('\u{E0200}'), Block::Unassigned);
}

#[test]
fn uni5_ucd_29_block_private_use_and_specials() {
    // PUA: E000..F8FF, Specials: FFF0..FFFF.
    assert_eq!(block('\u{E000}'), Block::PrivateUseArea);
    assert_eq!(block('\u{F8FF}'), Block::PrivateUseArea);
    assert_eq!(block('\u{FFFD}'), Block::Specials);
}

#[test]
fn uni5_ucd_30_block_supplementary_private_use() {
    // Supplementary PUA-A: F0000..FFFFF.
    // Supplementary PUA-B: 100000..10FFFF.
    assert_eq!(block('\u{F0000}'), Block::SupplementaryPrivateUseAreaA);
    assert_eq!(block('\u{100000}'), Block::SupplementaryPrivateUseAreaB);
    assert_eq!(block('\u{10FFFF}'), Block::SupplementaryPrivateUseAreaB);
}
