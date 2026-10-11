//! UNI-1 NFD / NFKC / NFKD + dispatcher conformance.
//!
//! Companion suite to `tests/nfc_conformance.rs`. The NFC path is
//! covered there (30 vectors stamped `r221m1-nfc-NN`); here we add
//! known-value vectors for the other three forms, dispatcher parity,
//! ASCII fast-path confirmation, and per-form idempotence.
//!
//! Fingerprint discipline matches `r221m1-nfc-NN`: every assertion
//! carries a `uni1-norm-NN` tag so a failing binary can be correlated
//! to the UNI-1 ticket line in the dispatch map.

use paideia_as_unicode::{
    Form, is_form, is_nfc, is_nfd, is_nfkc, is_nfkd, nfc_normalize, nfd_normalize, nfkc_normalize,
    nfkd_normalize, normalize,
};

// --------------------------------------------------------------------
// Known-value vectors
// --------------------------------------------------------------------

#[test]
fn uni1_norm_01_nfd_decomposes_eacute() {
    // U+00E9 (é precomposed) → U+0065 U+0301 under NFD.
    assert_eq!(nfd_normalize("\u{00E9}"), "e\u{0301}", "uni1-norm-01");
}

#[test]
fn uni1_norm_02_nfd_decomposes_already_decomposed_is_identity() {
    assert_eq!(nfd_normalize("e\u{0301}"), "e\u{0301}", "uni1-norm-02");
}

#[test]
fn uni1_norm_03_nfd_hangul_decomposes() {
    // U+AC00 (HANGUL SYLLABLE GA) decomposes into U+1100 + U+1161.
    assert_eq!(nfd_normalize("\u{AC00}"), "\u{1100}\u{1161}", "uni1-norm-03");
}

#[test]
fn uni1_norm_04_nfd_round_trips_through_nfc() {
    // UAX#15 §1.4: NFC(NFD(x)) == NFC(x) for any x.
    let xs = [
        "é",
        "\u{AC00}",
        "a\u{0304}\u{0323}",
        "ls 中文",
        "\u{212B}",
    ];
    for x in xs {
        let via_nfd = nfd_normalize(x);
        assert_eq!(nfc_normalize(&via_nfd), nfc_normalize(x), "uni1-norm-04:{x}");
    }
}

#[test]
fn uni1_norm_05_nfkc_collapses_cjk_compat_ideograph() {
    // U+F900 (CJK COMPATIBILITY IDEOGRAPH-F900) has a compatibility
    // decomposition to U+8C48, which NFKC then recomposes (no further
    // decomposition — U+8C48 has none). Expected: U+8C48.
    assert_eq!(nfkc_normalize("\u{F900}"), "\u{8C48}", "uni1-norm-05");
}

#[test]
fn uni1_norm_06_nfkc_collapses_halfwidth_katakana() {
    // U+FF76 (HALFWIDTH KATAKANA LETTER KA) → U+30AB (KATAKANA LETTER KA)
    // via compatibility decomposition. NFKC emits the composed form.
    assert_eq!(nfkc_normalize("\u{FF76}"), "\u{30AB}", "uni1-norm-06");
}

#[test]
fn uni1_norm_07_nfkc_collapses_ligature_fi() {
    // U+FB01 (LATIN SMALL LIGATURE FI) → "fi" under NFKC. Classic
    // compatibility-flattening test.
    assert_eq!(nfkc_normalize("\u{FB01}"), "fi", "uni1-norm-07");
}

#[test]
fn uni1_norm_08_nfkc_pure_nfc_input_unchanged() {
    // é has NFKC == NFC (no compatibility decomposition applies).
    assert_eq!(nfkc_normalize("\u{00E9}"), "\u{00E9}", "uni1-norm-08");
}

#[test]
fn uni1_norm_09_nfkd_decomposes_ligature_fi() {
    // U+FB01 → "fi" under NFKD as well (NFKD = compat-decomp only).
    assert_eq!(nfkd_normalize("\u{FB01}"), "fi", "uni1-norm-09");
}

#[test]
fn uni1_norm_10_nfkd_halfwidth_to_decomposed_katakana() {
    // U+FF76 → U+30AB directly (U+30AB itself has no decomposition).
    assert_eq!(nfkd_normalize("\u{FF76}"), "\u{30AB}", "uni1-norm-10");
}

#[test]
fn uni1_norm_11_nfkd_decomposes_eacute() {
    // NFKD subsumes NFD on canonical cases: é → e + combining acute.
    assert_eq!(nfkd_normalize("\u{00E9}"), "e\u{0301}", "uni1-norm-11");
}

#[test]
fn uni1_norm_12_predicates_recognize_decomposed_eacute() {
    assert!(is_nfd("e\u{0301}"), "uni1-norm-12a");
    assert!(!is_nfc("e\u{0301}"), "uni1-norm-12b");
    assert!(is_nfkd("e\u{0301}"), "uni1-norm-12c");
    assert!(!is_nfkc("e\u{0301}"), "uni1-norm-12d");
}

#[test]
fn uni1_norm_13_predicates_recognize_precomposed_eacute() {
    assert!(is_nfc("\u{00E9}"), "uni1-norm-13a");
    assert!(!is_nfd("\u{00E9}"), "uni1-norm-13b");
    assert!(is_nfkc("\u{00E9}"), "uni1-norm-13c");
    assert!(!is_nfkd("\u{00E9}"), "uni1-norm-13d");
}

#[test]
fn uni1_norm_14_predicates_reject_compat_form_under_nfkc() {
    // U+FB01 (fi ligature) is NFC-stable but not NFKC-stable.
    assert!(is_nfc("\u{FB01}"), "uni1-norm-14a");
    assert!(is_nfd("\u{FB01}"), "uni1-norm-14b");
    assert!(!is_nfkc("\u{FB01}"), "uni1-norm-14c");
    assert!(!is_nfkd("\u{FB01}"), "uni1-norm-14d");
}

// --------------------------------------------------------------------
// ASCII fast-path
// --------------------------------------------------------------------

#[test]
fn uni1_norm_15_ascii_fast_path_all_forms() {
    for form in [Form::Nfc, Form::Nfd, Form::Nfkc, Form::Nfkd] {
        assert_eq!(normalize("hello", form), "hello", "uni1-norm-15:{form:?}");
        assert!(is_form("hello", form), "uni1-norm-15-is:{form:?}");
    }
}

#[test]
fn uni1_norm_16_ascii_empty_string_all_forms() {
    for form in [Form::Nfc, Form::Nfd, Form::Nfkc, Form::Nfkd] {
        assert_eq!(normalize("", form), "", "uni1-norm-16:{form:?}");
        assert!(is_form("", form), "uni1-norm-16-is:{form:?}");
    }
}

#[test]
fn uni1_norm_17_ascii_long_command_line_all_forms() {
    // 4 KiB of ASCII — must bypass UCD tables entirely.
    let s: String = "ls -la /usr/local/bin ".repeat(200);
    for form in [Form::Nfc, Form::Nfd, Form::Nfkc, Form::Nfkd] {
        assert_eq!(normalize(&s, form), s, "uni1-norm-17:{form:?}");
    }
}

// --------------------------------------------------------------------
// Dispatcher parity
// --------------------------------------------------------------------

/// A 50-element corpus mixing ASCII, Latin-1, CJK, Hangul, astral, and
/// combining-mark cases. Used by the dispatcher-parity and
/// idempotence tests below.
fn parity_corpus() -> Vec<&'static str> {
    vec![
        "",
        "a",
        "ls",
        "cd /tmp",
        "find . -name '*.pdx'",
        "\u{00E9}",             // é precomposed
        "e\u{0301}",            // é decomposed
        "\u{00F1}",             // ñ
        "n\u{0303}",            // ñ decomposed
        "\u{00F6}",             // ö
        "o\u{0308}",            // ö decomposed
        "\u{00E7}",             // ç
        "c\u{0327}",            // ç decomposed
        "\u{00E0}",             // à
        "a\u{0300}",            // à decomposed
        "\u{1EA1}",             // ạ (a with dot below)
        "a\u{0323}",            // ạ decomposed
        "\u{1EA5}",             // ấ
        "a\u{0302}\u{0301}",    // ấ decomposed
        "\u{1EDF}",             // ở
        "o\u{031B}\u{0309}",    // ở decomposed
        "\u{AC00}",             // 가 Hangul
        "\u{1100}\u{1161}",     // 가 decomposed
        "\u{AC01}",             // 각 Hangul
        "\u{1100}\u{1161}\u{11A8}", // 각 decomposed
        "\u{4E2D}",             // 中
        "\u{4E2D}\u{6587}",     // 中文
        "ls 中文",
        "\u{0644}\u{0627}",     // لا Arabic
        "\u{0915}\u{094D}\u{0937}\u{093F}", // Devanagari
        "\u{05E9}\u{05DC}\u{05D5}\u{05DD}", // Hebrew shalom
        "\u{03B1}",             // α
        "\u{03B1}\u{0301}",     // α + acute
        "\u{03AC}",             // ά
        "\u{1D11E}",            // 𝄞 astral
        "\u{1F600}",            // 😀
        "\u{1F468}\u{200D}\u{1F469}", // ZWJ pair
        "\u{1F44D}\u{1F3FD}",   // thumbs-up + skin tone
        "\u{1F1FA}\u{1F1F8}",   // 🇺🇸
        "\u{212B}",             // ANGSTROM SIGN
        "\u{00C5}",             // Å
        "\u{F900}",             // CJK compat ideograph
        "\u{FF76}",             // halfwidth katakana KA
        "\u{FB01}",             // fi ligature
        "\u{FB02}",             // fl ligature
        "a\u{0304}\u{0323}",    // multi-combining
        "\u{0301}leading",      // defective combining at start
        "hello world",
        "mixed é and ö",
        "a\u{0300}b\u{0301}c\u{0303}", // chain
    ]
}

#[test]
fn uni1_norm_18_dispatcher_parity_nfc() {
    for (i, x) in parity_corpus().iter().enumerate() {
        assert_eq!(
            normalize(x, Form::Nfc),
            nfc_normalize(x),
            "uni1-norm-18[{i}]"
        );
    }
}

#[test]
fn uni1_norm_19_dispatcher_parity_nfd() {
    for (i, x) in parity_corpus().iter().enumerate() {
        assert_eq!(
            normalize(x, Form::Nfd),
            nfd_normalize(x),
            "uni1-norm-19[{i}]"
        );
    }
}

#[test]
fn uni1_norm_20_dispatcher_parity_nfkc() {
    for (i, x) in parity_corpus().iter().enumerate() {
        assert_eq!(
            normalize(x, Form::Nfkc),
            nfkc_normalize(x),
            "uni1-norm-20[{i}]"
        );
    }
}

#[test]
fn uni1_norm_21_dispatcher_parity_nfkd() {
    for (i, x) in parity_corpus().iter().enumerate() {
        assert_eq!(
            normalize(x, Form::Nfkd),
            nfkd_normalize(x),
            "uni1-norm-21[{i}]"
        );
    }
}

#[test]
fn uni1_norm_22_is_form_parity() {
    for (i, x) in parity_corpus().iter().enumerate() {
        assert_eq!(is_form(x, Form::Nfc), is_nfc(x), "uni1-norm-22a[{i}]");
        assert_eq!(is_form(x, Form::Nfd), is_nfd(x), "uni1-norm-22b[{i}]");
        assert_eq!(is_form(x, Form::Nfkc), is_nfkc(x), "uni1-norm-22c[{i}]");
        assert_eq!(is_form(x, Form::Nfkd), is_nfkd(x), "uni1-norm-22d[{i}]");
    }
}

// --------------------------------------------------------------------
// Idempotence (UAX#15 §1.4)
// --------------------------------------------------------------------

/// 20-element idempotence corpus (a shuffle-and-trim of the parity set
/// biased toward combining-mark cases, where re-normalization is most
/// likely to misbehave).
fn idempotence_corpus() -> Vec<&'static str> {
    vec![
        "",
        "a",
        "\u{00E9}",
        "e\u{0301}",
        "\u{00F1}",
        "n\u{0303}",
        "\u{AC00}",
        "\u{1100}\u{1161}",
        "a\u{0302}\u{0301}",
        "o\u{031B}\u{0309}",
        "a\u{0304}\u{0323}",
        "\u{F900}",
        "\u{FF76}",
        "\u{FB01}",
        "\u{212B}",
        "\u{03B1}\u{0301}",
        "\u{1D11E}",
        "ls 中文",
        "mixed é and ö",
        "\u{0301}leading",
    ]
}

#[test]
fn uni1_norm_23_idempotence_all_forms() {
    for form in [Form::Nfc, Form::Nfd, Form::Nfkc, Form::Nfkd] {
        for (i, x) in idempotence_corpus().iter().enumerate() {
            let once = normalize(x, form);
            let twice = normalize(&once, form);
            assert_eq!(
                once, twice,
                "uni1-norm-23:{form:?}[{i}] not idempotent"
            );
        }
    }
}

// --------------------------------------------------------------------
// Form enum basics
// --------------------------------------------------------------------

#[test]
fn uni1_norm_24_form_default_is_nfc() {
    // SH-D9.4 contract: default form at the IPC boundary is NFC.
    assert_eq!(Form::default(), Form::Nfc, "uni1-norm-24");
}

#[test]
fn uni1_norm_25_form_hash_eq_copy() {
    use std::collections::HashSet;
    let mut set: HashSet<Form> = HashSet::new();
    for form in [Form::Nfc, Form::Nfd, Form::Nfkc, Form::Nfkd] {
        set.insert(form);
    }
    assert_eq!(set.len(), 4, "uni1-norm-25");
    // Copy-ness check: use after insert.
    let f = Form::Nfd;
    let _ = f;
    let _ = f;
}
