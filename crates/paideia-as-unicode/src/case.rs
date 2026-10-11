//! UNI-2 (paideia-as#1589) — Unicode case folding + locale-aware
//! equality, plus full Unicode uppercase/lowercase.
//!
//! # Why a locale enum and not an `eq_ignore_case` flag
//!
//! The Unicode Character Database distributes two orthogonal fold rows
//! per codepoint in `CaseFolding.txt`:
//!
//! * **C** (common) + **F** (full) — the "default" fold used by every
//!   locale except Turkic. `İ` (U+0130) decomposes to `i` + combining
//!   dot above (U+0069 U+0307); `I` (U+0049) folds to `i`.
//! * **T** (Turkic) — Turkic-script locales (Turkish, Azerbaijani)
//!   preserve the dotted/dotless I/i distinction: `İ` folds to `i`
//!   (no combining dot) and `I` folds to `ı` (U+0131, dotless i).
//!
//! `CaseFoldLocale` is `#[non_exhaustive]` so future UCD-mandated
//! locales — Lithuanian's U+0307 dot-propagation (CaseFolding row
//! status is C+F, so no locale needed yet, but SpecialCasing.txt row
//! `lt` would), Azerbaijani (currently rolls under Turkic per CLDR but
//! may split if the Unicode spec diverges) — can be added without a
//! major-version bump. Modeling the locale as a flag on
//! `eq_ignore_case_unicode` would force every call-site to grow a new
//! `bool` argument per locale added; the enum keeps the dispatch
//! closed-under-pattern-match at the fold boundary.
//!
//! See `design/toolchain/paideia-as-unicode-spec.md` (UNI-0) §Case
//! Folding for the full API-surface rationale.
//!
//! # ASCII fast path
//!
//! All-ASCII inputs under the `Default` locale are folded via a byte
//! scan (`c | 0x20` for A..=Z, pass through otherwise), bypassing the
//! UCD tables entirely. This matters because the overwhelmingly common
//! `eq_ignore_case_unicode` traffic (semantic-shell command-name and
//! flag matching, record-field lookup) is ASCII-only, and we want it
//! to stay roughly constant time per byte — the UCD-table-driven
//! `caseless::default_case_fold_str` is linear in character count but
//! with a non-trivial per-character constant.
//!
//! Note: the ASCII fast path is **only** used for `Default`. In
//! `Turkic`, ASCII `I` (U+0049) must fold to the non-ASCII `ı`
//! (U+0131), so the fold is never byte-local.
//!
//! # Fold output is always valid UTF-8
//!
//! Full case folding may lengthen the input (U+00DF `ß` → `ss`;
//! U+0130 `İ` → `i` + U+0307). We allocate a `String` sized to a
//! lower-bound (`s.len()`) and let it grow; the result is UTF-8 by
//! construction (every pushed `char` is a valid scalar value, and
//! `String::push` encodes to UTF-8).

/// Locale discriminator for Unicode case folding.
///
/// `Default` applies the UCD `C` (common) + `F` (full) fold rows.
/// `Turkic` additionally applies the `T` rows for the I/i
/// dotted/dotless pair that Turkish and Azerbaijani require.
///
/// Marked `#[non_exhaustive]` so new locales (Lithuanian's
/// dot-above handling, a future Azerbaijani split) can be added
/// without a major-version bump. Marked `#[derive(Default)]` with
/// `#[default]` on `Default` so call-sites that do not care about the
/// locale can write `CaseFoldLocale::default()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum CaseFoldLocale {
    /// UCD `C` + `F` rows. The fold used by every locale that does
    /// not preserve the Turkic I/i dotted/dotless distinction.
    #[default]
    Default,
    /// UCD `C` + `F` rows, with the `T` row applied for U+0049 (`I`)
    /// → U+0131 (`ı`) and U+0130 (`İ`) → U+0069 (`i`).
    ///
    /// Used by Turkish and Azerbaijani locales (per CLDR 44); may
    /// split into a dedicated `Azerbaijani` variant in a future
    /// release if the Unicode spec diverges.
    Turkic,
}

/// Fold `s` to its locale-sensitive Unicode case-folded form.
///
/// Returns a fresh `String`; the folded output is always valid UTF-8
/// (full folds such as `ß` → `ss` or `İ` → `i\u{0307}` may make the
/// output longer in bytes than the input).
///
/// Idempotent: `fold(fold(x, l), l) == fold(x, l)` for every locale.
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::case::{fold, CaseFoldLocale};
/// assert_eq!(fold("Hello", CaseFoldLocale::Default), "hello");
/// assert_eq!(fold("ß", CaseFoldLocale::Default), "ss");
/// assert_eq!(fold("I", CaseFoldLocale::Turkic), "ı");
/// ```
pub fn fold(s: &str, locale: CaseFoldLocale) -> String {
    match locale {
        CaseFoldLocale::Default => fold_default_impl(s),
        CaseFoldLocale::Turkic => fold_turkic_impl(s),
    }
}

/// Convenience wrapper for `fold(s, CaseFoldLocale::Default)`.
///
/// Call this when you know you want the default fold and want to
/// avoid naming the enum variant at the call-site (e.g. one-shot
/// `eq_ignore_case_unicode` lowerings in the shell lexer).
#[inline]
pub fn fold_default(s: &str) -> String {
    fold(s, CaseFoldLocale::Default)
}

/// Return `true` iff `a` and `b` are equal after folding both to
/// their locale-sensitive case-folded form.
///
/// All-ASCII inputs under the `Default` locale take the
/// `str::eq_ignore_ascii_case` fast path (no allocation, byte-local).
/// Under `Turkic` the fast path is *not* applicable: ASCII `I`
/// (U+0049) folds to the non-ASCII `ı` (U+0131), so byte-case
/// equality disagrees with Unicode case equality for that locale.
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::case::{eq_ignore_case_unicode, CaseFoldLocale};
/// assert!(eq_ignore_case_unicode("Hello", "HELLO", CaseFoldLocale::Default));
/// // Turkic: dotless 'ı' equals ASCII 'I' because 'I' folds to 'ı'.
/// assert!(eq_ignore_case_unicode("I", "ı", CaseFoldLocale::Turkic));
/// // Same pair under Default: 'I' folds to 'i', not 'ı'.
/// assert!(!eq_ignore_case_unicode("I", "ı", CaseFoldLocale::Default));
/// ```
pub fn eq_ignore_case_unicode(a: &str, b: &str, locale: CaseFoldLocale) -> bool {
    if matches!(locale, CaseFoldLocale::Default) && a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    fold(a, locale) == fold(b, locale)
}

/// Unicode uppercasing per the standard library's `str::to_uppercase`.
///
/// Honors UCD `SpecialCasing.txt` locale-independent rows — in
/// particular `ß` (U+00DF) uppercases to `SS` (per CLDR 44 the
/// capital `ẞ` U+1E9E is a 1:1 fallback for the uppercase mapping;
/// Rust's std uses the `SS` expansion by default, matching the
/// `uppercase("straße") == "STRASSE"` test fixture).
///
/// Not locale-sensitive: the Turkic I/i dotted-dotless uppercasing
/// (Turkish `i` → `İ`) is deferred to a future UNI-2.1 ticket once a
/// Turkic-aware uppercaser lands; the current surface targets the
/// semantic-shell command-name and schema-field paths where
/// locale-independent casing is sufficient.
#[inline]
pub fn uppercase(s: &str) -> String {
    s.to_uppercase()
}

/// Unicode lowercasing per the standard library's `str::to_lowercase`.
///
/// Not locale-sensitive — see [`uppercase`] for the Turkic deferral.
/// The `eq_ignore_case_unicode` surface already handles the Turkic
/// case for equality queries, which covers the semantic-shell
/// command-matching contract (SH-D9.4).
#[inline]
pub fn lowercase(s: &str) -> String {
    s.to_lowercase()
}

// ---------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------

/// `Default`-locale fold. ASCII fast path then the `caseless` crate's
/// `default_case_fold_str`.
fn fold_default_impl(s: &str) -> String {
    if s.is_ascii() {
        // ASCII fast path: `c | 0x20` lowercases A..=Z and is a no-op
        // on every other 7-bit byte. The resulting string is valid
        // UTF-8 because every output byte is still `< 0x80`.
        let mut out = String::with_capacity(s.len());
        for &b in s.as_bytes() {
            let folded = if b.is_ascii_uppercase() { b | 0x20 } else { b };
            out.push(folded as char);
        }
        return out;
    }
    caseless::default_case_fold_str(s)
}

/// `Turkic`-locale fold. Pre-empts U+0049 and U+0130 to their Turkic
/// targets, then falls back to the default per-char fold for every
/// other codepoint.
fn fold_turkic_impl(s: &str) -> String {
    // Lower bound: at least `s.len()` bytes for the pass-through
    // majority; full folds may lengthen but the allocator will grow.
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            // U+0049 LATIN CAPITAL LETTER I → U+0131 LATIN SMALL
            // LETTER DOTLESS I.
            'I' => out.push('\u{0131}'),
            // U+0130 LATIN CAPITAL LETTER I WITH DOT ABOVE → U+0069
            // LATIN SMALL LETTER I (no trailing combining dot, unlike
            // the Default fold).
            '\u{0130}' => out.push('i'),
            // Every other codepoint uses the default (C+F) fold.
            // `caseless::Caseless` is an extension trait on
            // `Iterator<Item = char>`; its `default_case_fold()`
            // yields one or more folded codepoints per input char.
            other => {
                use caseless::Caseless;
                for folded in core::iter::once(other).default_case_fold() {
                    out.push(folded);
                }
            }
        }
    }
    out
}
