//! `cargo test --test runner -p paideia-reflection-corpus` runs the reflection corpus.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use paideia_reflection_corpus::{m_codes_for, parse_expect_file};

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
/// zero M-codes (M0308, M0309, M0311, M0312).
#[test]
fn accept_corpus_emits_no_macro_codes() {
    let dir = corpus_root().join("corpus/accept");
    let files = collect_pdx_files(&dir);
    let mut failures = Vec::new();
    for path in &files {
        match m_codes_for(path) {
            Ok(codes) if codes.is_empty() => {}
            Ok(codes) => failures.push(format!(
                "{}: expected no M-codes, got {:?}",
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

/// Reject corpus test: each `.pdx` file in `corpus/reject/` has a companion
/// `.expect` file that lists the expected M-codes. This test validates that
/// the emitted M-codes match expectations. Fixtures marked `#[ignore]` with
/// explicit reasons await further driver implementation.
#[test]
#[ignore = "reject corpus documentation-by-example until m3 driver (macro matching, expansion)"]
fn reject_corpus_emits_expected_codes() {
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
        match m_codes_for(path) {
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
