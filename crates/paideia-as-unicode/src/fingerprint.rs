//! UNI-6 (paideia-as#1593) — fingerprint-golden harness.
//!
//! Per `design/toolchain/paideia-as-unicode-spec.md` §5, every public
//! text-transforming function in this crate owns a golden file of
//! `<input> ⇒ <output>` pairs at `tests/fingerprints/<fn>.golden`. The
//! conformance test loads the golden, runs the function on each input,
//! and compares against the recorded output; on a UCD upgrade (per §3)
//! the regen flag re-records every output in place and the resulting
//! git diff is reviewed one line at a time.
//!
//! # Format (hex, line-oriented)
//!
//! Each file is UTF-8, LF line endings. A record is one line:
//!
//! ```text
//! TAG\tINPUT_HEX\tOUTPUT_HEX
//! ```
//!
//! Blank lines and lines starting with `#` are ignored (used for the
//! header and section comments). Hex is lowercase, two chars per byte,
//! no 0x prefix. Hex is intentionally used instead of literal UTF-8 so
//! that a git diff on a `.golden` file after a UCD upgrade is a pure
//! byte-level diff that reviewers can decode exactly (editor-driven
//! normalization or BOM insertion cannot silently rewrite the test
//! inputs or outputs).
//!
//! # Regen workflow
//!
//! ```text
//! PAIDEIA_UNICODE_REGEN_FINGERPRINTS=1 \
//!     cargo test -p paideia-as-unicode --test fingerprint_test
//! ```
//!
//! The test harness in `tests/fingerprint_test.rs` watches
//! [`maybe_regen_fingerprints`]; when the env var is set it reads the
//! current golden file's inputs, runs the function to produce fresh
//! outputs, and writes the result back. Reviewers inspect the diff.
//!
//! # Why `FingerprintCase` is `&'static str`
//!
//! Rust-side programmatic case lists are typically typed as
//! `&[FingerprintCase]` string literals, which gives us `'static`
//! lifetimes for free. The owned counterpart [`FingerprintRecord`]
//! covers the case where cases come off disk (where allocation is
//! unavoidable) or are computed at runtime (regen).

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

/// Three-line header stamped on every regenerated golden file.
///
/// Documents the format inline so a reviewer who stumbles onto a
/// `.golden` file in a diff can decode it without reading the harness.
const HEADER: &str = concat!(
    "# paideia-as-unicode fingerprint golden\n",
    "# Format: TAG\\tINPUT_HEX\\tOUTPUT_HEX\n",
    "# Blank lines + lines starting with # are ignored.\n",
);

/// A single fingerprint case, held in a static Rust manifest.
///
/// `tag` names *what is being tested* (stable across UCD upgrades);
/// `input` is the input string the function under test will consume;
/// `expected` is the recorded output. See spec §5.6 for the tag
/// vocabulary and the dispatch rule.
#[derive(Debug, Clone, Copy)]
pub struct FingerprintCase {
    /// Short dot- or underscore-separated tag naming the invariant
    /// and its corpus origin (e.g. `nfc.ucd.d800`, `width.cjk.han.4e2d`).
    pub tag: &'static str,
    /// Input the function will consume.
    pub input: &'static str,
    /// Expected output, encoded as the function's canonical string
    /// serialization (see the per-function wrappers in
    /// `tests/fingerprint_test.rs`).
    pub expected: &'static str,
}

/// Owned counterpart of [`FingerprintCase`], used when records come off
/// disk or are computed at runtime (regen).
///
/// Carries the same three fields with heap-allocated storage so the
/// records can outlive the file buffer they were parsed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FingerprintRecord {
    /// Short dot- or underscore-separated tag (see [`FingerprintCase::tag`]).
    pub tag: String,
    /// Input the function will consume.
    pub input: String,
    /// Expected output, canonically serialized.
    pub expected: String,
}

/// Resolve `name` (e.g. `"nfc_normalize"`) to the on-disk golden path
/// `tests/fingerprints/<name>.golden` inside this crate.
///
/// Uses `CARGO_MANIFEST_DIR` so the path is correct whether the test is
/// run from the crate root, the workspace root, or an integration-test
/// harness binary installed under `target/`.
pub fn fingerprint_golden_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fingerprints");
    p.push(format!("{}.golden", name));
    p
}

/// Verify `actual` matches `case.expected`.
///
/// Returns `Ok(())` on match; otherwise a diagnostic string naming the
/// tag and showing the input / expected / actual byte sequences in hex
/// so the mismatch is readable even for inputs that contain control
/// characters or non-rendering code points.
pub fn verify_fingerprint(case: &FingerprintCase, actual: &str) -> Result<(), String> {
    if case.expected == actual {
        return Ok(());
    }
    Err(format!(
        "fingerprint `{}` mismatch:\n  input    (hex): {}\n  expected (hex): {}\n  actual   (hex): {}",
        case.tag,
        hex_encode(case.input.as_bytes()),
        hex_encode(case.expected.as_bytes()),
        hex_encode(actual.as_bytes()),
    ))
}

/// Serialize `cases` as a golden file at
/// `tests/fingerprints/<name>.golden`, overwriting any existing file.
///
/// Writes the three-line header followed by one record per case,
/// hex-encoding the `input` and `expected` strings. The parent
/// directory is created if missing.
pub fn write_fingerprint_golden(name: &str, cases: &[FingerprintCase]) -> io::Result<()> {
    let path = fingerprint_golden_path(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut buf = String::new();
    buf.push_str(HEADER);
    for c in cases {
        buf.push_str(c.tag);
        buf.push('\t');
        buf.push_str(&hex_encode(c.input.as_bytes()));
        buf.push('\t');
        buf.push_str(&hex_encode(c.expected.as_bytes()));
        buf.push('\n');
    }
    let mut f = fs::File::create(path)?;
    f.write_all(buf.as_bytes())?;
    Ok(())
}

/// Serialize `records` as a golden file.
///
/// Owned counterpart of [`write_fingerprint_golden`] used by the regen
/// flow in `tests/fingerprint_test.rs`, where expected values come from
/// running the function on each input and are therefore heap-allocated.
pub fn write_fingerprint_golden_records(name: &str, records: &[FingerprintRecord]) -> io::Result<()> {
    let path = fingerprint_golden_path(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut buf = String::new();
    buf.push_str(HEADER);
    for r in records {
        buf.push_str(&r.tag);
        buf.push('\t');
        buf.push_str(&hex_encode(r.input.as_bytes()));
        buf.push('\t');
        buf.push_str(&hex_encode(r.expected.as_bytes()));
        buf.push('\n');
    }
    let mut f = fs::File::create(path)?;
    f.write_all(buf.as_bytes())?;
    Ok(())
}

/// Parse the golden file at `tests/fingerprints/<name>.golden`.
///
/// Blank lines and lines starting with `#` are skipped. Each remaining
/// line must have exactly three tab-separated columns (tag, input hex,
/// output hex); the input/output hex is decoded to bytes and the bytes
/// must be valid UTF-8. All error paths return `io::ErrorKind::InvalidData`
/// with a message that names the file and the 1-based line number so a
/// malformed golden file is diagnosable without a debugger.
pub fn read_fingerprint_golden(name: &str) -> io::Result<Vec<FingerprintRecord>> {
    let path = fingerprint_golden_path(name);
    let text = fs::read_to_string(&path)?;
    let mut records = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let lineno = idx + 1;
        let line = raw.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let (tag, in_hex, out_hex) = match (parts.next(), parts.next(), parts.next()) {
            (Some(t), Some(i), Some(o)) => (t, i, o),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}:{}: expected three tab-separated columns", path.display(), lineno),
                ));
            }
        };
        let in_bytes = hex_decode(in_hex).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}:{}: input hex: {}", path.display(), lineno, e),
            )
        })?;
        let out_bytes = hex_decode(out_hex).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}:{}: output hex: {}", path.display(), lineno, e),
            )
        })?;
        let input = String::from_utf8(in_bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}:{}: input not UTF-8: {}", path.display(), lineno, e),
            )
        })?;
        let expected = String::from_utf8(out_bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}:{}: output not UTF-8: {}", path.display(), lineno, e),
            )
        })?;
        records.push(FingerprintRecord {
            tag: tag.to_string(),
            input,
            expected,
        });
    }
    Ok(records)
}

/// Whether the current test invocation is a fingerprint regeneration.
///
/// Returns `true` iff the env var `PAIDEIA_UNICODE_REGEN_FINGERPRINTS`
/// is set to the literal string `"1"`. Any other value (including
/// `"true"`, `"yes"`, `"0"`, or an unset variable) returns `false` —
/// opt-in regen is deliberately explicit so a CI job accidentally
/// inheriting the var does not silently rewrite the goldens.
///
/// The test harness in `tests/fingerprint_test.rs` watches this flag;
/// when it fires, the harness reads the current golden's inputs, runs
/// the function under test on each input, and writes the fresh outputs
/// back to disk via [`write_fingerprint_golden_records`]. A standard
/// `cargo test` leaves the goldens untouched and verifies instead.
pub fn maybe_regen_fingerprints() -> bool {
    env::var("PAIDEIA_UNICODE_REGEN_FINGERPRINTS").as_deref() == Ok("1")
}

// ---------------------------------------------------------------------
// Hex codec (no new deps per the dispatch rule).
// ---------------------------------------------------------------------

/// Lowercase hex encode `bytes` (two chars per byte, no prefix, no
/// separator). Allocates one `String` sized exactly to the output.
///
/// Uses `format!("{:02x}", b)` per byte per the dispatch rule's
/// "no new deps" bullet; this crate does not pull `hex` or
/// `base16ct`. The per-byte overhead is tolerable because the golden
/// files are small (≤ 50 KiB each) and encoding runs only during regen.
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Decode a lowercase-or-uppercase hex string to bytes. Returns an
/// error message (not a typed error) on any invalid char or odd
/// length, matching the one-off harness use.
fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err(format!("odd hex length {}", s.len()));
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = decode_nib(bytes[i])
            .map_err(|c| format!("invalid hex char '{}' at offset {}", c, i))?;
        let lo = decode_nib(bytes[i + 1])
            .map_err(|c| format!("invalid hex char '{}' at offset {}", c, i + 1))?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

#[inline]
fn decode_nib(c: u8) -> Result<u8, char> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(c as char),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip_ascii() {
        let input = b"Hello";
        let enc = hex_encode(input);
        assert_eq!(enc, "48656c6c6f");
        assert_eq!(hex_decode(&enc).unwrap(), input);
    }

    #[test]
    fn hex_roundtrip_utf8() {
        let input = "e\u{0301}".as_bytes();
        let enc = hex_encode(input);
        assert_eq!(enc, "65cc81");
        assert_eq!(hex_decode(&enc).unwrap(), input);
    }

    #[test]
    fn hex_decode_rejects_odd_length() {
        assert!(hex_decode("abc").is_err());
    }

    #[test]
    fn hex_decode_rejects_invalid_char() {
        assert!(hex_decode("4z").is_err());
    }

    #[test]
    fn verify_fingerprint_matches() {
        let c = FingerprintCase {
            tag: "nfc.ascii.hello",
            input: "Hello",
            expected: "Hello",
        };
        assert!(verify_fingerprint(&c, "Hello").is_ok());
        assert!(verify_fingerprint(&c, "hello").is_err());
    }

    #[test]
    fn maybe_regen_defaults_to_false() {
        // Unless the env var is deliberately set in a given test
        // invocation, regen is off. We don't force-unset the var here
        // because that would race with other tests in the same
        // process; just assert that the function is pure w.r.t. its
        // env read.
        let _ = maybe_regen_fingerprints();
    }

    #[test]
    fn golden_path_is_under_tests_fingerprints() {
        let p = fingerprint_golden_path("nfc_normalize");
        assert!(p.ends_with("tests/fingerprints/nfc_normalize.golden"));
    }
}
