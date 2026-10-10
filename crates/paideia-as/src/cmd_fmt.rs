//! `paideia-as fmt` — source code formatter.
//!
//! Delegates to paideia-fmt for all formatting logic. Supports:
//! - Reading from one or more files (default) or stdin (--stdin).
//! - Shell-style glob expansion for positional args containing `*`, `?`, or `[`
//!   (FMT-12 / paideia-as#1576). Literal paths pass through unchanged, so
//!   `fmt a.pdx b.pdx` and `fmt 'src/**/*.pdx'` share one entry point.
//! - Workspace default (FMT-12): with no positional args and no `--stdin`,
//!   walks upward from CWD looking for a `.pdxfmt` marker (preferred) or a
//!   `Cargo.toml` (fallback) and formats every `.pdx` under `src/` + `tests/`
//!   of that root. This mirrors `rustfmt`'s "no args = workspace fmt" ergonomic.
//! - Writing in-place to each file (default) or stdout (--stdin).
//! - Checking mode (--check): exits 1 if any file's formatted output differs from input.
//! - Diff mode (--diff): displays a unified diff per changed file to stdout;
//!   exits 1 if any file's formatted output differs.
//! - List-different mode (--list-different, FMT-12): prints the path of each
//!   file whose formatted output would differ, one per line to stdout. Unlike
//!   `--check`, exits 0 regardless of whether anything would change — the
//!   designed consumer is a pipeline (`paideia-as fmt --list-different | xargs ...`).
//!
//! Multi-file semantics (#1126):
//! - All files are processed. A per-file read/write failure emits a diagnostic
//!   to stderr and yields exit code 2 at end-of-run, but does not stop processing.
//! - --check and --diff exit code is 1 if ANY file would change (2 if any I/O error).
//! - --list-different exit code is 0 for any clean/change outcome; still 2 on I/O error.
//!
//! Glob corner-cases (deliberately leaning on the `glob` crate's defaults):
//! - Hidden files/directories (`.`-prefix): glob 0.3 defaults to
//!   `require_literal_leading_dot = true`, so `src/**/*.pdx` will NOT descend
//!   into `.git/`, `.cache/`, etc. This matches shell `shopt -u dotglob`
//!   semantics and is the behaviour almost every `rustfmt --all` user expects.
//! - Symlinks: `glob::glob` follows symlinks when matching. We do NOT dedupe
//!   via canonicalization — if a user's layout links `src/pkg_a -> src/pkg_b`
//!   the same file may be formatted twice (idempotent on the second pass, so
//!   benign). Workspace-default enumeration (`collect_workspace_pdx_files`)
//!   likewise walks what `std::fs::read_dir` returns without resolving symlinks.
//! - Literal paths that don't exist: pass through unchanged and surface the
//!   usual "failed to read" diagnostic + exit 2 in `run_one_file`.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

// `cmd fmt` still delegates to the legacy whitespace normaliser
// (paideia-as#1565: new Doc-based `format()` lands in FMT-10 once FMT-4..8
// per-kind formatters produce structural output). Import under the
// local alias `format` so the rest of the module reads unchanged.
use paideia_fmt::{FormatOptions, format_whitespace as format};
use similar::TextDiff;

/// Marker file that pins a `.pdx` workspace root. Preferred over `Cargo.toml`
/// so a user working inside the paideia-as monorepo can override the Rust
/// workspace root (which lives at `tools/paideia-as/Cargo.toml`) by dropping
/// an empty `.pdxfmt` at the actual project root they want `fmt` to span.
const PDXFMT_MARKER: &str = ".pdxfmt";

/// Fallback workspace marker when no `.pdxfmt` is found on the ancestor chain.
const CARGO_MARKER: &str = "Cargo.toml";

/// Subdirectories (relative to the discovered workspace root) that the
/// workspace-default enumerator walks for `.pdx` sources.
const WORKSPACE_SRC_DIRS: &[&str] = &["src", "tests"];

/// Run `paideia-as fmt [--stdin] [--check] [--diff] [--list-different] [PATHS..]`.
///
/// `inputs` is a list of positional args as typed on the command line — each
/// entry may be either a literal path or a shell-style glob (expansion gated
/// on the presence of `*`, `?`, or `[`). The empty-args case triggers
/// workspace-default enumeration (`collect_workspace_pdx_files`).
pub fn run(inputs: &[String], check: bool, diff: bool, stdin: bool, list_different: bool) -> ExitCode {
    if stdin {
        return run_stdin(check, diff, list_different);
    }

    // Expand positional args into a flat PathBuf list. Globs expand; literal
    // paths pass through. On empty `inputs`, fall back to the workspace walk.
    let files = if inputs.is_empty() {
        match collect_workspace_pdx_files() {
            Ok(files) if files.is_empty() => {
                eprintln!(
                    "paideia-as fmt: no .pdx files found under workspace root \
                     (searched {}); pass paths, a glob, or --stdin",
                    WORKSPACE_SRC_DIRS.join(", ")
                );
                return ExitCode::from(2);
            }
            Ok(files) => files,
            Err(msg) => {
                eprintln!("paideia-as fmt: {msg}");
                return ExitCode::from(2);
            }
        }
    } else {
        match expand_inputs(inputs) {
            Ok(files) if files.is_empty() => {
                eprintln!(
                    "paideia-as fmt: no files matched the given paths/globs \
                     (checked {} entries)",
                    inputs.len()
                );
                return ExitCode::from(2);
            }
            Ok(files) => files,
            Err(msg) => {
                eprintln!("paideia-as fmt: {msg}");
                return ExitCode::from(2);
            }
        }
    };

    let mut any_change = false;
    let mut any_error = false;

    for file in &files {
        match run_one_file(file, check, diff, list_different) {
            FileOutcome::Unchanged => {}
            FileOutcome::Changed => any_change = true,
            FileOutcome::Error => any_error = true,
        }
    }

    if any_error {
        ExitCode::from(2)
    } else if any_change && (check || diff) {
        // --list-different is intentionally omitted here: it signals change
        // via stdout content, not exit code, so pipelines can keep going.
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

enum FileOutcome {
    Unchanged,
    Changed,
    Error,
}

/// Expand a list of raw positional args (as typed on the command line) into
/// a de-duplicated, order-preserving list of `PathBuf`s. An arg containing
/// `*`, `?`, or `[` is treated as a glob and expanded via `glob::glob`;
/// anything else passes through as a literal path.
fn expand_inputs(inputs: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut out: Vec<PathBuf> = Vec::with_capacity(inputs.len());
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for raw in inputs {
        if is_glob_pattern(raw) {
            let paths = match glob::glob(raw) {
                Ok(p) => p,
                Err(e) => {
                    return Err(format!("invalid glob pattern {raw:?}: {e}"));
                }
            };
            for entry in paths {
                match entry {
                    Ok(path) => {
                        // `glob` can yield directories when the pattern permits —
                        // filter to regular files so a `src/**/*` style pattern
                        // doesn't crash on `read_to_string(dir)`.
                        if path.is_file() && seen.insert(path.clone()) {
                            out.push(path);
                        }
                    }
                    Err(e) => {
                        // GlobError is per-entry (e.g. permission denied on one
                        // directory); surface it as a stderr diagnostic rather
                        // than aborting the whole expansion.
                        eprintln!("paideia-as fmt: glob entry error: {e}");
                    }
                }
            }
        } else {
            let path = PathBuf::from(raw);
            if seen.insert(path.clone()) {
                out.push(path);
            }
        }
    }

    Ok(out)
}

/// A pattern is a glob iff it contains at least one of the standard
/// metacharacters `*`, `?`, or `[`. This mirrors `glob::Pattern`'s own
/// grammar — anything outside those three is a literal path component.
fn is_glob_pattern(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

/// Discover the nearest ancestor workspace root (`.pdxfmt` preferred,
/// `Cargo.toml` fallback) and enumerate every `.pdx` file under its
/// `src/` and `tests/` subdirectories.
fn collect_workspace_pdx_files() -> Result<Vec<PathBuf>, String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("failed to read current directory: {e}"))?;
    let root = find_workspace_root(&cwd)
        .ok_or_else(|| format!(
            "no workspace marker ({PDXFMT_MARKER} or {CARGO_MARKER}) \
             found walking up from {}",
            cwd.display()
        ))?;

    let mut out = Vec::new();
    for sub in WORKSPACE_SRC_DIRS {
        let dir = root.join(sub);
        if dir.is_dir() {
            walk_pdx_files(&dir, &mut out);
        }
    }
    out.sort();
    Ok(out)
}

/// Walk upward from `start` looking for a workspace marker. `.pdxfmt`
/// wins over `Cargo.toml` at the same depth so a repo that pins its
/// `.pdx` root with the lightweight marker is honoured even inside a
/// larger Cargo workspace.
fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut cur: Option<&Path> = Some(start);
    while let Some(dir) = cur {
        if dir.join(PDXFMT_MARKER).is_file() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    // Second pass: Cargo.toml fallback. Done after `.pdxfmt` so a `.pdxfmt`
    // at the real project root beats a `Cargo.toml` deeper in the tree.
    let mut cur: Option<&Path> = Some(start);
    while let Some(dir) = cur {
        if dir.join(CARGO_MARKER).is_file() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

/// Recursively collect `.pdx` files under `dir`. Skips hidden directories
/// (`.`-prefix) so we never descend into `.git/`, `target/.cache/`, etc.
/// Non-`.pdx` files are ignored. `std::fs::read_dir` errors on `dir` are
/// surfaced via stderr and silently skipped — the workspace walk is
/// best-effort, not transactional.
fn walk_pdx_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(it) => it,
        Err(e) => {
            eprintln!("paideia-as fmt: cannot read {}: {e}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk_pdx_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("pdx") {
            out.push(path);
        }
    }
}

fn run_one_file(file: &Path, check: bool, diff: bool, list_different: bool) -> FileOutcome {
    let source = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("paideia-as fmt: failed to read {}: {e}", file.display());
            return FileOutcome::Error;
        }
    };

    let formatted = format(&source, &FormatOptions::default());
    let changed = source != formatted;

    if diff {
        if let Some(diff_output) = emit_diff(&source, &formatted, &file.display().to_string(), false) {
            print!("{}", diff_output);
        }
        return if changed { FileOutcome::Changed } else { FileOutcome::Unchanged };
    }

    if list_different {
        if changed {
            println!("{}", file.display());
            return FileOutcome::Changed;
        }
        return FileOutcome::Unchanged;
    }

    if check {
        return if changed { FileOutcome::Changed } else { FileOutcome::Unchanged };
    }

    if changed {
        if let Err(e) = fs::write(file, &formatted) {
            eprintln!("paideia-as fmt: failed to write {}: {e}", file.display());
            return FileOutcome::Error;
        }
    }
    FileOutcome::Unchanged
}

fn run_stdin(check: bool, diff: bool, list_different: bool) -> ExitCode {
    let mut source = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut source) {
        eprintln!("paideia-as fmt: failed to read stdin: {e}");
        return ExitCode::from(2);
    }

    let formatted = format(&source, &FormatOptions::default());
    let changed = source != formatted;

    if diff {
        if let Some(diff_output) = emit_diff(&source, &formatted, "<stdin>", true) {
            print!("{}", diff_output);
        }
        return if changed { ExitCode::from(1) } else { ExitCode::SUCCESS };
    }

    if list_different {
        if changed {
            // Stdin has no filesystem path; emit the conventional sentinel.
            println!("<stdin>");
        }
        return ExitCode::SUCCESS;
    }

    if check {
        return if changed { ExitCode::from(1) } else { ExitCode::SUCCESS };
    }

    if let Err(e) = std::io::stdout().write_all(formatted.as_bytes()) {
        eprintln!("paideia-as fmt: failed to write stdout: {e}");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn emit_diff(source: &str, formatted: &str, label: &str, stdin: bool) -> Option<String> {
    if source == formatted {
        return None;
    }

    let label_clean = label.trim_start_matches('/');
    let header_old = if stdin {
        "<stdin>".to_string()
    } else {
        format!("a/{}", label_clean)
    };

    let header_new = if stdin {
        "<stdin> (formatted)".to_string()
    } else {
        format!("b/{}", label_clean)
    };

    let diff = TextDiff::from_lines(source, formatted);
    let unified = diff
        .unified_diff()
        .context_radius(3)
        .header(&header_old, &header_new)
        .to_string();

    Some(unified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as TempWrite;
    use tempfile::NamedTempFile;

    /// Convenience: wrap a &[PathBuf] call into the new `&[String]` signature
    /// so the pre-FMT-12 unit tests stay readable.
    fn run_paths(paths: &[PathBuf], check: bool, diff: bool, stdin: bool) -> ExitCode {
        let inputs: Vec<String> = paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        run(&inputs, check, diff, stdin, false)
    }

    #[test]
    fn cmd_fmt_check_returns_ok_on_already_formatted() {
        let input = "let x = 1\nlet y = 2\n";
        let result = format(input, &FormatOptions::default());
        assert_eq!(result, input);
    }

    #[test]
    fn cmd_fmt_check_returns_nonzero_on_unformatted() {
        let input = "let x = 1  \nlet y = 2  ";
        let result = format(input, &FormatOptions::default());
        assert_ne!(result, input);
    }

    #[test]
    fn cmd_fmt_writes_to_file_in_place() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1  \nlet y = 2  ";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path().to_path_buf();
        let exit = run_paths(&[path.clone()], false, false, false);
        assert_eq!(exit, ExitCode::SUCCESS);

        let written = fs::read_to_string(&path).unwrap();
        let expected = format(input, &FormatOptions::default());
        assert_eq!(written, expected);
    }

    #[test]
    fn cmd_fmt_writes_to_multiple_files_in_place() {
        let inputs: Vec<(NamedTempFile, &str)> = vec![
            (NamedTempFile::new().unwrap(), "let a = 1  \n"),
            (NamedTempFile::new().unwrap(), "let b = 2  \n"),
            (NamedTempFile::new().unwrap(), "let c = 3  \n"),
        ];
        let mut paths = Vec::new();
        for (tmp, content) in &inputs {
            let mut f = tmp.reopen().unwrap();
            f.write_all(content.as_bytes()).unwrap();
            f.flush().unwrap();
            paths.push(tmp.path().to_path_buf());
        }

        let exit = run_paths(&paths, false, false, false);
        assert_eq!(exit, ExitCode::SUCCESS);

        for (path, (_, source)) in paths.iter().zip(inputs.iter()) {
            let written = fs::read_to_string(path).unwrap();
            let expected = format(source, &FormatOptions::default());
            assert_eq!(written, expected, "content mismatch in {}", path.display());
        }
    }

    #[test]
    fn cmd_fmt_check_multi_file_exits_1_if_any_changed() {
        let mut good = NamedTempFile::new().unwrap();
        good.write_all(b"let x = 1\nlet y = 2\n").unwrap();
        good.flush().unwrap();

        let mut bad = NamedTempFile::new().unwrap();
        bad.write_all(b"let z = 3  \n").unwrap();
        bad.flush().unwrap();

        let exit = run_paths(&[good.path().to_path_buf(), bad.path().to_path_buf()], true, false, false);
        assert_eq!(exit, ExitCode::from(1));

        // Both files unmodified — check doesn't write.
        let good_after = fs::read_to_string(good.path()).unwrap();
        assert_eq!(good_after, "let x = 1\nlet y = 2\n");
    }

    #[test]
    fn cmd_fmt_check_multi_file_exits_0_if_all_clean() {
        let mut a = NamedTempFile::new().unwrap();
        a.write_all(b"let x = 1\nlet y = 2\n").unwrap();
        a.flush().unwrap();

        let mut b = NamedTempFile::new().unwrap();
        b.write_all(b"let p = 5\nlet q = 6\n").unwrap();
        b.flush().unwrap();

        let exit = run_paths(&[a.path().to_path_buf(), b.path().to_path_buf()], true, false, false);
        assert_eq!(exit, ExitCode::SUCCESS);
    }

    #[test]
    fn cmd_fmt_diff_multi_file_prints_per_file_headers() {
        let mut a = NamedTempFile::new().unwrap();
        a.write_all(b"let x = 1  \n").unwrap();
        a.flush().unwrap();

        let mut b = NamedTempFile::new().unwrap();
        b.write_all(b"let y = 2  \n").unwrap();
        b.flush().unwrap();

        // Exercise the run path — output goes to stdout so we can't capture
        // it easily in a unit test, but assert exit code semantics.
        let exit = run_paths(&[a.path().to_path_buf(), b.path().to_path_buf()], false, true, false);
        assert_eq!(exit, ExitCode::from(1));

        // Diff mode must not modify files.
        let a_after = fs::read_to_string(a.path()).unwrap();
        assert_eq!(a_after, "let x = 1  \n");
    }

    #[test]
    fn cmd_fmt_no_inputs_no_stdin_exits_2_when_no_workspace() {
        // Point CWD at a scratch directory with no workspace marker so the
        // workspace walk fails cleanly. We can't change the process CWD in a
        // way that's test-safe (other threads can race), so we validate the
        // explicit code path via `find_workspace_root` instead.
        let tmp = tempfile::tempdir().unwrap();
        assert!(find_workspace_root(tmp.path()).is_none());
    }

    #[test]
    fn diff_produces_nonempty_output_on_unformatted_file() {
        let input = "let x = 1  \nlet y = 2  ";
        let formatted = format(input, &FormatOptions::default());
        let diff_output = emit_diff(input, &formatted, "test.pdx", false);
        assert!(diff_output.is_some());
        let output = diff_output.unwrap();
        assert!(!output.is_empty());
        assert!(output.contains("@@"));
    }

    #[test]
    fn diff_returns_exit_1_on_change() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1  \nlet y = 2  ";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path().to_path_buf();
        let exit = run_paths(&[path], false, true, false);
        assert_eq!(exit, ExitCode::from(1));
    }

    #[test]
    fn diff_returns_exit_0_on_no_change() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1\nlet y = 2\n";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path().to_path_buf();
        let exit = run_paths(&[path], false, true, false);
        assert_eq!(exit, ExitCode::SUCCESS);
    }

    #[test]
    fn diff_does_not_modify_file() {
        let mut tmp = NamedTempFile::new().unwrap();
        let input = "let x = 1  \nlet y = 2  ";
        tmp.write_all(input.as_bytes()).unwrap();
        tmp.flush().unwrap();

        let path = tmp.path().to_path_buf();
        let _ = run_paths(&[path.clone()], false, true, false);

        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written, input);
    }

    // --- FMT-12 (paideia-as#1576) unit tests ------------------------------

    #[test]
    fn is_glob_pattern_detects_metacharacters() {
        assert!(is_glob_pattern("src/*.pdx"));
        assert!(is_glob_pattern("src/**/*.pdx"));
        assert!(is_glob_pattern("src/[abc].pdx"));
        assert!(is_glob_pattern("src/?.pdx"));
        assert!(!is_glob_pattern("src/main.pdx"));
        assert!(!is_glob_pattern("plain-literal.pdx"));
    }

    #[test]
    fn find_workspace_root_prefers_pdxfmt_over_cargo_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("outer");
        let inner = outer.join("inner");
        fs::create_dir_all(&inner).unwrap();
        // Outer carries Cargo.toml; inner carries .pdxfmt — the .pdxfmt
        // should win because it's higher-priority at any ancestor depth
        // where it exists.
        fs::write(outer.join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::write(inner.join(".pdxfmt"), "").unwrap();

        let root = find_workspace_root(&inner).unwrap();
        assert_eq!(root, inner);
    }

    #[test]
    fn find_workspace_root_falls_back_to_cargo_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("outer");
        let inner = outer.join("src").join("deep");
        fs::create_dir_all(&inner).unwrap();
        fs::write(outer.join("Cargo.toml"), "[workspace]\n").unwrap();

        let root = find_workspace_root(&inner).unwrap();
        assert_eq!(root, outer);
    }

    #[test]
    fn walk_pdx_files_skips_hidden_dirs_and_non_pdx() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("hidden.pdx"), "").unwrap();
        fs::write(root.join("a.pdx"), "").unwrap();
        fs::write(root.join("b.txt"), "").unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("c.pdx"), "").unwrap();

        let mut out = Vec::new();
        walk_pdx_files(root, &mut out);
        out.sort();

        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|p| p.ends_with("a.pdx")));
        assert!(out.iter().any(|p| p.ends_with("sub/c.pdx")));
        // .git-hidden file must NOT appear.
        assert!(out.iter().all(|p| !p.to_string_lossy().contains(".git")));
    }

    #[test]
    fn expand_inputs_mixes_literals_and_globs_and_dedupes() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.pdx");
        let b = tmp.path().join("b.pdx");
        fs::write(&a, "").unwrap();
        fs::write(&b, "").unwrap();

        let glob_pat = format!("{}/*.pdx", tmp.path().display());
        let inputs = vec![
            a.display().to_string(),
            glob_pat, // expands to a.pdx + b.pdx
            a.display().to_string(), // duplicate — must be dropped
        ];
        let out = expand_inputs(&inputs).unwrap();
        assert_eq!(out.len(), 2, "expected 2 distinct files, got {out:?}");
    }

    #[test]
    fn list_different_prints_changed_path_and_returns_changed() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"let x = 1  \n").unwrap();
        tmp.flush().unwrap();

        let outcome = run_one_file(tmp.path(), false, false, true);
        matches!(outcome, FileOutcome::Changed);

        // File must NOT be rewritten in --list-different mode.
        let after = fs::read_to_string(tmp.path()).unwrap();
        assert_eq!(after, "let x = 1  \n");
    }

    #[test]
    fn list_different_returns_zero_even_when_files_differ() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"let x = 1  \n").unwrap();
        tmp.flush().unwrap();

        let inputs = vec![tmp.path().display().to_string()];
        let exit = run(&inputs, false, false, false, true);
        assert_eq!(
            exit,
            ExitCode::SUCCESS,
            "list-different must exit 0 regardless of change state"
        );
    }
}
