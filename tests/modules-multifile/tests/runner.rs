//! `cargo test --test runner -p paideia-modules-multifile` runs the modules corpus.
//!
//! TODO(m5-013): cross-file import resolution lands at signature-registry layer.

use std::path::{Path, PathBuf};

use paideia_modules_multifile::codes_for;

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

/// Corpus test: each `.pdx` file in `corpus/` is validated individually
/// for correct file-to-module mapping (module name matches file basename
/// in PascalCase, exactly one module per file).
///
/// Currently, each file validates independently. Cross-file import resolution
/// (m5-013+) will add module-linkage tests later.
#[test]
fn corpus_validates_file_module_mapping() {
    let dir = corpus_root().join("corpus");
    let files = collect_pdx_files(&dir);
    let mut failures = Vec::new();

    for path in &files {
        match codes_for(path) {
            Ok(codes) if codes.is_empty() => {
                // Expected: each file validates with no M-codes emitted.
            }
            Ok(codes) => failures.push(format!(
                "{}: expected no module-system codes, got {:?}",
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
        "{} corpus files failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
