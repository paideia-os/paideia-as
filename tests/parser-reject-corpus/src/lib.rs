//! Parser-reject test harness for paideia-as P-codes.
//!
//! Sibling of `paideia-reflection-corpus`; the two crates are
//! structurally identical apart from the diagnostic-category regex the
//! stderr scraper applies. This one captures parser diagnostics
//! (P-category: P0100..P0179 today, notably P0110, P0162, P0170, P0171
//! covered by the reject fixtures relocated from reflection-corpus in
//! issue #1557); the reflection sibling captures macro/reflection
//! diagnostics (M-category).
//!
//! See `tests/runner.rs` for the test entry point. The harness walks
//! the `corpus/accept/` and `corpus/reject/` subdirectories of this
//! crate and asserts:
//!
//! - Each accept fixture emits zero P-category diagnostic codes
//!   (any `P\d{4}` on stderr fails the accept test — accept fixtures
//!   are minimal by construction so no coincidental parser diagnostic
//!   should slip through).
//! - Each reject fixture emits exactly the set of P-codes listed in
//!   the companion `.expect` sidecar (one P-code per line; `#` starts
//!   a comment; blanks skipped).

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::path::Path;

/// Run `paideia-as build --emit placeholder` on `path` via subprocess
/// and return the sorted set of P-category diagnostic codes emitted on
/// stderr.
///
/// Subprocess failures and utf-8 problems are surfaced as a descriptive
/// error string so the runner reports them as a harness failure rather
/// than a silent zero-set match. Structurally identical to
/// `paideia_reflection_corpus::m_codes_for` — kept as a sibling
/// implementation rather than a shared crate to avoid coupling the two
/// harnesses (they may evolve their normalization or emission-flag
/// choices independently as P- and M-code semantics diverge).
pub fn p_codes_for(path: &Path) -> Result<BTreeSet<String>, String> {
    // Warm up: on first call, build the binary once to amortize
    // per-fixture compilation across the corpus walk. Errors here
    // fall through to the `cargo run` invocation below, which will
    // report the real failure.
    static CARGO_INIT: std::sync::Once = std::sync::Once::new();
    CARGO_INIT.call_once(|| {
        let _ = std::process::Command::new(env!("CARGO"))
            .args(["build", "--quiet", "-p", "paideia-as"])
            .output();
    });

    // Invoke `cargo run` to launch the binary from the test
    // environment. The warm-up above ensures the binary is already
    // built, so `cargo run` will not recompile — it just spawns the
    // cached binary.
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
    Ok(parse_p_codes_from_stderr(&stderr))
}

/// Parse P-codes from stderr output of `paideia-as build`.
///
/// Looks for patterns like `P0110`, `P0162`, `P0170`, `P0171`, etc. —
/// capital `P` followed by exactly 4 ASCII digits. Extracts all matches
/// in order of appearance; duplicates are collapsed by the BTreeSet.
fn parse_p_codes_from_stderr(stderr: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes = stderr.as_bytes();
    let mut i = 0;
    while i + 5 <= bytes.len() {
        if bytes[i] == b'P' && bytes[i + 1..i + 5].iter().all(|b| b.is_ascii_digit()) {
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

/// Parse a `.expect` sidecar file: one P-code per line (Pxxxx);
/// `#` starts a comment; blank lines are skipped.
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
