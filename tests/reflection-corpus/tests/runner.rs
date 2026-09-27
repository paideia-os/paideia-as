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
///
/// PAS-DEBT-B7-002: staying `#[ignore]`'d. Current reject corpus is unfit:
///   - 4 fixtures (`r_macro_no_matching_rule`, `r_pattern_match_failure`,
///     `r_recursion_depth`, `r_unbound_metavariable`) are placeholder `.pdx`
///     files (`let m = 1`) with `.expect` sidecars carrying only a comment.
///     They need real macro invocations that fire M0308/M0309/M0311, which
///     is blocked on the m3 macro-match/expand driver.
///   - 4 fixtures (`r_antiquote_outside_quote`, `r_finally_not_last`,
///     `r_malformed_quote`, `r_unknown_fragment_kind`) target P-category
///     codes (P0170/P0162/P0171/P0110). The comparator only extracts
///     M-codes, so these belong in a parser-reject corpus, not here.
/// Close the ticket as blocked on m3 driver + fixture authoring.
#[test]
#[ignore = "reject fixtures are placeholder modules or P-code targets; needs \
    authored macro invocations after m3 driver lands (M0308/M0309/M0311) and \
    relocation of P-code fixtures to a parser-reject corpus. \
    See PAS-DEBT-B7-002."]
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
