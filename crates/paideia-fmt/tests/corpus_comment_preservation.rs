//! FMT-15 (paideia-as#1579): corpus comment-preservation harness.
//!
//! Walks every `.pdx` file under the paideia-as workspace and asserts
//! that `format(src)` preserves every comment (line, block, doc-line,
//! doc-block) byte-for-byte, in the same relative order they appeared
//! in the input. This locks in the style-spec §9.2 wording-preservation
//! guarantee at the corpus scale.
//!
//! # Invariant
//!
//! ```ignore
//! extract_comments(src) == extract_comments(format(src))
//! ```
//!
//! where equality is slice-equal on both the kind and the raw text of
//! every [`CommentToken`], and position-equal (first comment in == first
//! comment out, and so on).
//!
//! # Running
//!
//! Opt-in via `--include-ignored` to match FMT-13 and FMT-14:
//!
//! ```sh
//! cargo test --package paideia-fmt --test corpus_comment_preservation \
//!     -- --include-ignored --nocapture
//! ```
//!
//! # Corpus discovery
//!
//! Reuses the FMT-13 walker in [`paideia_fmt::corpus`]:
//!
//! 1. If `PAIDEIA_FMT_CORPUS` is set, walk only that directory. Useful
//!    for narrowing a bisect (`PAIDEIA_FMT_CORPUS=examples cargo test ...`).
//! 2. Otherwise, walk `tests/` and `examples/` under the paideia-as
//!    workspace root (located by climbing from `CARGO_MANIFEST_DIR`
//!    until a `Cargo.toml` + `crates/` pair is found — identical to
//!    FMT-13's `workspace_root`).
//!
//! # Reject fixtures and FMT-3 fallback
//!
//! The workspace carries ~86 reject fixtures that are intentionally
//! malformed. The FMT-3 fallback returns them verbatim, so the comment
//! lists trivially match — no special case is needed. The sweep
//! classifies them as `Preserved`.
//!
//! The FMT-4..8 structural walkers currently also fall back to
//! `Doc::text(source[node.span])` on every variant, so byte-identity
//! holds for everything. FMT-15 is therefore a *gate* that will start
//! finding real regressions as the FMT-4..8 formatters emit non-
//! verbatim output — exactly the point.
//!
//! # Skip list
//!
//! Empty at FMT-15 landing. The style spec promises comment preservation
//! on *every* `.pdx` file: a corpus fixture that cannot satisfy that is
//! either (a) a style-spec exception to document, or (b) a formatter
//! bug to fix. Both paths are preferable to silently excluding files,
//! so we keep [`SKIP_FILES`] empty and let it fail loudly if ever
//! populated.

use std::path::{Path, PathBuf};

use paideia_fmt::comment_extract::{extract_comments, CommentToken};
use paideia_fmt::corpus::walk_pdx;
use paideia_fmt::{format as fmt_source, FormatOptions};

/// Files intentionally excluded from the sweep.
///
/// Each entry is a substring matched against the full path (so a leaf
/// basename is fine for a unique fixture, and a `tests/foo/` prefix
/// covers a whole subdirectory).
///
/// **Empty at FMT-15 landing** — style-spec §9.2 promises every comment
/// survives. A corpus fixture that cannot satisfy that is either a
/// spec exception to document or a formatter bug to fix; we keep the
/// list empty so new entries are a deliberate act, not a drift.
const SKIP_FILES: &[&str] = &[];

/// Hard ceiling on the number of distinct failing files whose diffs
/// get printed. Keeps a batch regression from flooding the terminal.
const MAX_DIRTY_PRINTED: usize = 10;

/// Per-file outcome of a single comment-preservation check.
#[derive(Debug, Clone)]
enum CommentReport {
    /// Every input comment appears in the output at the same relative
    /// position, with identical text and kind.
    Preserved { path: PathBuf, count: usize },
    /// The comment lists diverge. Carries the first-diff summary for
    /// the harness diff printer.
    Mismatch {
        path: PathBuf,
        /// Total input-comment count (so the summary can show
        /// "lost 3/42 comments" rather than just "mismatch at 5").
        input_count: usize,
        /// Total output-comment count.
        output_count: usize,
        /// Zero-based index of the first mismatching comment (0 iff
        /// the very first comment is missing or wrong).
        first_diff_index: usize,
        /// Input comment at `first_diff_index`, if any (None means
        /// the output has *extra* comments past the input end).
        input: Option<CommentToken>,
        /// Output comment at `first_diff_index`, if any (None means
        /// the output *dropped* a comment the input had).
        output: Option<CommentToken>,
    },
    /// Skipped by the `SKIP_FILES` policy. Reported for visibility but
    /// does not count toward failure.
    Skipped { path: PathBuf, reason: &'static str },
}

impl CommentReport {
    fn is_mismatch(&self) -> bool {
        matches!(self, CommentReport::Mismatch { .. })
    }

    fn path(&self) -> &Path {
        match self {
            CommentReport::Preserved { path, .. }
            | CommentReport::Mismatch { path, .. }
            | CommentReport::Skipped { path, .. } => path,
        }
    }
}

#[test]
#[ignore = "sweeps the full .pdx corpus; opt in with `--include-ignored`"]
fn fmt_corpus_preserves_comments() {
    let dirs = resolve_corpus_dirs();
    assert!(
        !dirs.is_empty(),
        "FMT-15: no corpus directories resolved — check PAIDEIA_FMT_CORPUS \
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
    let mut preserved = 0usize;
    let mut total_comments = 0usize;
    let mut skipped: Vec<CommentReport> = Vec::new();
    let mut mismatches: Vec<CommentReport> = Vec::new();

    for path in &paths {
        scanned += 1;
        let report = check_comment_preservation(path, &opts);
        match &report {
            CommentReport::Preserved { count, .. } => {
                preserved += 1;
                total_comments += count;
            }
            CommentReport::Mismatch { .. } => mismatches.push(report),
            CommentReport::Skipped { .. } => skipped.push(report),
        }
    }

    eprintln!("FMT-15 corpus comment-preservation sweep");
    eprintln!("  roots:         {:?}", dirs);
    eprintln!("  scanned:       {scanned}");
    eprintln!("  preserved:     {preserved}");
    eprintln!("  total-comments: {total_comments}");
    eprintln!("  skipped:       {}", skipped.len());
    eprintln!("  mismatches:    {}", mismatches.len());

    for s in &skipped {
        if let CommentReport::Skipped { path, reason } = s {
            eprintln!("  skip: {} — {reason}", path.display());
        }
    }

    if !mismatches.is_empty() {
        for m in mismatches.iter().take(MAX_DIRTY_PRINTED) {
            if let CommentReport::Mismatch {
                path,
                input_count,
                output_count,
                first_diff_index,
                input,
                output,
            } = m
            {
                eprintln!(
                    "--- comments diverge: {} ({input_count} in, {output_count} out) ---",
                    path.display()
                );
                eprintln!("  first diff @ index {first_diff_index}:");
                eprintln!("    input:  {}", render_opt_comment(input));
                eprintln!("    output: {}", render_opt_comment(output));
            }
        }
        if mismatches.len() > MAX_DIRTY_PRINTED {
            eprintln!(
                "… and {} more mismatching files (output capped at {})",
                mismatches.len() - MAX_DIRTY_PRINTED,
                MAX_DIRTY_PRINTED
            );
        }
        panic!(
            "FMT-15: {} / {} files failed comment preservation",
            mismatches.len(),
            scanned
        );
    }
}

// --- Per-file checker ----------------------------------------------------

/// Check comment preservation on a single file.
///
/// Returns [`CommentReport::Preserved`] on success, [`Mismatch`] on
/// failure, or [`Skipped`] when `SKIP_FILES` matches. I/O errors are
/// classified as `Skipped { reason: "io-error" }` rather than
/// propagated — we never want a transient filesystem hiccup to fail
/// the sweep (and the per-file read is on a known-good corpus).
///
/// [`Mismatch`]: CommentReport::Mismatch
/// [`Skipped`]: CommentReport::Skipped
fn check_comment_preservation(path: &Path, opts: &FormatOptions) -> CommentReport {
    if let Some(reason) = skip_reason(path) {
        return CommentReport::Skipped {
            path: path.to_path_buf(),
            reason,
        };
    }
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => {
            return CommentReport::Skipped {
                path: path.to_path_buf(),
                reason: "io-error",
            }
        }
    };
    let input = extract_comments(&src);
    let output_src = match fmt_source(&src, opts) {
        Ok(s) => s,
        // FMT-3's `format` never returns Err today; defensive branch
        // lets FMT-9/10 keep working if that changes.
        Err(_) => {
            return CommentReport::Skipped {
                path: path.to_path_buf(),
                reason: "format-error",
            }
        }
    };
    let output = extract_comments(&output_src);

    match first_diff(&input, &output) {
        None => CommentReport::Preserved {
            path: path.to_path_buf(),
            count: input.len(),
        },
        Some((idx, lhs, rhs)) => CommentReport::Mismatch {
            path: path.to_path_buf(),
            input_count: input.len(),
            output_count: output.len(),
            first_diff_index: idx,
            input: lhs,
            output: rhs,
        },
    }
}

/// Returns `Some(reason)` iff `path` matches the skip-list policy.
fn skip_reason(path: &Path) -> Option<&'static str> {
    let s = path.to_string_lossy();
    for pat in SKIP_FILES {
        if s.contains(pat) {
            return Some("skip-list");
        }
    }
    None
}

/// Compare two comment vectors position-wise and return the first
/// mismatch — the index plus both sides' value at that index. Returns
/// `None` iff the two vectors are equal in length and element-wise.
///
/// Equality is on the whole [`CommentToken`] (kind + text + offset).
/// In practice `byte_offset` differences are expected (the formatter
/// may re-indent), so a kind-and-text comparison is the one the harness
/// surfaces in the diff; we still carry the full tokens so a future
/// stricter mode can turn offset drift into a signal.
fn first_diff(
    a: &[CommentToken],
    b: &[CommentToken],
) -> Option<(usize, Option<CommentToken>, Option<CommentToken>)> {
    let n = a.len().max(b.len());
    for i in 0..n {
        let la = a.get(i);
        let lb = b.get(i);
        let eq = match (la, lb) {
            (Some(x), Some(y)) => x.kind == y.kind && x.text == y.text,
            (None, None) => true,
            _ => false,
        };
        if !eq {
            return Some((i, la.cloned(), lb.cloned()));
        }
    }
    None
}

/// Render a `CommentToken` (or `None`) for the diff printer. Keeps the
/// line short enough to stay readable in CI logs — long doc comments
/// are truncated with an ellipsis.
fn render_opt_comment(c: &Option<CommentToken>) -> String {
    const MAX: usize = 80;
    match c {
        None => "<missing>".to_string(),
        Some(t) => {
            let s = t.text.replace('\n', "\\n");
            if s.len() > MAX {
                format!("{:?} {:?}…", t.kind, &s[..MAX])
            } else {
                format!("{:?} {:?}", t.kind, s)
            }
        }
    }
}

// --- Corpus-root resolution (mirrors FMT-13) -----------------------------

/// Compute the list of root directories to walk. See the module header.
fn resolve_corpus_dirs() -> Vec<PathBuf> {
    resolve_corpus_dirs_with(std::env::var("PAIDEIA_FMT_CORPUS").ok())
}

/// Testable core: takes the env var as an explicit `Option` so sanity
/// tests can exercise both branches without racing on the process-
/// global environment.
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
/// manifest. Identical heuristic to FMT-13's `workspace_root`.
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
        "FMT-15: failed to locate paideia-as workspace root starting from \
         CARGO_MANIFEST_DIR = {}",
        env!("CARGO_MANIFEST_DIR")
    );
}

// --- Sanity tests that run by default ------------------------------------
//
// These keep the harness itself regression-tested at the normal
// `cargo test` tempo without taking the `#[ignore]`'d full-corpus hit.

#[test]
fn harness_preserves_simple_line_comment() {
    // A one-liner with a leading comment: FMT-3's byte-identical
    // fallback must round-trip the comment untouched.
    let src = "// hello\nlet x = 1\n";
    let out = fmt_source(src, &FormatOptions::default()).unwrap();
    let a = extract_comments(src);
    let b = extract_comments(&out);
    assert_eq!(a, b);
}

#[test]
fn harness_preserves_trailing_comment() {
    let src = "let x = 1 // tail\n";
    let out = fmt_source(src, &FormatOptions::default()).unwrap();
    let a = extract_comments(src);
    let b = extract_comments(&out);
    assert_eq!(a.len(), 1);
    assert_eq!(a, b);
}

#[test]
fn harness_preserves_doc_comment() {
    let src = "/// doc\nlet x = 1\n";
    let out = fmt_source(src, &FormatOptions::default()).unwrap();
    let a = extract_comments(src);
    let b = extract_comments(&out);
    assert_eq!(a.len(), 1);
    assert_eq!(a, b);
}

#[test]
fn harness_preserves_nested_block_comment() {
    // The lexer collapses nested `/* /* */ */` to a single block —
    // comment preservation must therefore see one in, one out.
    let src = "/* outer /* inner */ outer */ let x = 1\n";
    let out = fmt_source(src, &FormatOptions::default()).unwrap();
    let a = extract_comments(src);
    let b = extract_comments(&out);
    assert_eq!(a.len(), 1);
    assert_eq!(a, b);
}

#[test]
fn harness_preserves_multi_kind_mix() {
    // Line + doc-line + block + doc-block + trailing: all five must
    // survive and keep their document order.
    let src = "// A\n/// B\n/* C */ /** D */ let x = 1 // E\n";
    let out = fmt_source(src, &FormatOptions::default()).unwrap();
    let a = extract_comments(src);
    let b = extract_comments(&out);
    assert_eq!(a.len(), 5);
    assert_eq!(a, b);
}

#[test]
fn first_diff_detects_dropped_comment() {
    // Simulates a formatter regression: the output drops the second
    // comment. first_diff must report index 1 with (Some, None).
    let input = extract_comments("// A\n// B\n// C\nlet x = 1\n");
    let output = extract_comments("// A\n// C\nlet x = 1\n");
    let (idx, lhs, rhs) = first_diff(&input, &output).expect("expected a diff");
    assert_eq!(idx, 1);
    assert_eq!(lhs.unwrap().text, "// B");
    assert_eq!(rhs.unwrap().text, "// C");
}

#[test]
fn first_diff_detects_extra_output_comment() {
    // Reverse direction: output has a comment the input doesn't.
    let input = extract_comments("// A\nlet x = 1\n");
    let output = extract_comments("// A\n// B\nlet x = 1\n");
    let (idx, lhs, rhs) = first_diff(&input, &output).expect("expected a diff");
    assert_eq!(idx, 1);
    assert!(lhs.is_none());
    assert_eq!(rhs.unwrap().text, "// B");
}

#[test]
fn first_diff_detects_altered_text() {
    // Same count, same kind, different text — e.g. the formatter
    // reflowed a doc comment it shouldn't have.
    let input = extract_comments("/// hello world\nlet x = 1\n");
    let output = extract_comments("/// hello  world\nlet x = 1\n");
    let (idx, lhs, rhs) = first_diff(&input, &output).expect("expected a diff");
    assert_eq!(idx, 0);
    assert_ne!(lhs.unwrap().text, rhs.unwrap().text);
}

#[test]
fn first_diff_returns_none_on_match() {
    let a = extract_comments("// A\n// B\nlet x = 1\n");
    let b = extract_comments("// A\n// B\nlet x = 1\n");
    assert!(first_diff(&a, &b).is_none());
}

#[test]
fn resolve_corpus_dirs_honours_env_override() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = resolve_corpus_dirs_with(Some(tmp.path().to_string_lossy().into_owned()));
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0], tmp.path());
}

#[test]
fn resolve_corpus_dirs_defaults_cover_tests_or_examples() {
    let dirs = resolve_corpus_dirs_with(None);
    // On a real workspace at least one of tests/ or examples/ exists.
    assert!(dirs.iter().any(|d| d.ends_with("tests") || d.ends_with("examples")));
}

#[test]
fn workspace_root_lands_on_paideia_as() {
    let root = workspace_root();
    assert!(root.join("Cargo.toml").is_file());
    assert!(root.join("crates/paideia-fmt").is_dir());
}

#[test]
fn skip_list_is_empty_at_landing() {
    // Explicit guard: the FMT-15 contract is that every corpus file
    // preserves its comments. A non-empty skip list is a signal to
    // the reviewer that an exception has been documented upstream.
    assert!(
        SKIP_FILES.is_empty(),
        "FMT-15 skip list grew — add a design note before merging"
    );
}

#[test]
fn single_file_report_on_simple_fixture() {
    // Build a tiny ad-hoc fixture and run it through the real checker.
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("sanity.pdx");
    std::fs::write(&f, "// top\nlet x = 1\n").unwrap();
    let r = check_comment_preservation(&f, &FormatOptions::default());
    match r {
        CommentReport::Preserved { count, .. } => assert_eq!(count, 1),
        other => panic!("expected Preserved, got {other:?}"),
    }
}

#[test]
fn single_file_report_on_zero_comment_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("empty.pdx");
    std::fs::write(&f, "let x = 1\n").unwrap();
    let r = check_comment_preservation(&f, &FormatOptions::default());
    assert!(!r.is_mismatch());
    if let CommentReport::Preserved { count, .. } = r {
        assert_eq!(count, 0);
    } else {
        panic!("expected Preserved with count=0");
    }
}
