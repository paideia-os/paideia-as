//! UNI-4 (paideia-as#1591) — UAX #31 identifier predicates + UTS #39
//! identifier-security profile.
//!
//! # What this module owns
//!
//! * Byte-level [`is_xid_start`] / [`is_xid_continue`] predicates backed
//!   by the Servo-maintained [`unicode-ident`] crate (which tracks the
//!   current UCD and ships the two bitset tables UAX #31 §2 defines).
//! * The single-string [`is_identifier`] predicate the semantic shell's
//!   R220.M4 `Str::ident` lowering consumes: `s[0]` must be `XID_Start`,
//!   every subsequent character must be `XID_Continue`, and `s` must be
//!   stable under NFC. The NFC-stability bite is the paideia-as-owned
//!   extension to UAX #31 — SH-D9.4 pins NFC at every IPC boundary, and
//!   admitting a decomposed identifier at the semantic-shell boundary
//!   would let the same identifier survive two different byte spellings
//!   through the capability registry.
//! * An [`IdentProfile`] discriminator with three levels — [`Ascii`],
//!   [`Default`] (UAX #31 + NFC-stable, the shell default), and
//!   [`AllowedChars`] (UAX #31 + NFC-stable + UTS #39 §5 single-script
//!   restriction) — and a dispatcher [`check_ident`] that returns a
//!   structured [`IdentError`] on rejection.
//! * [`mixed_script`] — the UTS #39 §5 "scripts of a string" helper that
//!   returns the distinct non-`Common`, non-`Inherited` scripts seen in
//!   the input in a stable (name-sorted) order. UNI-5's [`crate::script`]
//!   is the per-character backend.
//!
//! # Why `unicode-ident`
//!
//! Three well-maintained Rust implementations of `XID_Start` /
//! `XID_Continue` exist (`unicode-xid`, `unicode-properties`,
//! `unicode-ident`). `unicode-ident` is the Dtolnay-maintained
//! `rustc_lexer`-adjacent crate; it is the one `syn` and `proc-macro2`
//! consume, and its bitset-based implementation is both faster and
//! smaller than the alternatives (~15 KiB of static tables vs ~45 KiB
//! for `unicode-properties`' full General_Category table). It is `#![no_std]`
//! and forbids `unsafe` in its public surface, matching this crate's
//! `#![forbid(unsafe_code)]` discipline. The workspace already pins
//! `unicode-ident = "1"` (currently 1.0.24+ against UCD 15.1).
//!
//! # Why `AllowedChars` (not the full UTS #39 moderately-restricted set)
//!
//! Shipping the full UTS #39 `IdentifierStatus.txt` + `confusables.txt`
//! data (~15 KiB + ~140 KiB respectively) is UNI-7's job — the UCD
//! vendor sweep. UNI-4 ships the structural piece: a profile enum, a
//! mixed-script detector, and an error-kind variant for `Confusable`
//! that stays unused until UNI-7 lands the confusables matrix. The
//! `IdentErrorKind::Confusable` variant is reserved so UNI-7 can
//! populate the check behind the same `AllowedChars` profile without a
//! consumer-visible enum shape change (the enum is `#[non_exhaustive]`
//! anyway, but keeping the variant lets consumers that enumerate the
//! surface see the full future shape).
//!
//! # NFC-stability contract
//!
//! `is_identifier` and `check_ident(Default)` both require the input to
//! be stable under NFC: `crate::norm::is_nfc(s)` must hold. This is a
//! paideia-as policy, strictly stronger than UAX #31. The rationale is
//! SH-D9.4: identifiers are compared byte-for-byte at the capability
//! boundary, and admitting a decomposed form would let `"caf\u{00E9}"`
//! and `"cafe\u{0301}"` present as two distinct identifiers of the same
//! user-visible name. Elaboration-time lowering already normalizes to
//! NFC; this check catches hand-rolled bytes that bypass that lowering.
//!
//! # Script-set determinism
//!
//! [`mixed_script`] returns a `Vec<Script>` sorted by the Script's
//! `Debug` name (lexicographic ASCII order on the enum variant spelling).
//! `Script` upstream derives `Clone, Copy, Debug, Eq, PartialEq, Hash`
//! but *not* `Ord`, so Debug-string sort is the stable alternative that
//! does not require pinning a `#[derive(PartialOrd, Ord)]` upstream
//! change. The sort key is a function of the enum variant, not of the
//! character set, so new UCD-era scripts land at well-defined positions
//! in the output.
//!
//! See `design/toolchain/paideia-as-unicode-spec.md` §4.8 and §6.4 for
//! the API-surface and error-kind freeze.

use crate::norm::is_nfc;
use crate::ucd::{Script, script};

/// Return `true` iff `c` is a UAX #31 `XID_Start` character.
///
/// Backed by `unicode_ident::is_xid_start`. Constant time per character.
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::is_xid_start;
/// assert!(is_xid_start('a'));
/// assert!(is_xid_start('A'));
/// assert!(is_xid_start('α'));
/// assert!(!is_xid_start('5'));
/// assert!(!is_xid_start('_')); // _ is XID_Continue but not XID_Start
/// ```
#[inline]
pub fn is_xid_start(c: char) -> bool {
    unicode_ident::is_xid_start(c)
}

/// Return `true` iff `c` is a UAX #31 `XID_Continue` character.
///
/// Backed by `unicode_ident::is_xid_continue`. Constant time per
/// character. Includes digits, combining marks and connector punctuation
/// (`_`); excludes whitespace, operators and surrogates.
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::is_xid_continue;
/// assert!(is_xid_continue('a'));
/// assert!(is_xid_continue('5'));
/// assert!(is_xid_continue('_'));
/// assert!(!is_xid_continue(' '));
/// assert!(!is_xid_continue('-'));
/// ```
#[inline]
pub fn is_xid_continue(c: char) -> bool {
    unicode_ident::is_xid_continue(c)
}

/// Return `true` iff `s` is a syntactically valid paideia-as identifier.
///
/// Three conditions:
///
/// * `s` is non-empty.
/// * The first character is `XID_Start`.
/// * Every subsequent character is `XID_Continue`.
/// * `s` is stable under NFC (`crate::norm::is_nfc(s)` holds).
///
/// The NFC-stability condition is a paideia-as policy strictly stronger
/// than UAX #31; see the module header for the SH-D9.4 rationale.
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::is_identifier;
/// assert!(is_identifier("hello"));
/// assert!(is_identifier("Σ"));
/// assert!(is_identifier("Σgma"));        // UAX #31 allows mixed script
/// assert!(!is_identifier(""));
/// assert!(!is_identifier("5foo"));       // leading digit
/// assert!(!is_identifier("hello world")); // space is not XID_Continue
/// ```
pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !is_xid_start(first) {
        return false;
    }
    for c in chars {
        if !is_xid_continue(c) {
            return false;
        }
    }
    is_nfc(s)
}

/// UTS #39 identifier-security profile selector.
///
/// `#[non_exhaustive]`: UNI-7's follow-on sweep may add
/// `ModeratelyRestricted` and `MinimallyRestricted` variants without a
/// semver break. [`Default`] is the shell default — UAX #31 identifier
/// shape plus NFC-stability, the SH-D9.4 IPC-boundary pin.
///
/// * [`Ascii`] — the lowest-friction profile: only `[A-Za-z_]` for the
///   first character, only `[A-Za-z0-9_]` for the rest. No NFC check is
///   needed (ASCII is NFC-stable by construction). This is the profile
///   the R220 `name` lint recommends for public capability names.
/// * [`Default`] — the UAX #31 + NFC-stable identifier the semantic
///   shell's `Str::ident` lowering checks against. Permits every script
///   `XID_Start` admits (so `Σgma`, `αβγ`, `日本` all pass), but
///   requires NFC.
/// * [`AllowedChars`] — Default plus the UTS #39 §5 single-script
///   restriction. Rejects strings whose non-`Common`, non-`Inherited`
///   scripts are more than one. The confusables matrix (UTS #39 §4) is
///   deferred to UNI-7; the structural shape is here so UNI-7 can wire
///   in the confusables check behind this profile without an API
///   change. See `design/toolchain/paideia-as-unicode-spec.md` §4.8 for
///   the UNI-7 freeze.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
#[non_exhaustive]
pub enum IdentProfile {
    /// ASCII-only identifier: first char `[A-Za-z_]`, remainder
    /// `[A-Za-z0-9_]`. NFC is trivially satisfied.
    Ascii,
    /// UAX #31 identifier (`XID_Start` + `XID_Continue*`) with the
    /// paideia-as NFC-stability extension. The shell default.
    #[default]
    Default,
    /// Default + UTS #39 §5 single-script restriction. The confusables
    /// matrix is a UNI-7 follow-up; today this profile only enforces
    /// script-set uniqueness.
    AllowedChars,
}

/// Reason an identifier failed [`check_ident`].
///
/// `#[non_exhaustive]`: UNI-7 may add variants
/// (`DisallowedByAllowedChars`, finer-grained `Confusable` payloads) as
/// the UCD-vendor sweep lands.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum IdentErrorKind {
    /// The first character is not `XID_Start` (for [`IdentProfile::Default`]
    /// / [`IdentProfile::AllowedChars`]), or not `[A-Za-z_]` for
    /// [`IdentProfile::Ascii`]. Includes the empty-input case: an empty
    /// identifier reports `BadStart` at offset 0.
    BadStart,
    /// A non-first character is not `XID_Continue` (for the UAX #31
    /// profiles), or not `[A-Za-z0-9_]` for [`IdentProfile::Ascii`].
    BadContinue,
    /// The input is syntactically a valid UAX #31 identifier but is not
    /// stable under NFC. Only reported for the two UAX #31 profiles;
    /// [`IdentProfile::Ascii`] can never produce this kind.
    NotNfc,
    /// The input mixes scripts in a way UTS #39 §5 forbids under the
    /// active [`IdentProfile::AllowedChars`] level. `scripts` carries
    /// the distinct non-`Common`, non-`Inherited` scripts seen, sorted
    /// by the Script variant's `Debug` name (see [`mixed_script`]).
    MixedScript {
        /// The distinct scripts observed in the input, sorted by Debug
        /// name for deterministic output.
        scripts: Vec<Script>,
    },
    /// The input contains a sequence the UTS #39 confusables matrix
    /// flags. Reserved for UNI-7; `check_ident` never produces this
    /// variant until the UCD-vendor sweep lands.
    Confusable,
}

/// Structured identifier-validation error.
///
/// `offset` is the byte offset into the input that first exhibits the
/// problem:
///
/// * `BadStart` → always `0`.
/// * `BadContinue` → the byte offset of the first bad character.
/// * `NotNfc` → always `0` (the check is whole-string).
/// * `MixedScript` → always `0` (the check is whole-string).
/// * `Confusable` → reserved for UNI-7; `0` by convention until then.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct IdentError {
    /// Byte offset of the first offending character (see the type doc).
    pub offset: usize,
    /// What went wrong.
    pub kind: IdentErrorKind,
}

impl IdentError {
    /// Shorthand constructor. The struct is not `#[non_exhaustive]`
    /// (the discriminating future-growth axis is [`IdentErrorKind`]),
    /// so external callers can also construct it directly.
    #[inline]
    fn new(offset: usize, kind: IdentErrorKind) -> Self {
        IdentError { offset, kind }
    }
}

/// Validate `s` as an identifier under the given [`IdentProfile`].
///
/// Returns `Ok(())` on success and a structured [`IdentError`] on
/// rejection. See the type docs for the per-profile contract.
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::{check_ident, IdentProfile, IdentErrorKind};
/// assert_eq!(check_ident("hello", IdentProfile::Default), Ok(()));
///
/// let err = check_ident("5foo", IdentProfile::Default).unwrap_err();
/// assert_eq!(err.kind, IdentErrorKind::BadStart);
///
/// let err = check_ident("Σ", IdentProfile::Ascii).unwrap_err();
/// assert_eq!(err.kind, IdentErrorKind::BadStart);
/// ```
pub fn check_ident(s: &str, profile: IdentProfile) -> Result<(), IdentError> {
    match profile {
        IdentProfile::Ascii => check_ascii(s),
        IdentProfile::Default => check_default(s),
        IdentProfile::AllowedChars => {
            check_default(s)?;
            check_single_script(s)
        }
    }
}

/// ASCII-profile check. Does not consult the Unicode tables.
fn check_ascii(s: &str) -> Result<(), IdentError> {
    let bytes = s.as_bytes();
    let first = match bytes.first() {
        Some(b) => *b,
        None => return Err(IdentError::new(0, IdentErrorKind::BadStart)),
    };
    if !is_ascii_ident_start_byte(first) {
        return Err(IdentError::new(0, IdentErrorKind::BadStart));
    }
    for (i, &b) in bytes.iter().enumerate().skip(1) {
        if !is_ascii_ident_continue_byte(b) {
            return Err(IdentError::new(i, IdentErrorKind::BadContinue));
        }
    }
    Ok(())
}

/// UAX #31 + NFC-stable check.
fn check_default(s: &str) -> Result<(), IdentError> {
    let mut iter = s.char_indices();
    let (_, first) = match iter.next() {
        Some(x) => x,
        None => return Err(IdentError::new(0, IdentErrorKind::BadStart)),
    };
    if !is_xid_start(first) {
        return Err(IdentError::new(0, IdentErrorKind::BadStart));
    }
    for (i, c) in iter {
        if !is_xid_continue(c) {
            return Err(IdentError::new(i, IdentErrorKind::BadContinue));
        }
    }
    if !is_nfc(s) {
        return Err(IdentError::new(0, IdentErrorKind::NotNfc));
    }
    Ok(())
}

/// UTS #39 §5 single-script restriction.
///
/// Called *after* `check_default` succeeds, so we know `s` is a valid
/// UAX #31 identifier and NFC-stable. The remaining question is whether
/// its script set (excluding `Common` and `Inherited`) is a singleton.
fn check_single_script(s: &str) -> Result<(), IdentError> {
    let scripts = mixed_script(s);
    if scripts.len() > 1 {
        return Err(IdentError::new(0, IdentErrorKind::MixedScript { scripts }));
    }
    Ok(())
}

#[inline]
fn is_ascii_ident_start_byte(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

#[inline]
fn is_ascii_ident_continue_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Return the distinct scripts observed in `s`, excluding
/// [`Script::Common`] and [`Script::Inherited`], sorted by the Script
/// variant's `Debug` name (deterministic across releases).
///
/// Backed by [`crate::script`] (UNI-5). Pure ASCII returns `[Latin]`
/// because UCD Scripts.txt assigns Latin to `A..Z`, `a..z` (the ASCII
/// digits are `Common`; the `_` is also `Common`; both are filtered).
///
/// The empty string returns an empty vector.
///
/// # Examples
///
/// ```
/// use paideia_as_unicode::{mixed_script, Script};
/// assert_eq!(mixed_script(""), Vec::<Script>::new());
/// assert_eq!(mixed_script("hello"), vec![Script::Latin]);
/// // Σ is Greek, "gma" is Latin — UAX #31 permits this; UTS #39 flags it.
/// let s = mixed_script("Σgma");
/// assert_eq!(s, vec![Script::Greek, Script::Latin]);
/// ```
pub fn mixed_script(s: &str) -> Vec<Script> {
    // Collect distinct scripts, filtering Common and Inherited per UTS
    // #39 §5. Small-N dedup (identifier scripts rarely exceed 2–3): a
    // linear scan over a `Vec` beats a HashSet in both cycles and
    // allocation count.
    let mut scripts: Vec<Script> = Vec::new();
    for c in s.chars() {
        let sc = script(c);
        if matches!(sc, Script::Common | Script::Inherited) {
            continue;
        }
        if !scripts.iter().any(|existing| *existing == sc) {
            scripts.push(sc);
        }
    }
    // Deterministic order by Debug-name — `Script` is upstream
    // `#[non_exhaustive]` and does not derive `Ord`, so the Debug
    // spelling is the only stable key.
    scripts.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
    scripts
}
