//! `cargo test --test runner -p paideia-effects-corpus` runs the effects corpus.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use paideia_effects_corpus::{codes_for, parse_expect_file};

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
/// zero effect-system codes (F1100, F1101, F1102, F1105, F1106; T0510).
#[test]
fn accept_corpus_emits_no_effect_codes() {
    let dir = corpus_root().join("corpus/accept");
    let files = collect_pdx_files(&dir);
    let mut failures = Vec::new();
    for path in &files {
        match codes_for(path) {
            Ok(codes) if codes.is_empty() => {}
            Ok(codes) => failures.push(format!(
                "{}: expected no effect codes, got {:?}",
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
/// `.expect` file that lists the expected effect-system codes. This test
/// validates that the emitted codes match expectations. Fixtures marked
/// `#[ignore]` with explicit reasons await further driver implementation.
#[test]
#[ignore = "reject corpus documentation-by-example until m3 elaborator driver wires effect walkers through end-to-end"]
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
        match codes_for(path) {
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

/// paideia-as#1307 (paideia-os R29.M2-002 #1024) — the effect-row →
/// cap-set coupling fixtures are not aspirational; the walker landed
/// with the issue and both the accept + reject fixtures must be live-
/// verified end-to-end.
///
/// Kept as a dedicated test so it is not swept up by the ignored
/// `reject_corpus_emits_expected_codes` gate that documents-by-example
/// the pre-m3 F/T fixtures.
#[test]
fn effect_cap_coupling_reject_fixture_emits_c1301() {
    let path = corpus_root().join("corpus/reject/r_mmio_effect_without_cap.pdx");
    assert!(path.exists(), "fixture missing: {}", path.display());
    let codes = codes_for(&path).expect("codes_for succeeds");
    assert!(
        codes.contains("C1301"),
        "expected C1301 in emitted codes; got {codes:?}"
    );
}

#[test]
fn effect_cap_coupling_accept_fixture_emits_no_codes() {
    let path = corpus_root().join("corpus/accept/effect_cap_pair_satisfied.pdx");
    assert!(path.exists(), "fixture missing: {}", path.display());
    let codes = codes_for(&path).expect("codes_for succeeds");
    assert!(
        !codes.contains("C1301"),
        "expected no C1301 on positive companion; got {codes:?}"
    );
}
