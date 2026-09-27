//! `cargo test --test runner -p paideia-end-to-end` runs the smoke corpus.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use paideia_end_to_end::{codes_for, parse_expect_file};

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

/// Corpus test: each `.pdx` file in `codes/` has a companion `.expect` file
/// that lists the codes it should emit. This test validates that the .expect
/// sidecars exist and are properly formatted, and (once the walkers fire on
/// structured IR) that the emitted codes match expectations.
///
/// PAS-DEBT-B7-001 (paideia-as#1529): this is the true home of the
/// `codes/m2_macro_*.pdx` fixtures cited in the debt-catalog row for B7-001.
/// Two independent blockers remain (both open):
///   (1) Structured-IR payload emission for linearity / effect / capability
///       diagnostic classes (m2/m5) — needed for the S/F/C-code walkers to
///       fire against the corpus at all.
///   (2) Macro driver: template expansion + substitution (#1541) and
///       repetition + hygiene (#1542). The catalog previously cited #217
///       as "macro driver", but #217 is a closed handler-typing ticket;
///       the correct macro-driver dependencies are #1541 and #1542.
/// See `design/paideia-as-debt-catalog.md` §8.2 (B7-001).
#[test]
#[ignore = "PAS-DEBT-B7-001 (#1529): blocked on (1) structured-IR payload emission for S/F/C-code walkers (m2/m5) AND (2) macro driver #1541 (template expansion) + #1542 (repetition + hygiene) for m2_macro_*.pdx"]
fn codes_corpus_matches_expect_files() {
    let dir = corpus_root().join("codes");
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
        "{} codes files failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// This test is NOT ignored. It validates that every diagnostic code in the
/// acceptance criteria is present in at least one `.expect` file. This catches
/// "fixture missing for code X" regressions even before the walkers are fully
/// wired to fire on structured IR.
#[test]
fn expect_files_cover_every_listed_code() {
    // Acceptance criteria codes: all diagnostic codes that should surface
    // per the m1-009 deliverable.
    let required_codes: BTreeSet<&str> = [
        "S0900", "S0901", "S0903", "S0906", "S0907", "F1100", "F1101", "F1102", "F1105", "F1106",
        "C1300", "T0501",
    ]
    .into_iter()
    .collect();

    let dir = corpus_root().join("codes");
    let files = collect_pdx_files(&dir);

    let mut found_codes: BTreeSet<String> = BTreeSet::new();
    let mut errors = Vec::new();

    for path in &files {
        let expect_path = path.with_extension("expect");
        match std::fs::read_to_string(&expect_path) {
            Ok(content) => {
                let codes = parse_expect_file(&content);
                found_codes.extend(codes);
            }
            Err(_) => {
                errors.push(format!(
                    "missing .expect sidecar: {}",
                    expect_path.display()
                ));
            }
        }
    }

    for required in &required_codes {
        if !found_codes.contains(*required) {
            errors.push(format!("code {} not found in any .expect file", required));
        }
    }

    assert!(
        errors.is_empty(),
        "missing codes or broken .expect files:\n{}",
        errors.join("\n")
    );
}
