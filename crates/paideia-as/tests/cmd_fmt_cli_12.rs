//! FMT-12 (paideia-as#1576) — CLI integration tests for the three new
//! `paideia-as fmt` ergonomics:
//!   - Shell-style glob expansion for positional args.
//!   - `--list-different` printing paths and exiting 0.
//!   - Workspace default (no args) walking the nearest `.pdxfmt` /
//!     `Cargo.toml` ancestor's `src/` + `tests/`.
//!
//! These tests spawn the real `paideia-as` binary via `assert_cmd` so they
//! cover the full clap + dispatch path, not just the module internals.

use assert_cmd::Command;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

/// Build a one-shot workspace: a tempdir with a `.pdxfmt` marker plus
/// `src/` and `tests/` subdirectories pre-populated from the given file
/// map (`relative-path -> contents`). Returns the tempdir handle so the
/// caller can keep it alive and the resolved workspace root.
fn make_workspace(files: &[(&str, &str)]) -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join(".pdxfmt"), "").unwrap();
    for (rel, body) in files {
        let full = tmp.path().join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(&full).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }
    tmp
}

#[test]
fn fmt_glob_expands_double_star_and_formats_each_match() {
    let ws = make_workspace(&[
        ("src/a.pdx", "let x = 1  \n"),
        ("src/sub/b.pdx", "let y = 2  \n"),
        ("src/sub/not_pdx.txt", "let z = 3  \n"),
        ("tests/c.pdx", "let w = 4  \n"),
    ]);

    // Run with an explicit glob (not the workspace-default path), so the
    // workspace walk does NOT fire — we are testing glob expansion alone.
    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let pat = format!("{}/src/**/*.pdx", ws.path().display());
    let output = cmd.arg("fmt").arg(&pat).output().unwrap();
    assert!(
        output.status.success(),
        "fmt failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    // src/a.pdx and src/sub/b.pdx should be reformatted in place.
    let a_after = fs::read_to_string(ws.path().join("src/a.pdx")).unwrap();
    let b_after = fs::read_to_string(ws.path().join("src/sub/b.pdx")).unwrap();
    assert!(!a_after.contains("1  "), "src/a.pdx not formatted: {a_after:?}");
    assert!(!b_after.contains("2  "), "src/sub/b.pdx not formatted: {b_after:?}");

    // tests/c.pdx and the .txt sibling must be untouched by this invocation.
    let c_after = fs::read_to_string(ws.path().join("tests/c.pdx")).unwrap();
    assert_eq!(c_after, "let w = 4  \n", "tests/c.pdx should not match src/** glob");
    let txt_after = fs::read_to_string(ws.path().join("src/sub/not_pdx.txt")).unwrap();
    assert_eq!(txt_after, "let z = 3  \n", "non-.pdx siblings should be untouched");
}

#[test]
fn fmt_list_different_prints_changed_paths_and_exits_zero() {
    let ws = make_workspace(&[
        ("src/clean.pdx", "let x = 1\n"),
        ("src/dirty.pdx", "let y = 2  \n"),
    ]);

    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let output = cmd
        .arg("fmt")
        .arg("--list-different")
        .arg(ws.path().join("src/clean.pdx"))
        .arg(ws.path().join("src/dirty.pdx"))
        .output()
        .unwrap();

    // KEY contract: exit 0 even though one file would be reformatted.
    assert_eq!(output.status.code(), Some(0), "list-different must exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("dirty.pdx"),
        "expected dirty.pdx in --list-different output, got: {stdout:?}"
    );
    assert!(
        !stdout.contains("clean.pdx"),
        "clean.pdx must NOT appear in --list-different output, got: {stdout:?}"
    );

    // --list-different is read-only like --check.
    let dirty_after = fs::read_to_string(ws.path().join("src/dirty.pdx")).unwrap();
    assert_eq!(
        dirty_after, "let y = 2  \n",
        "--list-different must not rewrite the file"
    );
}

#[test]
fn fmt_list_different_all_clean_prints_nothing_exits_zero() {
    let ws = make_workspace(&[
        ("src/a.pdx", "let x = 1\n"),
        ("src/b.pdx", "let y = 2\n"),
    ]);

    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let output = cmd
        .arg("fmt")
        .arg("--list-different")
        .arg(ws.path().join("src/a.pdx"))
        .arg(ws.path().join("src/b.pdx"))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().is_empty(), "expected empty stdout, got: {stdout:?}");
}

#[test]
fn fmt_list_different_conflicts_with_check_and_diff() {
    let ws = make_workspace(&[("src/a.pdx", "let x = 1\n")]);
    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let output = cmd
        .arg("fmt")
        .arg("--list-different")
        .arg("--check")
        .arg(ws.path().join("src/a.pdx"))
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "--list-different + --check must be a clap usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "expected clap conflict error, got: {stderr:?}"
    );
}

#[test]
fn fmt_workspace_default_formats_pdxfmt_root() {
    let ws = make_workspace(&[
        ("src/a.pdx", "let x = 1  \n"),
        ("src/sub/b.pdx", "let y = 2  \n"),
        ("tests/c.pdx", "let w = 4  \n"),
    ]);

    // Invoke with NO positional args and CWD pinned inside the workspace.
    // The .pdxfmt marker should anchor the walk, and all three .pdx files
    // across src/ and tests/ should be reformatted.
    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    cmd.current_dir(ws.path().join("src")); // walk upward 1 level to hit .pdxfmt
    let output = cmd.arg("fmt").output().unwrap();
    assert!(
        output.status.success(),
        "fmt failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    for rel in ["src/a.pdx", "src/sub/b.pdx", "tests/c.pdx"] {
        let after = fs::read_to_string(ws.path().join(rel)).unwrap();
        assert!(
            !after.contains("  \n") && !after.ends_with("  "),
            "{rel} not reformatted: {after:?}"
        );
    }
}

#[test]
fn fmt_workspace_default_with_list_different_lists_only_dirty() {
    let ws = make_workspace(&[
        ("src/clean.pdx", "let x = 1\n"),
        ("src/dirty.pdx", "let y = 2  \n"),
    ]);

    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    cmd.current_dir(ws.path());
    let output = cmd.arg("fmt").arg("--list-different").output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dirty.pdx"), "stdout: {stdout:?}");
    assert!(!stdout.contains("clean.pdx"), "stdout: {stdout:?}");
    // Files must be unmodified under --list-different.
    let dirty_after = fs::read_to_string(ws.path().join("src/dirty.pdx")).unwrap();
    assert_eq!(dirty_after, "let y = 2  \n");
}

#[test]
fn fmt_glob_matches_zero_files_exits_two() {
    // A glob whose expansion is empty should surface a diagnostic + exit 2
    // rather than silently succeed, so a CI typo (`src/*.pdxx`) is loud.
    let ws = make_workspace(&[("src/a.pdx", "let x = 1\n")]);
    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let pat = format!("{}/src/**/*.pdxx", ws.path().display());
    let output = cmd.arg("fmt").arg(&pat).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no files matched"), "stderr: {stderr:?}");
}

#[test]
fn fmt_glob_skips_hidden_dir_contents_by_default() {
    // glob 0.3 defaults to require_literal_leading_dot = true, which means
    // `src/**/*.pdx` will NOT descend into `.cache/` even though the inner
    // file ends in `.pdx`. This is the shell-POSIX default and matches
    // rustfmt / prettier behaviour.
    let ws = make_workspace(&[
        ("src/a.pdx", "let x = 1  \n"),
        ("src/.cache/hidden.pdx", "let secret = 1  \n"),
    ]);

    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let pat = format!("{}/src/**/*.pdx", ws.path().display());
    cmd.arg("fmt").arg(&pat).output().unwrap();

    // Visible file should have been reformatted.
    let a_after = fs::read_to_string(ws.path().join("src/a.pdx")).unwrap();
    assert!(!a_after.contains("1  "), "src/a.pdx not formatted: {a_after:?}");
    // Hidden-directory file should be UNTOUCHED.
    let hidden_after = fs::read_to_string(ws.path().join("src/.cache/hidden.pdx")).unwrap();
    assert_eq!(
        hidden_after, "let secret = 1  \n",
        ".cache/hidden.pdx should be excluded by the default dot-glob rule"
    );
}
