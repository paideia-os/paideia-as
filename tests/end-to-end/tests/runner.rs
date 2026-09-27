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
///
/// Reactivation status (Wave 38, v0.36.66):
///   - Slice B (#1541, v0.36.65, template expansion + `expand_macro`) landed.
///   - Slice C (#1542, v0.36.66, repetition + soft hygiene) landed.
///     The elaborator-side macro grammar and `expand_macro` are complete;
///     round-trip unit tests live in
///     `crates/paideia-as-elaborator/src/macro_expand.rs`.
///
/// Three independent blockers still gate un-`#[ignore]`'ing this test:
///
///   (1) Structured-IR payload emission (m2/m5) for the S / F / C-code
///       diagnostic walkers is still absent — the walkers never fire
///       against real source lowered through `paideia-as-elaborator`.
///       Shared blocker with `PAS-DEBT-B7-004`; no dedicated tracking
///       issue yet. See `design/paideia-as-debt-catalog.md` §8.2.
///
///   (2) `expand_macro` (elaborator crate) is NOT wired into the
///       `paideia-as` binary's `cmd_build` pass pipeline —
///       `grep -rn expand_macro crates/paideia-as/src/` returns
///       empty. Consequently the `codes_for` harness in
///       `tests/end-to-end/src/lib.rs` invokes `paideia-as build`,
///       which parses the macro declaration but never expands any
///       invocation, so `M0311` (deep / infinite recursion) never
///       fires and template substitution never runs end-to-end.
///       No dedicated tracking issue yet; belongs on the Phase-2
///       "wire elaborator passes into cmd_build" driver work.
///
///   (3) Harness gap: `parse_expect_file` in
///       `tests/end-to-end/src/lib.rs` treats the `ok` sentinel
///       (24 fixtures — 8 of them `m2_*`) as a literal opaque
///       "code" instead of the "expect empty diagnostic set"
///       marker the fixtures document. Every `ok`-sentinel
///       fixture will fail today with `expected {"ok"}, got {}`
///       until the sentinel is interpreted. Fix belongs in the
///       end-to-end lib crate, not this file.
///
/// The macro-driver dependencies previously cited in the ignore
/// reason (#1541 + #1542) have both landed; keep this test
/// `#[ignore]` until (1), (2) and (3) each ship. Do NOT touch this
/// attribute again until each dependency has a landed commit.
#[test]
#[ignore = "PAS-DEBT-B7-001 (#1529): slices B (#1541) + C (#1542) landed the elaborator-side macro grammar + expand_macro (v0.36.65 + v0.36.66); three blockers remain: (1) structured-IR payload emission for S/F/C-code walkers (m2/m5, shared with PAS-DEBT-B7-004, no tracking issue), (2) expand_macro is not yet wired into cmd_build pass pipeline (no call-sites in crates/paideia-as/src/, no tracking issue yet — Phase-2 driver work), (3) harness parse_expect_file in tests/end-to-end/src/lib.rs does not interpret the `ok` sentinel (24 zero-diagnostic fixtures fail with expected={\"ok\"}, got={}). See design/paideia-as-debt-catalog.md §8.2."]
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
