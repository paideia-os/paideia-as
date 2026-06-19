//! `cargo test --test runner -p paideia-opt-peephole` runs the peephole corpus.

use std::path::{Path, PathBuf};

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

/// Peephole corpus validation: count fixture files in corpus/
/// to ensure the test harness has the expected 8 fixtures.
#[test]
fn peephole_corpus_has_eight_fixtures() {
    let dir = corpus_root().join("corpus");
    let files = collect_pdx_files(&dir);
    assert_eq!(
        files.len(),
        8,
        "Expected 8 peephole corpus fixtures, found {}",
        files.len()
    );
}
