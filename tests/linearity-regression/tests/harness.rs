//! `cargo test --test harness -p paideia-linearity-regression` runs the
//! seed corpus.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use paideia_linearity_regression::{parse_expect_file, s_codes_for};

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

#[test]
fn accept_corpus_emits_no_s_codes() {
    let dir = corpus_root().join("accept");
    let files = collect_pdx_files(&dir);
    let mut failures = Vec::new();
    for path in &files {
        match s_codes_for(path) {
            Ok(codes) if codes.is_empty() => {}
            Ok(codes) => failures.push(format!(
                "{}: expected no S-codes, got {:?}",
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

// Reject corpus is documentation-by-example until the IR carries
// structured symbol/binding payloads (m2/m5).
//
// The harness now invokes `paideia-as build` via subprocess (m1-010),
// so the CLI wiring is complete. However, the LinearityWalker (and other
// walkers) runs end-to-end against kind-only IR. Linearity classes and
// effect/capability payloads are empty, so the walkers cannot fire
// S0901/S0903 (overused/wrong-effect) diagnostics on real source yet.
//
// Once m2/m5 inject structured payloads at lowering time, this test
// will light up and catch regressions where the accept corpus stops
// being clean.
#[test]
#[ignore = "reject corpus fixtures pending valid paideia-as syntax (phase-4-m1-002 adds walker; m-TBD wires elaboration)"]
fn reject_corpus_emits_expected_s_codes() {
    let dir = corpus_root().join("reject");
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
        match s_codes_for(path) {
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

#[test]
fn parse_expect_file_basic() {
    let s = "S0901\n# a comment\nS0903\n\n   S0904   \n";
    let parsed = parse_expect_file(s);
    let expected: BTreeSet<String> = ["S0901", "S0903", "S0904"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(parsed, expected);
}
