//! UAX#15 Normalization Form C, with a paideia-as-owned ASCII fast path.
//!
//! Per SH-D9.4 (see `design/terminal/semantic-shell.md` §10.4), every
//! string that crosses an IPC boundary — and, by the R220.M4 contract,
//! every argument to `Str::eq` — is normalized to NFC. This keeps
//! "looks identical, compares unequal" bugs (é encoded as U+00E9 vs.
//! U+0065 U+0301) out of the shell's command-name index, the schema
//! registry's fingerprint keys, and the record-field name space.
//!
//! # Fast path
//!
//! An input whose every byte is `< 0x80` is pure ASCII, and pure ASCII
//! is trivially NFC (no combining marks, no compositions). We detect
//! this in a byte-scan (linear, branch-predictable, autovectorizable)
//! and return an owned copy without touching the UAX#15 tables. The
//! command-name workloads R222/R228 dominate the shell's normalization
//! traffic and are overwhelmingly ASCII; keeping their L1 footprint
//! small matters for the ≤50µs command-lookup budget the R222.M5
//! acceptance requires.

use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

/// Whether `input` is pure 7-bit ASCII and therefore already NFC
/// without consulting the combining-class tables.
///
/// Exposed for callers that want to branch on it directly (the
/// `Str::eq` lowering at R220.M4 uses this to skip normalizing both
/// sides when both are ASCII).
#[inline]
pub fn is_ascii_fast_path_eligible(input: &str) -> bool {
    input.is_ascii()
}

/// Whether `input` is already in NFC.
///
/// Uses the `unicode-normalization` quick-check table: `Yes` means
/// definitely NFC, `Maybe`/`No` fall through to the slow path. We
/// treat `Maybe` conservatively as "not yet known", i.e. re-normalize.
pub fn is_nfc(input: &str) -> bool {
    if is_ascii_fast_path_eligible(input) {
        return true;
    }
    matches!(is_nfc_quick(input.chars()), IsNormalized::Yes)
}

/// Normalize `input` to Unicode Normalization Form C (UAX#15).
///
/// Idempotent: `nfc_normalize(nfc_normalize(x).as_str()) == nfc_normalize(x)`.
/// This is asserted by the round-trip property test at
/// `tests/nfc_roundtrip_property.rs`.
///
/// The returned `String` is a fresh allocation even on the fast path,
/// so the caller may consume or store it without borrowing from
/// `input`. Callers that want to avoid the allocation for
/// already-normalized inputs should gate on [`is_nfc`] first.
pub fn nfc_normalize(input: &str) -> String {
    if is_ascii_fast_path_eligible(input) {
        return input.to_owned();
    }
    if matches!(is_nfc_quick(input.chars()), IsNormalized::Yes) {
        return input.to_owned();
    }
    input.nfc().collect()
}
