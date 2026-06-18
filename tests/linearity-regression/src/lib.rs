//! Linearity-regression harness for paideia-as.
//!
//! See `tests/harness.rs` for the test entry point. The harness walks
//! `accept/` and `reject/` subdirectories of this crate and asserts:
//!
//! - Each accept file produces zero `S`-category diagnostics.
//! - Each reject file produces exactly the set of `S`-codes listed in
//!   the companion `<file>.expect` sidecar (one `Sxxxx` code per line).

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::path::Path;

/// Run `paideia-as build` on `path` via subprocess and return the sorted
/// set of `S`-category diagnostic codes emitted on stderr.
///
/// Reading errors and subprocess failures are surfaced as a descriptive
/// error string so the harness reports them as a failure.
pub fn s_codes_for(path: &Path) -> Result<BTreeSet<String>, String> {
    // Warm up: on first call, build the binary to amortize per-test compilation.
    static CARGO_INIT: std::sync::Once = std::sync::Once::new();
    CARGO_INIT.call_once(|| {
        let _ = std::process::Command::new(env!("CARGO"))
            .args(["build", "--quiet", "-p", "paideia-as"])
            .output();
    });

    // Invoke `cargo run` to launch the binary from the test environment.
    // The warm-up above ensures the binary is already built, so cargo run
    // will not recompile and just spawns the cached binary.
    let mut cmd = std::process::Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("paideia-as")
        .arg("--")
        .arg("build")
        .arg("--emit")
        .arg("placeholder")
        .arg(path);
    cmd.env("NO_COLOR", "1");

    let out = cmd
        .output()
        .map_err(|e| format!("failed to spawn cargo run: {e}"))?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    Ok(parse_s_codes_from_stderr(&stderr))
}

/// Parse S-codes from stderr output of `paideia-as build`.
///
/// Looks for patterns like `S0901`, `S0900`, etc. — capital S followed
/// by exactly 4 ASCII digits. Extracts all matches in order of appearance.
fn parse_s_codes_from_stderr(stderr: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes = stderr.as_bytes();
    let mut i = 0;
    while i + 5 <= bytes.len() {
        if bytes[i] == b'S' && bytes[i + 1..i + 5].iter().all(|b| b.is_ascii_digit()) {
            if let Ok(s) = std::str::from_utf8(&bytes[i..i + 5]) {
                out.insert(s.to_string());
            }
            i += 5;
        } else {
            i += 1;
        }
    }
    out
}

/// Parse a `.expect` sidecar file: one `Sxxxx` code per line; `#`
/// starts a comment; blank lines are skipped.
pub fn parse_expect_file(content: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in content.lines() {
        let trimmed = match line.split('#').next() {
            Some(s) => s.trim(),
            None => "",
        };
        if !trimmed.is_empty() {
            out.insert(trimmed.to_string());
        }
    }
    out
}
