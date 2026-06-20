//! `cargo test --test runner -p paideia-opt-pool-constants` runs the pool-constants corpus.

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

/// Pool-constants corpus validation: count fixture files in corpus/
/// to ensure the test harness has the expected 1 fixture.
#[test]
fn pool_constants_corpus_has_one_fixture() {
    let dir = corpus_root().join("corpus");
    let files = collect_pdx_files(&dir);
    assert_eq!(
        files.len(),
        1,
        "Expected 1 pool-constants corpus fixture, found {}",
        files.len()
    );
}
