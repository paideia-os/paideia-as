//! Property: every well-formed `DiagnosticCode` Displays to its canonical
//! wire form, which `FromStr` parses back to an equal code.
//!
//! Severity is not part of the wire form (see `code::FromStr` docs and
//! `diagnostics.md` §1), so the strategy fixes severity to `Error`.

use paideia_as_diagnostics::{Category, DiagnosticCode, Severity};
use proptest::prelude::*;
use std::str::FromStr;

const CATEGORIES: [Category; 15] = [
    Category::E,
    Category::P,
    Category::M,
    Category::T,
    Category::S,
    Category::F,
    Category::C,
    Category::O,
    Category::U,
    Category::B,
    Category::D,
    Category::L,
    Category::W,
    Category::R,
    Category::Z,
];

/// Generate an arbitrary canonical-form `DiagnosticCode`: pick a category,
/// then pick a number from its allocated range. Severity defaults to
/// `Severity::Error` because the wire form does not carry severity.
fn arb_diagnostic_code() -> impl Strategy<Value = DiagnosticCode> {
    (0usize..CATEGORIES.len()).prop_flat_map(|cat_idx| {
        let category = CATEGORIES[cat_idx];
        let range = category.range();
        (*range.start()..=*range.end()).prop_map(move |number| {
            DiagnosticCode::new(category, Severity::Error, number)
                .expect("strategy yields in-range numbers")
        })
    })
}

proptest! {
    /// Smoke run: 256 cases on every `cargo test` invocation.
    #[test]
    fn prop_display_parse_roundtrip(code in arb_diagnostic_code()) {
        let s = code.to_string();
        let parsed = DiagnosticCode::from_str(&s).expect("failed to parse own display output");
        prop_assert_eq!(code, parsed);
    }
}

/// The issue's acceptance criterion: 10 000 round-trips.
#[test]
fn proptest_display_parse_roundtrip_10k() {
    proptest!(ProptestConfig::with_cases(10_000), |(code in arb_diagnostic_code())| {
        let s = code.to_string();
        let parsed = DiagnosticCode::from_str(&s).expect("failed to parse own display output");
        prop_assert_eq!(code, parsed);
    });
}
