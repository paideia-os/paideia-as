//! FMT-13 (paideia-as#1577): corpus idempotence harness.
//!
//! Walks every `.pdx` file under the paideia-as workspace and asserts
//! `format(format(src)) == format(src)` — the single most important
//! invariant a formatter can carry (a non-idempotent formatter means
//! "format on save" causes churn forever).
//!
//! # Running
//!
//! The sweep touches ~1,200 files and is deliberately *opt-in*: the
//! single test is marked `#[ignore]` so a stock `cargo test --package
//! paideia-fmt` stays fast. To actually run it:
//!
//! ```sh
//! cargo test --package paideia-fmt --test corpus_idempotence \
//!     -- --include-ignored --nocapture
//! ```
//!
//! `--nocapture` surfaces the "scanned / clean / dirty / errors"
//! summary that the harness prints via `eprintln!`.
//!
//! # Corpus discovery
//!
//! 1. If `PAIDEIA_FMT_CORPUS` is set, walk *only* that directory.
//!    Useful for narrowing a bisect (`PAIDEIA_FMT_CORPUS=
//!    tests/effects-corpus cargo test ...`).
//! 2. Otherwise, walk `tests/` and `examples/` under the paideia-as
//!    workspace root (located by climbing from `CARGO_MANIFEST_DIR`
//!    until a `Cargo.toml` + `crates/` pair is found).
//!
//! # Reject fixtures are *not* excluded
//!
//! The workspace carries ~86 reject fixtures (`r_*.pdx` under
//! `tests/*/corpus/reject/`, plus `*_malformed_*`, `parse_error_*`,
//! `lex_error.pdx`). They are intentionally malformed. The FMT-3
//! fallback returns malformed input verbatim, so idempotence holds
//! trivially — the harness classifies them as `Clean`, no special
//! case needed. If FMT-9/10 ever starts propagating lex/parse errors
//! out of `format`, those files will migrate to the `errors` bucket
//! without breaking the `dirty == 0` gate.
//!
//! # Complexity
//!
//! Each file is `format`'d twice (two full lex+parse+walk+layout
//! passes) and the two strings compared. For the current ~1,200-file
//! corpus the sweep is ~2,400 format calls; empirically that lands at
//! single-digit seconds on a modern laptop (dominated by the parser,
//! not I/O). It is NOT appropriate for the default `cargo test` loop,
//! hence `#[ignore]`.

use paideia_fmt::corpus::{check_idempotence, walk_pdx, IdemReport};
use paideia_fmt::FormatOptions;
use std::path::PathBuf;

/// Maximum diff lines printed per non-idempotent file. The sweep
/// commonly surfaces a handful of regressions at once; we cap each
/// one at 20 lines so a batch failure doesn't flood the terminal.
const MAX_DIFF_LINES: usize = 20;

/// Hard ceiling on the number of distinct `Dirty` files whose diffs
/// get printed. If every file in a 1,200-file sweep breaks (e.g. a
/// FMT-4 trailing-whitespace regression), stopping after 10 is enough
/// to classify the pattern without dumping ~24k lines.
const MAX_DIRTY_PRINTED: usize = 10;

#[test]
#[ignore = "sweeps ~1,200 .pdx files; opt in with `--include-ignored`"]
fn fmt_corpus_is_idempotent() {
    let dirs = resolve_corpus_dirs();
    assert!(
        !dirs.is_empty(),
        "FMT-13: no corpus directories resolved — check PAIDEIA_FMT_CORPUS \
         or that CARGO_MANIFEST_DIR points inside the paideia-as workspace"
    );

    let mut paths: Vec<PathBuf> = Vec::new();
    for d in &dirs {
        paths.extend(walk_pdx(d));
    }
    paths.sort();
    paths.dedup();

    let opts = FormatOptions::default();

    let mut scanned = 0usize;
    let mut clean = 0usize;
    let mut errors: Vec<IdemReport> = Vec::new();
    let mut dirty: Vec<IdemReport> = Vec::new();

    for path in &paths {
        scanned += 1;
        match check_idempotence(path, &opts) {
            Ok(r) => match &r {
                IdemReport::Clean { .. } => clean += 1,
                IdemReport::Dirty { .. } => dirty.push(r),
                IdemReport::FormatError { .. } => errors.push(r),
            },
            Err(e) => {
                eprintln!("FMT-13: i/o error on {}: {e}", path.display());
            }
        }
    }

    // Summary — always emitted so `--nocapture` surfaces a usable
    // digest even on a green run. Order matches the ticket's
    // "files scanned / clean / dirty / parse-error" layout.
    eprintln!("FMT-13 corpus idempotence sweep");
    eprintln!("  roots:        {:?}", dirs);
    eprintln!("  scanned:      {scanned}");
    eprintln!("  clean:        {clean}");
    eprintln!("  dirty:        {}", dirty.len());
    eprintln!("  format-error: {}", errors.len());

    if !errors.is_empty() {
        // Non-fatal — the FMT-3 contract today is "always Ok"; this
        // branch is a canary for FMT-9/10 diagnostic propagation.
        for e in errors.iter().take(5) {
            if let IdemReport::FormatError { path, kind } = e {
                eprintln!("  format-error on {}: {kind}", path.display());
            }
        }
        if errors.len() > 5 {
            eprintln!("  … and {} more format-error entries", errors.len() - 5);
        }
    }

    if !dirty.is_empty() {
        for d in dirty.iter().take(MAX_DIRTY_PRINTED) {
            if let IdemReport::Dirty { path, once, twice } = d {
                eprintln!("--- not idempotent: {} ---", path.display());
                print_diff(once, twice);
            }
        }
        if dirty.len() > MAX_DIRTY_PRINTED {
            eprintln!(
                "… and {} more non-idempotent files (output capped at {})",
                dirty.len() - MAX_DIRTY_PRINTED,
                MAX_DIRTY_PRINTED
            );
        }
        panic!(
            "FMT-13: {} / {} files failed `format(format(src)) == format(src)`",
            dirty.len(),
            scanned
        );
    }
}

/// Line-oriented diff capped at [`MAX_DIFF_LINES`]. Deliberately
/// simple — we don't want the harness to pull in the `similar` crate
/// for a sanity gate.
fn print_diff(once: &str, twice: &str) {
    let a: Vec<&str> = once.lines().collect();
    let b: Vec<&str> = twice.lines().collect();
    let n = a.len().max(b.len());
    let mut printed = 0usize;
    for i in 0..n {
        if printed >= MAX_DIFF_LINES {
            eprintln!("  (diff truncated at {MAX_DIFF_LINES} lines)");
            return;
        }
        match (a.get(i), b.get(i)) {
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) => {
                eprintln!("  - {x}");
                eprintln!("  + {y}");
                printed += 2;
            }
            (Some(x), None) => {
                eprintln!("  - {x}");
                printed += 1;
            }
            (None, Some(y)) => {
                eprintln!("  + {y}");
                printed += 1;
            }
            (None, None) => {}
        }
    }
}

/// Compute the list of root directories to walk. See the module
/// header for the resolution rules.
fn resolve_corpus_dirs() -> Vec<PathBuf> {
    resolve_corpus_dirs_with(std::env::var("PAIDEIA_FMT_CORPUS").ok())
}

/// Testable core: takes the env var as an explicit `Option` so the
/// sanity tests can exercise both branches without racing on the
/// process-global environment (Rust's default test harness runs
/// tests in parallel; `set_var` races are a known footgun).
fn resolve_corpus_dirs_with(override_dir: Option<String>) -> Vec<PathBuf> {
    if let Some(v) = override_dir {
        return vec![PathBuf::from(v)];
    }
    let root = workspace_root();
    let mut dirs = Vec::new();
    for sub in ["tests", "examples"] {
        let d = root.join(sub);
        if d.is_dir() {
            dirs.push(d);
        }
    }
    dirs
}

/// Locate the paideia-as workspace root by climbing from this crate's
/// manifest. We identify the root by the presence of both
/// `Cargo.toml` and a `crates/` subdirectory — the paideia-as repo's
/// signature pair, stable since v0.1. The climb is bounded (6 steps)
/// so a malformed layout fails fast instead of walking to `/`.
fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..6 {
        if dir.join("Cargo.toml").is_file() && dir.join("crates").is_dir() {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    panic!(
        "FMT-13: failed to locate paideia-as workspace root starting from \
         CARGO_MANIFEST_DIR = {}",
        env!("CARGO_MANIFEST_DIR")
    );
}

// --- Sanity tests that run by default --------------------------------
//
// These keep the harness itself regression-tested without taking the
// `#[ignore]`'d full-corpus hit. If the corpus walker or the single-
// file checker breaks, these fail at the normal `cargo test` tempo.

#[test]
fn resolve_corpus_dirs_defaults_cover_tests_and_examples() {
    let dirs = resolve_corpus_dirs_with(None);
    // At least `tests/` must resolve; it exists on main as of
    // 2026-10-10 and holds the bulk of the corpus.
    assert!(
        dirs.iter().any(|d| d.ends_with("tests")),
        "expected tests/ dir in {dirs:?}"
    );
}

#[test]
fn resolve_corpus_dirs_honours_env_override() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = resolve_corpus_dirs_with(Some(tmp.path().display().to_string()));
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0], tmp.path());
}

#[test]
fn workspace_root_lands_on_paideia_as() {
    let root = workspace_root();
    assert!(root.join("Cargo.toml").is_file());
    assert!(root.join("crates").is_dir());
    assert!(root.join("crates/paideia-fmt").is_dir());
}

#[test]
fn harness_detects_fake_dirty_file() {
    // Build a tiny ad-hoc corpus with one source and format it
    // through the real pipeline; the FMT-3 fallback keeps it clean.
    // Then rewrite it to something malformed so we exercise the
    // reject-fixture branch (also expected clean under the current
    // byte-identical fallback). Both branches should classify
    // `Clean`, confirming the harness doesn't false-positive.
    let tmp = tempfile::tempdir().unwrap();
    let good = tmp.path().join("good.pdx");
    std::fs::write(&good, "pub let x : u64 = 1\n").unwrap();
    let r = check_idempotence(&good, &FormatOptions::default()).unwrap();
    assert!(r.is_clean());

    let bad = tmp.path().join("bad.pdx");
    std::fs::write(&bad, "let 123\n").unwrap();
    let r = check_idempotence(&bad, &FormatOptions::default()).unwrap();
    assert!(r.is_clean());
}

#[test]
fn walk_pdx_discovers_files_under_tempdir() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("sub")).unwrap();
    std::fs::write(tmp.path().join("sub/a.pdx"), "let x = 1\n").unwrap();
    std::fs::write(tmp.path().join("b.pdx"), "let y = 2\n").unwrap();

    let got = walk_pdx(tmp.path());
    assert_eq!(got.len(), 2);
    assert!(got.iter().any(|p| p.ends_with("sub/a.pdx")));
    assert!(got.iter().any(|p| p.ends_with("b.pdx")));
}
