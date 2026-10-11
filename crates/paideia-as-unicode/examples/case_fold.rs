//! `case_fold` — Unicode case folding + the Turkic I/ı/İ/i pair.
//!
//! Shows the three guarantees UNI-2 lands:
//!   1. `fold_default` collapses the UCD `C` + `F` rows (so "ß" → "ss"
//!      and "İ" (U+0130) → "i\u{0307}");
//!   2. the `Turkic` locale applies the `T` row, so `I` folds to `ı`
//!      (U+0131) and `İ` folds to `i` — the two pairs that would
//!      otherwise collide in Turkish/Azerbaijani input;
//!   3. `eq_ignore_case_unicode` is symmetric and transitive against
//!      the chosen locale.
//!
//! Run with: `cargo run --example case_fold -p paideia-as-unicode`.

use paideia_as_unicode::{CaseFoldLocale, eq_ignore_case_unicode, fold, fold_default};

fn main() {
    // Default fold: UCD `C` + `F` rows.
    let sharp_s = "straße";
    let folded = fold_default(sharp_s);
    println!("fold_default({:?}) = {:?}", sharp_s, folded);
    assert_eq!(folded, "strasse", "ß must full-fold to ss");

    // Default fold: dotted capital I folds to lowercase i + combining dot above.
    let dotted = "\u{0130}STANBUL"; // "İSTANBUL"
    let folded = fold_default(dotted);
    println!("fold_default({:?}) = {:?}", dotted, folded);
    assert_eq!(folded, "i\u{0307}stanbul");

    // Turkic locale: I/i preserve the dotted/dotless distinction.
    //   U+0049 (I)  → U+0131 (ı)    [dotless]
    //   U+0130 (İ)  → U+0069 (i)    [dotted]
    let turkic_pair = "Iı İi";
    let default_fold = fold(turkic_pair, CaseFoldLocale::Default);
    let turkic_fold = fold(turkic_pair, CaseFoldLocale::Turkic);
    println!("Default fold({:?}) = {:?}", turkic_pair, default_fold);
    println!("Turkic  fold({:?}) = {:?}", turkic_pair, turkic_fold);

    // The two locales produce DIFFERENT normal forms for the Turkic pair.
    assert_ne!(default_fold, turkic_fold);

    // Symmetry: eq_ignore_case_unicode is order-independent.
    assert!(eq_ignore_case_unicode("Straße", "STRASSE", CaseFoldLocale::Default));
    assert!(eq_ignore_case_unicode("STRASSE", "Straße", CaseFoldLocale::Default));

    // Turkic locale: "I" and "ı" compare equal; "I" and "i" do NOT.
    assert!(eq_ignore_case_unicode("I", "ı", CaseFoldLocale::Turkic));
    assert!(!eq_ignore_case_unicode("I", "i", CaseFoldLocale::Turkic));
    // But under Default, "I" and "i" DO compare equal.
    assert!(eq_ignore_case_unicode("I", "i", CaseFoldLocale::Default));

    println!("\nall case-fold assertions passed.");
}
