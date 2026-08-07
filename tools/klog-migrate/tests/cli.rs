//! Integration tests for `paideia-as-klog-migrate` (assert_cmd).
//!
//! Covers stdout, --in-place, --check, --diff, and golden-fixture round
//! trips. All fixtures live under `tests/fixtures/`.

use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push(name);
    p
}

fn read(path: &PathBuf) -> String {
    fs::read_to_string(path).expect("read fixture")
}

fn bin() -> Command {
    Command::cargo_bin("paideia-as-klog-migrate").expect("cargo bin")
}

#[test]
fn stdout_mode_prints_migrated_source() {
    let src = fixture("basic.pdx");
    let expected = read(&fixture("basic.expected.pdx"));
    let out = bin().arg(&src).output().unwrap();
    assert!(out.status.success(), "status: {:?}", out.status);
    let got = String::from_utf8(out.stdout).unwrap();
    assert_eq!(got, expected, "stdout != expected fixture");
}

#[test]
fn stdout_mode_does_not_touch_input() {
    let src_path = fixture("basic.pdx");
    let before = read(&src_path);
    let _ = bin().arg(&src_path).output().unwrap();
    let after = read(&src_path);
    assert_eq!(before, after, "input file must not be modified in stdout mode");
}

#[test]
fn in_place_writes_and_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("basic.pdx");
    fs::write(&path, read(&fixture("basic.pdx"))).unwrap();
    let expected = read(&fixture("basic.expected.pdx"));

    // First run: rewrites.
    bin()
        .arg(&path)
        .arg("--in-place")
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);

    // Second run: idempotent, no changes.
    bin()
        .arg(&path)
        .arg("--in-place")
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
}

#[test]
fn check_mode_exits_1_when_would_change() {
    let src = fixture("basic.pdx");
    bin().arg(&src).arg("--check").assert().code(1);
}

#[test]
fn check_mode_exits_0_on_clean_file() {
    // basic.expected.pdx is already fully migrated; it must be clean.
    let src = fixture("basic.expected.pdx");
    bin().arg(&src).arg("--check").assert().code(0);
}

#[test]
fn comment_only_file_is_a_no_op() {
    let src = fixture("comment_safe.pdx");
    let before = read(&src);
    let out = bin().arg(&src).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    // Because comments are stripped by the lexer, no matches → source out
    // equals source in.
    assert_eq!(stdout, before);
    // --check must also be clean.
    bin().arg(&src).arg("--check").assert().code(0);
}

#[test]
fn fail_pattern_elevates_matching_symbols() {
    let src = fixture("fail_pattern.pdx");
    let expected = read(&fixture("fail_pattern.expected.pdx"));
    let out = bin().arg(&src).output().unwrap();
    assert!(out.status.success());
    let got = String::from_utf8(out.stdout).unwrap();
    assert_eq!(got, expected);
}

#[test]
fn diff_mode_emits_unified_diff_on_stderr() {
    let src = fixture("basic.pdx");
    let out = bin().arg(&src).arg("--diff").output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    // Unified diff header prefixes.
    assert!(stderr.contains("--- "), "no unified-diff `---` header");
    assert!(stderr.contains("+++ "), "no unified-diff `+++` header");
    // A '-' line for the old call and a '+' line for the new klog_s1 call.
    assert!(stderr.contains("-        call uart_puts;"));
    assert!(stderr.contains("+        call klog_s1;"));
}

#[test]
fn custom_subsys_flag_is_respected() {
    let src = fixture("basic.pdx");
    let out = bin()
        .arg(&src)
        .arg("--subsys")
        .arg("SUBSYS_INT_")
        .output()
        .unwrap();
    assert!(out.status.success());
    let got = String::from_utf8(out.stdout).unwrap();
    assert!(got.contains("lea rsi, [rip + SUBSYS_INT_];"));
    assert!(!got.contains("SUBSYS_BOOT"));
}

#[test]
fn custom_level_flag_is_respected() {
    let src = fixture("basic.pdx");
    let out = bin()
        .arg(&src)
        .arg("--level")
        .arg("4")
        .output()
        .unwrap();
    assert!(out.status.success());
    let got = String::from_utf8(out.stdout).unwrap();
    assert!(got.contains("mov rdi, 4;"));
}

#[test]
fn missing_file_returns_exit_2() {
    bin().arg("/nonexistent/path.pdx").assert().code(2);
}

#[test]
fn bad_regex_returns_exit_2() {
    let src = fixture("basic.pdx");
    bin()
        .arg(&src)
        .arg("--fail-pattern")
        .arg("[unclosed")
        .assert()
        .code(2);
}

#[test]
fn trailing_comment_emits_warning_on_stderr() {
    let src = fixture("trailing_comment.pdx");
    let out = bin().arg(&src).output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    // Warning line names the file, the line, and the affected symbol.
    assert!(
        stderr.contains("dropped trailing comment"),
        "no comment-drop warning in stderr:\n{stderr}"
    );
    assert!(stderr.contains("banner_msg"), "warning missing symbol name");
    // Warning fires against line 8 (1-based) — the `lea rdi` line.
    assert!(stderr.contains(":8:"), "warning missing line number 8:\n{stderr}");
    // Stdout still holds the migrated source; the warning does not block the rewrite.
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("call klog_s1;"));
    assert!(!stdout.contains("call uart_puts;"));
}

#[test]
fn in_place_and_check_are_mutually_exclusive() {
    let src = fixture("basic.pdx");
    bin()
        .arg(&src)
        .arg("--in-place")
        .arg("--check")
        .assert()
        .failure(); // clap rejects at parse time; not exit code 0.
}

// ---- semicolon-optional fixtures (paideia-as#1273) ----------------------
//
// paideia-os#717 / paideia-as#1272 shipped a scanner that required both `;`
// tokens (positions 8 and 11). Real `.pdx` sources like
// paideia-os/src/kernel/boot/kernel_main.pdx mix the semicolon and
// newline-terminated styles freely (54 valid sites silently skipped on the
// v0.20.1 build). The following three fixtures pin the fix.

#[test]
fn no_semi_lea_fixture_migrates() {
    let src = fixture("no_semi_lea.pdx");
    let expected = read(&fixture("no_semi_lea.expected.pdx"));
    let out = bin().arg(&src).output().unwrap();
    assert!(out.status.success(), "status: {:?}", out.status);
    let got = String::from_utf8(out.stdout).unwrap();
    assert_eq!(got, expected, "no_semi_lea: stdout != expected");
}

#[test]
fn no_semi_lea_is_idempotent() {
    // The expected file has no direct-UART pattern left; a second run
    // must be a no-op (--check exits 0).
    let src = fixture("no_semi_lea.expected.pdx");
    bin().arg(&src).arg("--check").assert().code(0);
}

#[test]
fn no_semi_call_fixture_migrates() {
    let src = fixture("no_semi_call.pdx");
    let expected = read(&fixture("no_semi_call.expected.pdx"));
    let out = bin().arg(&src).output().unwrap();
    assert!(out.status.success(), "status: {:?}", out.status);
    let got = String::from_utf8(out.stdout).unwrap();
    assert_eq!(got, expected, "no_semi_call: stdout != expected");
}

#[test]
fn no_semi_call_is_idempotent() {
    let src = fixture("no_semi_call.expected.pdx");
    bin().arg(&src).arg("--check").assert().code(0);
}

#[test]
fn no_semi_both_fixture_migrates() {
    let src = fixture("no_semi_both.pdx");
    let expected = read(&fixture("no_semi_both.expected.pdx"));
    let out = bin().arg(&src).output().unwrap();
    assert!(out.status.success(), "status: {:?}", out.status);
    let got = String::from_utf8(out.stdout).unwrap();
    assert_eq!(got, expected, "no_semi_both: stdout != expected");
}

#[test]
fn no_semi_both_is_idempotent() {
    let src = fixture("no_semi_both.expected.pdx");
    bin().arg(&src).arg("--check").assert().code(0);
}
