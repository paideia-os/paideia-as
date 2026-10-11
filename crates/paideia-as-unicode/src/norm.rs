//! UAX#15 Unicode Normalization Forms (NFC / NFD / NFKC / NFKD),
//! with a paideia-as-owned ASCII fast path and a dispatcher.
//!
//! This is the general-form generalization of what the earlier
//! [`crate::nfc`] shim exposed. See
//! `design/toolchain/paideia-as-unicode-spec.md` (UNI-0) for the
//! full narrative — chiefly:
//!
//!   * SH-D9.4 keeps NFC as the *IPC-boundary* canonical form (every
//!     string that crosses a capability boundary, every key in the
//!     command index / schema registry, every record field name).
//!   * NFD is required for Unicode-aware collation at R229 and for the
//!     grapheme-boundary stability tests (UAX#29 §3 operates on NFD
//!     cluster streams even when NFC is the storage form).
//!   * NFKC collapses CJK compatibility ideographs, legacy width
//!     variants, and legacy ligatures — this is the form the shell's
//!     identifier predicate (R220.M4 `Str::ident`) uses when comparing
//!     user-visible identifiers that originated in a NFKC-normalizing
//!     paste source (browser URL bars, IME keyboards).
//!   * NFKD is NFKC's decomposed companion and is here for
//!     completeness: it is the form the font-shaping path in R229.G will
//!     consume when it lands.
//!
//! # Fast path
//!
//! A `&str` whose every byte is `< 0x80` is pure ASCII, and pure ASCII
//! is trivially in *every* normalization form (no combining marks, no
//! decompositions, no compatibility equivalents). The ASCII scan is
//! linear, branch-predictable, and autovectorizable; it short-circuits
//! before the UCD tables ever load. The R222/R228 command-name
//! workloads dominate shell normalization traffic and are
//! overwhelmingly ASCII, so keeping their L1 footprint small matters
//! for the ≤50µs command-lookup budget (R222.M5 acceptance).
//!
//! # Dispatcher
//!
//! [`normalize`] and [`is_form`] fan out on the [`Form`] discriminant.
//! The point is to let callers hold a *runtime* normalization form (as
//! chosen by a terminal setting, a config field, or a cross-boundary
//! handshake) without specializing their call sites. Monomorphized
//! inner calls preserve the hot path: a `normalize(s, Form::Nfc)` call
//! compiles to the same two instructions the direct `nfc_normalize(s)`
//! call does once the match is lowered.

use unicode_normalization::{
    IsNormalized, UnicodeNormalization, is_nfc_quick, is_nfd_quick, is_nfkc_quick, is_nfkd_quick,
};

/// Which UAX#15 normalization form a dispatcher call targets.
///
/// `#[non_exhaustive]` so adding future variants (streaming, FCC, FCD —
/// the follow-on tickets UNI-14 / UNI-15 may add one of these) does not
/// break downstream match arms. Default is [`Form::Nfc`] because that is
/// the SH-D9.4 IPC-boundary form the rest of the shell assumes.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
#[non_exhaustive]
pub enum Form {
    /// Canonical decomposition followed by canonical composition
    /// (UAX#15 §1.3). The SH-D9.4 IPC-boundary form.
    #[default]
    Nfc,
    /// Canonical decomposition (UAX#15 §1.3). Used by collation
    /// (R229.M? identifier-key canonicalization) and by the grapheme
    /// boundary spec's reference text.
    Nfd,
    /// Compatibility decomposition followed by canonical composition
    /// (UAX#15 §1.3). Collapses CJK compat ideographs, legacy width
    /// variants, ligatures.
    Nfkc,
    /// Compatibility decomposition (UAX#15 §1.3). The NFKC companion;
    /// here for completeness and for the font-shaping path in R229.G.
    Nfkd,
}

/// Whether `input` is pure 7-bit ASCII and therefore already normalized
/// in every form.
///
/// Exposed for callers that want to branch on it directly. The
/// `Str::eq` lowering at R220.M4 uses this to skip normalizing both
/// sides when both are ASCII; the dispatcher paths below reuse it.
#[inline]
pub fn is_ascii_fast_path_eligible(input: &str) -> bool {
    input.is_ascii()
}

/// Whether `input` is already in NFC.
///
/// Uses the `unicode-normalization` quick-check table: `Yes` means
/// definitely NFC, `Maybe`/`No` fall through to the slow path. We
/// treat `Maybe` conservatively as "not yet known" (not NFC).
#[inline]
pub fn is_nfc(input: &str) -> bool {
    if is_ascii_fast_path_eligible(input) {
        return true;
    }
    matches!(is_nfc_quick(input.chars()), IsNormalized::Yes)
}

/// Whether `input` is already in NFD. Same conservative `Maybe`
/// handling as [`is_nfc`].
#[inline]
pub fn is_nfd(input: &str) -> bool {
    if is_ascii_fast_path_eligible(input) {
        return true;
    }
    matches!(is_nfd_quick(input.chars()), IsNormalized::Yes)
}

/// Whether `input` is already in NFKC. Same conservative `Maybe`
/// handling as [`is_nfc`].
#[inline]
pub fn is_nfkc(input: &str) -> bool {
    if is_ascii_fast_path_eligible(input) {
        return true;
    }
    matches!(is_nfkc_quick(input.chars()), IsNormalized::Yes)
}

/// Whether `input` is already in NFKD. Same conservative `Maybe`
/// handling as [`is_nfc`].
#[inline]
pub fn is_nfkd(input: &str) -> bool {
    if is_ascii_fast_path_eligible(input) {
        return true;
    }
    matches!(is_nfkd_quick(input.chars()), IsNormalized::Yes)
}

/// Dispatcher: whether `input` is already in the given [`Form`].
#[inline]
pub fn is_form(input: &str, form: Form) -> bool {
    match form {
        Form::Nfc => is_nfc(input),
        Form::Nfd => is_nfd(input),
        Form::Nfkc => is_nfkc(input),
        Form::Nfkd => is_nfkd(input),
    }
}

/// Normalize `input` to Unicode Normalization Form C (UAX#15).
///
/// Idempotent: `nfc_normalize(nfc_normalize(x).as_str()) == nfc_normalize(x)`.
/// The returned `String` is a fresh allocation even on the fast path.
pub fn nfc_normalize(input: &str) -> String {
    if is_ascii_fast_path_eligible(input) {
        return input.to_owned();
    }
    if matches!(is_nfc_quick(input.chars()), IsNormalized::Yes) {
        return input.to_owned();
    }
    input.nfc().collect()
}

/// Normalize `input` to Unicode Normalization Form D (canonical
/// decomposition; UAX#15 §1.3).
///
/// Idempotent: `nfd_normalize(nfd_normalize(x).as_str()) == nfd_normalize(x)`.
/// ASCII and already-NFD inputs are returned via a short-circuit copy.
pub fn nfd_normalize(input: &str) -> String {
    if is_ascii_fast_path_eligible(input) {
        return input.to_owned();
    }
    if matches!(is_nfd_quick(input.chars()), IsNormalized::Yes) {
        return input.to_owned();
    }
    input.nfd().collect()
}

/// Normalize `input` to Unicode Normalization Form KC (compatibility
/// decomposition followed by canonical composition; UAX#15 §1.3).
///
/// Idempotent: `nfkc_normalize(nfkc_normalize(x).as_str()) == nfkc_normalize(x)`.
/// ASCII and already-NFKC inputs are returned via a short-circuit copy.
pub fn nfkc_normalize(input: &str) -> String {
    if is_ascii_fast_path_eligible(input) {
        return input.to_owned();
    }
    if matches!(is_nfkc_quick(input.chars()), IsNormalized::Yes) {
        return input.to_owned();
    }
    input.nfkc().collect()
}

/// Normalize `input` to Unicode Normalization Form KD (compatibility
/// decomposition; UAX#15 §1.3).
///
/// Idempotent: `nfkd_normalize(nfkd_normalize(x).as_str()) == nfkd_normalize(x)`.
/// ASCII and already-NFKD inputs are returned via a short-circuit copy.
pub fn nfkd_normalize(input: &str) -> String {
    if is_ascii_fast_path_eligible(input) {
        return input.to_owned();
    }
    if matches!(is_nfkd_quick(input.chars()), IsNormalized::Yes) {
        return input.to_owned();
    }
    input.nfkd().collect()
}

/// Dispatcher: normalize `input` to the given [`Form`].
///
/// Parity contract: `normalize(x, Form::Nfc) == nfc_normalize(x)` (and
/// the analogue for every other variant), asserted by the dispatcher
/// parity test in `tests/norm_test.rs`.
#[inline]
pub fn normalize(input: &str, form: Form) -> String {
    match form {
        Form::Nfc => nfc_normalize(input),
        Form::Nfd => nfd_normalize(input),
        Form::Nfkc => nfkc_normalize(input),
        Form::Nfkd => nfkd_normalize(input),
    }
}
