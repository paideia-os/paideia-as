//! `cargo test --test runner -p paideia-parser-reject-corpus` runs the
//! parser-reject corpus.
//!
//! The two tests are structurally identical to the ones in
//! `paideia-reflection-corpus/tests/runner.rs`; only the diagnostic
//! category differs (P vs M) and the file walker points at this
//! crate's `corpus/{accept,reject}/` subdirectories.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use paideia_parser_reject_corpus::{p_codes_for, parse_expect_file};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_pdx_files(dir: &Path) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read corpus dir")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdx"))
        .collect();
    out.sort();
    out
}

/// Accept corpus test: each `.pdx` file in `corpus/accept/` must emit
/// zero P-category diagnostic codes (any `P\d{4}` on stderr fails).
/// Accept fixtures are deliberately minimal so no coincidental parser
/// diagnostic should slip through.
#[test]
fn accept_corpus_emits_no_parser_codes() {
    let dir = corpus_root().join("corpus/accept");
    let files = collect_pdx_files(&dir);
    let mut failures = Vec::new();
    for path in &files {
        match p_codes_for(path) {
            Ok(codes) if codes.is_empty() => {}
            Ok(codes) => failures.push(format!(
                "{}: expected no P-codes, got {:?}",
                path.file_name().unwrap().to_string_lossy(),
                codes
            )),
            Err(e) => failures.push(format!(
                "{}: harness error: {e}",
                path.file_name().unwrap().to_string_lossy()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} accept files failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Reject corpus test: each `.pdx` file in `corpus/reject/` has a
/// companion `.expect` file that lists the expected P-codes. This test
/// asserts the emitted P-code set matches the expected set exactly.
///
/// Marked `#[ignore]` because the harness invokes `cargo run -p
/// paideia-as` per fixture, which requires a `cargo`-driven test
/// environment with the workspace fully warmed. In a CI shape that
/// runs `cargo test --workspace` this works out of the box; in a
/// stripped or offline shape it does not. Un-ignore locally with
/// `cargo test --test runner -p paideia-parser-reject-corpus -- \
/// --include-ignored` once the CI story is established for this
/// harness (mirroring the reflection-corpus reject-runner ignore
/// story, though the reflection runner is ignored for a different
/// reason — pipeline glue, not env prerequisites).
///
/// The four P-code fixtures relocated from
/// `tests/reflection-corpus/corpus/reject/` in issue #1557 target
/// diagnostics already emitted end-to-end by the parser today:
///
/// - `r_antiquote_outside_quote.pdx` → P0170 (quote.rs:131)
/// - `r_finally_not_last.pdx`        → P0162 (parse_handler.rs:167)
/// - `r_malformed_quote.pdx`         → P0171 (quote.rs:90)
/// - `r_unknown_fragment_kind.pdx`   → P0110 (parse_macro.rs:392)
///
/// See the crate-level `README.md` for the sidecar convention and
/// fixture-authoring guidance.
#[test]
#[ignore = "reject runner shells out to `cargo run -p paideia-as` per \
    fixture; requires a `cargo test --workspace`-style environment with \
    the workspace pre-warmed. Un-ignore locally with `cargo test --test \
    runner -p paideia-parser-reject-corpus -- --include-ignored` once \
    the harness has been validated against the full corpus in CI. \
    Mirrors the reflection-corpus reject-runner ignore posture."]
fn reject_corpus_emits_expected_p_codes() {
    let dir = corpus_root().join("corpus/reject");
    let files = collect_pdx_files(&dir);
    let mut failures = Vec::new();
    for path in &files {
        let expect_path = path.with_extension("expect");
        let expected: BTreeSet<String> = match std::fs::read_to_string(&expect_path) {
            Ok(s) => parse_expect_file(&s),
            Err(_) => {
                failures.push(format!(
                    "{}: missing .expect sidecar at {}",
                    path.file_name().unwrap().to_string_lossy(),
                    expect_path.display()
                ));
                continue;
            }
        };
        match p_codes_for(path) {
            Ok(codes) if codes == expected => {}
            Ok(codes) => failures.push(format!(
                "{}: expected {:?}, got {:?}",
                path.file_name().unwrap().to_string_lossy(),
                expected,
                codes
            )),
            Err(e) => failures.push(format!(
                "{}: harness error: {e}",
                path.file_name().unwrap().to_string_lossy()
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} reject files failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
