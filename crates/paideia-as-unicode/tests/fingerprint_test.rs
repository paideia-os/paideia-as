//! UNI-6 (paideia-as#1593) — fingerprint-golden conformance tests.
//!
//! Each `#[test]` here reads one golden file under
//! `tests/fingerprints/<fn>.golden`, runs the function under test on
//! every recorded input, and verifies the live output matches the
//! recorded output. The golden-file format and the per-function
//! serialization are documented in `src/fingerprint.rs` and in
//! `design/toolchain/paideia-as-unicode-spec.md` §5.
//!
//! # Regen flow
//!
//! Setting the env var `PAIDEIA_UNICODE_REGEN_FINGERPRINTS=1` makes
//! every test rewrite its golden file with freshly computed outputs
//! instead of verifying — the resulting git diff is the review
//! artifact. Verifying is the default; regen is strictly opt-in so a
//! CI run that accidentally inherits the var cannot silently stamp
//! new goldens.
//!
//! ```text
//! PAIDEIA_UNICODE_REGEN_FINGERPRINTS=1 \
//!     cargo test -p paideia-as-unicode --test fingerprint_test
//! ```
//!
//! # Why one `#[test]` per golden
//!
//! A single mega-test would collapse 24 failure modes into one
//! message and defeat `cargo test --test fingerprint_test
//! golden_nfc_normalize`-style selection, which the UCD upgrade
//! workflow uses to rerun one golden at a time while a reviewer walks
//! the diff. The common dispatch below ([`run_golden`]) keeps each
//! test one line of boilerplate.

use paideia_as_unicode::fingerprint::{
    FingerprintRecord, maybe_regen_fingerprints, read_fingerprint_golden,
    write_fingerprint_golden_records,
};
use paideia_as_unicode::{
    CaseFoldLocale, Utf8Decoder, WidthMode, block, eq_ignore_case_unicode, fold, fold_default,
    general_category, grapheme_boundaries, grapheme_count, is_nfc, is_nfd, is_nfkc, is_nfkd,
    line_break_opportunities, lowercase, nfc_normalize, nfd_normalize, nfkc_normalize,
    nfkd_normalize, script, str_width_in, uppercase, width_in, wrap_at_width,
};

/// Common dispatch for every golden-file test.
///
/// Reads `name.golden` into [`FingerprintRecord`]s, applies `compute`
/// to each input to obtain the live output, and either (a) overwrites
/// the golden file with the live outputs if
/// `PAIDEIA_UNICODE_REGEN_FINGERPRINTS=1` is set (regen mode), or (b)
/// verifies every live output against the recorded `expected`
/// (default mode).
///
/// Collects every mismatch before panicking so a single failing run
/// surfaces all broken rows at once rather than only the first.
fn run_golden<F>(name: &str, mut compute: F)
where
    F: FnMut(&str) -> String,
{
    let records = read_fingerprint_golden(name)
        .unwrap_or_else(|e| panic!("read golden `{}`: {}", name, e));
    assert!(
        !records.is_empty(),
        "golden `{}` has no records — refusing to pass vacuously",
        name,
    );
    if maybe_regen_fingerprints() {
        let regen: Vec<FingerprintRecord> = records
            .iter()
            .map(|r| FingerprintRecord {
                tag: r.tag.clone(),
                input: r.input.clone(),
                expected: compute(&r.input),
            })
            .collect();
        write_fingerprint_golden_records(name, &regen)
            .unwrap_or_else(|e| panic!("regen golden `{}`: {}", name, e));
        return;
    }
    let mut failures = Vec::new();
    for r in &records {
        let actual = compute(&r.input);
        if actual != r.expected {
            failures.push(format!(
                "  tag `{}`:\n    input    = {:?}\n    expected = {:?}\n    actual   = {:?}",
                r.tag, r.input, r.expected, actual,
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatch(es) in golden `{}`:\n{}",
        failures.len(),
        name,
        failures.join("\n"),
    );
}

/// Extract the first `char` of `s`, defaulting to `\0` for an empty
/// input. Shared by every single-character golden
/// (general_category / script / block / width_*).
#[inline]
fn first_char(s: &str) -> char {
    s.chars().next().unwrap_or('\0')
}

// ---------------------------------------------------------------------
// Normalization: nfc / nfd / nfkc / nfkd
// ---------------------------------------------------------------------

#[test]
fn golden_nfc_normalize() {
    run_golden("nfc_normalize", nfc_normalize);
}

#[test]
fn golden_nfd_normalize() {
    run_golden("nfd_normalize", nfd_normalize);
}

#[test]
fn golden_nfkc_normalize() {
    run_golden("nfkc_normalize", nfkc_normalize);
}

#[test]
fn golden_nfkd_normalize() {
    run_golden("nfkd_normalize", nfkd_normalize);
}

// ---------------------------------------------------------------------
// Normalization predicates: output is the literal word "true" / "false"
// ---------------------------------------------------------------------

#[inline]
fn bool_str(b: bool) -> String {
    if b { "true".into() } else { "false".into() }
}

#[test]
fn golden_is_nfc() {
    run_golden("is_nfc", |s| bool_str(is_nfc(s)));
}

#[test]
fn golden_is_nfd() {
    run_golden("is_nfd", |s| bool_str(is_nfd(s)));
}

#[test]
fn golden_is_nfkc() {
    run_golden("is_nfkc", |s| bool_str(is_nfkc(s)));
}

#[test]
fn golden_is_nfkd() {
    run_golden("is_nfkd", |s| bool_str(is_nfkd(s)));
}

// ---------------------------------------------------------------------
// Case folding + equality
// ---------------------------------------------------------------------

#[test]
fn golden_fold_default() {
    run_golden("fold_default", fold_default);
}

#[test]
fn golden_fold_turkic() {
    run_golden("fold_turkic", |s| fold(s, CaseFoldLocale::Turkic));
}

/// `eq_ignore_case` encodes two sides in one input: `"<a>\x1f<b>"`.
/// The U+001F unit-separator keeps the pair representable in a single
/// hex column without a second tab in the golden-file format.
#[test]
fn golden_eq_ignore_case() {
    run_golden("eq_ignore_case", |s| {
        let mut parts = s.splitn(2, '\u{001f}');
        let a = parts.next().unwrap_or("");
        let b = parts.next().unwrap_or("");
        bool_str(eq_ignore_case_unicode(a, b, CaseFoldLocale::Default))
    });
}

// ---------------------------------------------------------------------
// Case mapping
// ---------------------------------------------------------------------

#[test]
fn golden_uppercase() {
    run_golden("uppercase", uppercase);
}

#[test]
fn golden_lowercase() {
    run_golden("lowercase", lowercase);
}

// ---------------------------------------------------------------------
// Line-break segmentation (UAX #14)
// ---------------------------------------------------------------------

/// Serialize `line_break_opportunities(s)` as
/// `"<off>:<Kind>,<off>:<Kind>,..."` where `<Kind>` is the
/// `Debug` form of [`LineBreak`] ("Mandatory" / "Allowed").
#[test]
fn golden_line_break_opportunities() {
    run_golden("line_break_opportunities", |s| {
        line_break_opportunities(s)
            .iter()
            .map(|o| format!("{}:{:?}", o.byte_offset, o.kind))
            .collect::<Vec<_>>()
            .join(",")
    });
}

/// `wrap_at_width` encodes `(width, text)` as `"<width>\x1f<text>"`.
/// Output is the wrapped segments joined by `\n`.
#[test]
fn golden_wrap_at_width() {
    run_golden("wrap_at_width", |s| {
        let mut parts = s.splitn(2, '\u{001f}');
        let w: usize = parts
            .next()
            .unwrap_or("0")
            .parse()
            .expect("wrap_at_width golden: width prefix must parse as usize");
        let text = parts.next().unwrap_or("");
        wrap_at_width(text, w).join("\n")
    });
}

// ---------------------------------------------------------------------
// UCD property adapters (single-char inputs)
// ---------------------------------------------------------------------

#[test]
fn golden_general_category() {
    run_golden("general_category", |s| {
        format!("{:?}", general_category(first_char(s)))
    });
}

#[test]
fn golden_script() {
    run_golden("script", |s| format!("{:?}", script(first_char(s))));
}

#[test]
fn golden_block() {
    run_golden("block", |s| format!("{:?}", block(first_char(s))));
}

// ---------------------------------------------------------------------
// Width (UAX #11)
// ---------------------------------------------------------------------

#[test]
fn golden_width_latin() {
    run_golden("width_latin", |s| {
        format!("{:?}", width_in(first_char(s), WidthMode::Latin))
    });
}

#[test]
fn golden_width_eastasian() {
    run_golden("width_eastasian", |s| {
        format!("{:?}", width_in(first_char(s), WidthMode::EastAsian))
    });
}

#[test]
fn golden_str_width_latin() {
    run_golden("str_width_latin", |s| {
        str_width_in(s, WidthMode::Latin).to_string()
    });
}

// ---------------------------------------------------------------------
// Grapheme segmentation (UAX #29)
// ---------------------------------------------------------------------

#[test]
fn golden_grapheme_boundaries() {
    run_golden("grapheme_boundaries", |s| {
        grapheme_boundaries(s)
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(",")
    });
}

#[test]
fn golden_grapheme_count() {
    run_golden("grapheme_count", |s| grapheme_count(s).to_string());
}

// ---------------------------------------------------------------------
// UTF-8 decoding (RFC 3629 round-trip)
// ---------------------------------------------------------------------

/// Serialize `Utf8Decoder` output by collecting every `Ok(char)` back
/// into a `String`. For every valid UTF-8 input (the goldens contain
/// only valid inputs; invalid inputs live in `utf8_decoder.rs`
/// property tests) the output equals the input — the fingerprint
/// locks in the round-trip invariant across any future decoder
/// rewrite.
#[test]
fn golden_utf8_decode() {
    run_golden("utf8_decode", |s| {
        let mut out = String::with_capacity(s.len());
        for item in Utf8Decoder::new(s.as_bytes()) {
            match item {
                Ok(c) => out.push(c),
                Err(e) => panic!("utf8_decode golden: unexpected decode error on `{:?}`: {:?}", s, e),
            }
        }
        out
    });
}
