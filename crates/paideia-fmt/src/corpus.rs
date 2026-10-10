//! FMT-13 (paideia-as#1577): corpus idempotence harness helpers.
//!
//! Shared infrastructure for FMT-13 (idempotence), FMT-14
//! (parse-stability), and FMT-15 (round-trip). The core invariant
//! exposed here is:
//!
//! ```ignore
//! format(format(src)) == format(src)
//! ```
//!
//! for every `.pdx` file in the workspace corpus (~1,200 files as of
//! 2026-10-10).
//!
//! # Why a shared helper
//!
//! FMT-14 (parse-stability) walks the same corpus and asserts
//! `parse(format(src))` succeeds whenever `parse(src)` did. FMT-15
//! (round-trip) walks it again to assert `format(src)` tokens match
//! `src` tokens modulo whitespace/comments. Both reuse [`walk_pdx`]
//! for discovery and the shared `IdemReport::Dirty` diff plumbing for
//! failure reporting, so landing the walker + the single-file checker
//! here now avoids a trivial code-duplication across three test
//! binaries later.
//!
//! # FMT-3 fallback interaction
//!
//! As of FMT-3, every per-kind formatter in [`crate::walker`] returns
//! `Doc::text(source[node.span])` and [`crate::format`] falls back
//! verbatim on any lex/parse failure. The idempotence invariant is
//! therefore trivially satisfied today — the whole sweep is a *gate*
//! that will start finding real regressions as the FMT-4..8 structural
//! walkers emit non-verbatim output.
//!
//! # Reject fixtures
//!
//! The workspace carries ~86 reject fixtures under
//! `tests/*/corpus/reject/` plus a handful of named `*_malformed_*`,
//! `parse_error_*`, and `lex_error.pdx` files. They are intentionally
//! malformed so the parser's reject-paths get tested. We do *not*
//! exclude them from the idempotence sweep: the FMT-3 fallback returns
//! them verbatim and idempotence holds trivially; FMT-9/10 may later
//! surface them as `IdemReport::FormatError` once real diagnostics are
//! wired, but the invariant on the string result still holds.

use crate::{format, FormatError, FormatOptions};
use std::io;
use std::path::{Path, PathBuf};

/// Per-file outcome of a single idempotence check.
///
/// The variant tells the caller how to classify this file for the
/// aggregated sweep report; `Dirty` is the only variant that should
/// fail the harness.
#[derive(Debug, Clone)]
pub enum IdemReport {
    /// `format(src)` is a fixed point: `format(format(src)) == format(src)`.
    Clean {
        /// The file whose check succeeded.
        path: PathBuf,
        /// Byte length of the formatted output (useful for sweep stats).
        bytes: usize,
    },
    /// `format(format(src))` disagrees with `format(src)` — the
    /// formatter is not a fixed point on this input. Carries both
    /// strings so the test harness can produce a diff.
    Dirty {
        /// The file whose check failed.
        path: PathBuf,
        /// `format(src)` — the first-pass output.
        once: String,
        /// `format(format(src))` — the second-pass output.
        twice: String,
    },
    /// `format` returned `Err` on this input. Recorded but does NOT
    /// fail the sweep — the FMT-3 contract is `Ok` on every input
    /// today, so a `FormatError` here will surface the day FMT-9/10
    /// starts propagating real diagnostics.
    FormatError {
        /// The file whose format call errored.
        path: PathBuf,
        /// `Debug`-rendered error (kept opaque — `FormatError` is
        /// `#[non_exhaustive]` and we don't want callers matching
        /// on specific variants).
        kind: String,
    },
}

impl IdemReport {
    /// `true` iff this file is a fixed point (`Clean`).
    pub fn is_clean(&self) -> bool {
        matches!(self, IdemReport::Clean { .. })
    }

    /// `true` iff `format(format(src)) != format(src)`.
    pub fn is_dirty(&self) -> bool {
        matches!(self, IdemReport::Dirty { .. })
    }

    /// `true` iff `format` returned `Err`.
    pub fn is_error(&self) -> bool {
        matches!(self, IdemReport::FormatError { .. })
    }

    /// Path to the file this report describes.
    pub fn path(&self) -> &Path {
        match self {
            IdemReport::Clean { path, .. }
            | IdemReport::Dirty { path, .. }
            | IdemReport::FormatError { path, .. } => path,
        }
    }
}

/// Recursively collect every `.pdx` file under `dir`.
///
/// Mirrors `cmd_fmt::walk_pdx_files` deliberately — hidden directories
/// (`.`-prefix) and `target/` are skipped so a workspace walk never
/// descends into build artefacts, `.git/`, or `.cache/`. Returns an
/// alphabetically-sorted vector so sweep reports are deterministic
/// across runs.
///
/// Errors on `read_dir` are silently swallowed (best-effort walk,
/// matching the CLI's own `walk_pdx_files` posture). The caller gets
/// an empty vector on an unreadable directory; the aggregated sweep
/// surfaces that as "scanned: 0".
pub fn walk_pdx(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_pdx_into(dir, &mut out);
    out.sort();
    out
}

fn walk_pdx_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if path.is_dir() {
            walk_pdx_into(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("pdx") {
            out.push(path);
        }
    }
}

/// Check `format(format(src)) == format(src)` on a single file.
///
/// # Errors
///
/// Only I/O errors from `fs::read_to_string` propagate as `Err`.
/// Format failures are classified as [`IdemReport::FormatError`] and
/// returned via `Ok` so the sweep can keep going.
pub fn check_idempotence(path: &Path, opts: &FormatOptions) -> Result<IdemReport, io::Error> {
    let src = std::fs::read_to_string(path)?;
    let once = match format(&src, opts) {
        Ok(s) => s,
        Err(e) => return Ok(format_error(path, &e)),
    };
    let twice = match format(&once, opts) {
        Ok(s) => s,
        Err(e) => return Ok(format_error(path, &e)),
    };
    if once == twice {
        Ok(IdemReport::Clean {
            path: path.to_path_buf(),
            bytes: once.len(),
        })
    } else {
        Ok(IdemReport::Dirty {
            path: path.to_path_buf(),
            once,
            twice,
        })
    }
}

fn format_error(path: &Path, err: &FormatError) -> IdemReport {
    IdemReport::FormatError {
        path: path.to_path_buf(),
        kind: format!("{err:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn walk_pdx_finds_nested_files_and_sorts() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/z.pdx"), "let x = 1\n").unwrap();
        fs::write(root.join("a/alpha.pdx"), "let y = 2\n").unwrap();
        fs::write(root.join("not_pdx.txt"), "skip me\n").unwrap();

        let got = walk_pdx(root);
        assert_eq!(got.len(), 2);
        // Sorted alphabetically: alpha.pdx before b/z.pdx.
        assert!(got[0].ends_with("alpha.pdx"));
        assert!(got[1].ends_with("z.pdx"));
    }

    #[test]
    fn walk_pdx_skips_hidden_dirs_and_target() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".git/HEAD.pdx"), "should_skip\n").unwrap();
        fs::write(root.join("target/debug/x.pdx"), "should_skip\n").unwrap();
        fs::write(root.join("src/keep.pdx"), "let x = 1\n").unwrap();

        let got = walk_pdx(root);
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with("keep.pdx"));
    }

    #[test]
    fn check_idempotence_clean_on_simple_let() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("simple.pdx");
        fs::write(&file, "pub let x : u64 = 1\n").unwrap();

        let r = check_idempotence(&file, &FormatOptions::default()).unwrap();
        assert!(r.is_clean(), "expected Clean, got {r:?}");
        assert_eq!(r.path(), file.as_path());
    }

    #[test]
    fn check_idempotence_clean_on_parse_error() {
        // FMT-3 fallback returns malformed input verbatim, so idempotence
        // holds trivially — exactly the reject-corpus contract.
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("broken.pdx");
        fs::write(&file, "let 123\n").unwrap();

        let r = check_idempotence(&file, &FormatOptions::default()).unwrap();
        assert!(r.is_clean(), "expected Clean (fallback verbatim), got {r:?}");
    }

    #[test]
    fn check_idempotence_io_error_propagates() {
        let bogus = Path::new("/this/path/does/not/exist.pdx");
        let r = check_idempotence(bogus, &FormatOptions::default());
        assert!(r.is_err());
    }
}
