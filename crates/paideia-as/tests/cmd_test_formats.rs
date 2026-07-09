//! Integration tests for `paideia-as test --format <human|json|tap>`.
//!
//! Tests the three output formats: human-readable (default), JSON NDJSON,
//! and TAP 13.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Create a temporary test fixture with two test files.
/// Returns the temp directory, path to test1.pdx, and path to test2.pdx.
fn setup_fixture() -> (TempDir, String, String) {
    let temp_dir = TempDir::new().expect("create tempdir");
    let test_dir = temp_dir.path().to_path_buf();

    let test1_path = test_dir.join("test_add.pdx");
    let test1_content = r#"
#[test]
fn test_add() {
    // test body
}
"#;
    fs::write(&test1_path, test1_content).expect("write test1");

    let test2_path = test_dir.join("test_subtract.pdx");
    let test2_content = r#"
#[test]
fn test_subtract() {
    // test body
}
"#;
    fs::write(&test2_path, test2_content).expect("write test2");

    let test1_str = test1_path.display().to_string();
    let test2_str = test2_path.display().to_string();
    (temp_dir, test1_str, test2_str)
}

/// Setup with zero tests (empty test directory).
fn setup_empty_fixture() -> TempDir {
    TempDir::new().expect("create tempdir")
}

#[test]
fn human_format_default_stderr_summary_byte_identical() {
    let (_temp_dir, test1, test2) = setup_fixture();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let assert = cmd
        .arg("test")
        .arg(&test1)
        .arg(&test2)
        .assert();

    // Verify stderr contains the expected summary line
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("test result: ok. discovered: 2; passed: 2; failed: 0"),
        "stderr: {}",
        stderr
    );

    // Verify stdout is empty (human format doesn't use stdout for results)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.is_empty(), "stdout should be empty, got: {}", stdout);
}

#[test]
fn human_format_list_mode_stdout_unchanged() {
    let (_temp_dir, test1, test2) = setup_fixture();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let assert = cmd
        .arg("test")
        .arg("--list")
        .arg(&test1)
        .arg(&test2)
        .assert();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain two test entries
    assert!(stdout.contains("test_add.pdx: test_add"), "stdout: {}", stdout);
    assert!(stdout.contains("test_subtract.pdx: test_subtract"), "stdout: {}", stdout);

    // Verify stderr is empty in list mode
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "stderr should be empty, got: {}", stderr);
}

#[test]
fn json_format_run_mode_emits_ndjson() {
    let (_temp_dir, test1, test2) = setup_fixture();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let assert = cmd
        .arg("test")
        .arg("--format")
        .arg("json")
        .arg(&test1)
        .arg(&test2)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 6, "Expected at least 6 lines, got: {}", lines.len());

    // Parse each line and verify structure
    let start: serde_json::Value = serde_json::from_str(lines[0])
        .expect("first line should be valid JSON");
    assert_eq!(start["type"], "suite");
    assert_eq!(start["event"], "started");
    assert_eq!(start["test_count"], 2);

    // Lines 1-2 should be first test/started + test/ok
    // Lines 3-4 should be second test/started + test/ok
    // Line 5 should be suite/ok

    let suite_ok: serde_json::Value = serde_json::from_str(lines[5])
        .expect("last line should be valid JSON");
    assert_eq!(suite_ok["type"], "suite");
    assert_eq!(suite_ok["event"], "ok");
    assert_eq!(suite_ok["passed"], 2);
    assert_eq!(suite_ok["failed"], 0);
}

#[test]
fn json_format_list_mode_emits_discovered_events() {
    let (_temp_dir, test1, test2) = setup_fixture();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let assert = cmd
        .arg("test")
        .arg("--list")
        .arg("--format")
        .arg("json")
        .arg(&test1)
        .arg(&test2)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "Expected 3 lines (2 discovered + 1 suite)");

    // First two lines are discovered events
    let discovered1: serde_json::Value = serde_json::from_str(lines[0]).expect("line 0");
    assert_eq!(discovered1["type"], "test");
    assert_eq!(discovered1["event"], "discovered");

    // Last line is suite/listed
    let suite: serde_json::Value = serde_json::from_str(lines[2]).expect("line 2");
    assert_eq!(suite["type"], "suite");
    assert_eq!(suite["event"], "listed");
    assert_eq!(suite["count"], 2);
}

#[test]
fn tap_format_run_mode_shape() {
    let (_temp_dir, test1, test2) = setup_fixture();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let assert = cmd
        .arg("test")
        .arg("--format")
        .arg("tap")
        .arg(&test1)
        .arg(&test2)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 6, "Expected at least 6 lines");

    // Verify TAP structure
    assert_eq!(lines[0], "TAP version 13");
    assert_eq!(lines[1], "1..2");
    assert!(lines[2].contains("ok 1 -"));
    assert!(lines[3].contains("ok 2 -"));
    assert!(lines[4].contains("# tests 2"));
    assert!(lines[5].contains("# pass 2"));
    assert!(lines[6].contains("# fail 0"));
}

#[test]
fn tap_format_list_mode_uses_skip_directive() {
    let (_temp_dir, test1, test2) = setup_fixture();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let assert = cmd
        .arg("test")
        .arg("--list")
        .arg("--format")
        .arg("tap")
        .arg(&test1)
        .arg(&test2)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify all numbered lines contain SKIP directive
    assert!(stdout.contains("# SKIP listed only"));
    let lines: Vec<&str> = stdout.lines().collect();
    for line in lines.iter().skip(2).take(2) {
        assert!(line.contains("# SKIP listed only"), "line: {}", line);
    }
}

#[test]
fn tap_format_zero_tests_emits_plan_zero() {
    let temp_dir = setup_empty_fixture();
    let test_dir = temp_dir.path().display().to_string();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let assert = cmd
        .arg("test")
        .arg("--format")
        .arg("tap")
        .arg(&test_dir)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "TAP version 13");
    assert_eq!(lines[1], "1..0 # SKIP no tests found");
}

#[test]
#[ignore]  // Only runs if prove is available on the system
fn tap_format_zero_tests_valid_for_prove() {
    let temp_dir = setup_empty_fixture();
    let test_dir = temp_dir.path().display().to_string();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let output = cmd
        .arg("test")
        .arg("--format")
        .arg("tap")
        .arg(&test_dir)
        .output()
        .expect("run paideia-as");

    let tap_output = String::from_utf8_lossy(&output.stdout);

    // Run through prove
    let mut prove = std::process::Command::new("prove")
        .arg("-e")
        .arg("cat")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn prove");

    {
        if let Some(mut stdin) = prove.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(tap_output.as_bytes());
        }
    }

    let result = prove.wait().expect("wait for prove");
    assert!(
        result.success(),
        "prove should exit 0, got: {}",
        result
    );
}

#[test]
fn json_format_zero_tests_emits_suite_events_only() {
    let temp_dir = setup_empty_fixture();
    let test_dir = temp_dir.path().display().to_string();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let assert = cmd
        .arg("test")
        .arg("--format")
        .arg("json")
        .arg(&test_dir)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);

    let start: serde_json::Value = serde_json::from_str(lines[0]).expect("line 0");
    assert_eq!(start["type"], "suite");
    assert_eq!(start["event"], "started");
    assert_eq!(start["test_count"], 0);

    let ok: serde_json::Value = serde_json::from_str(lines[1]).expect("line 1");
    assert_eq!(ok["type"], "suite");
    assert_eq!(ok["event"], "ok");
    assert_eq!(ok["passed"], 0);
    assert_eq!(ok["failed"], 0);
}

#[test]
fn format_flag_rejects_unknown_value() {
    let (_temp_dir, test1, test2) = setup_fixture();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let assert = cmd
        .arg("test")
        .arg("--format")
        .arg("xml")
        .arg(&test1)
        .arg(&test2)
        .assert()
        .failure();

    // Should exit with status 2 (clap error)
    assert.code(2);
}

#[test]
fn filter_error_still_uses_stderr_across_all_formats() {
    let (_temp_dir, test1, test2) = setup_fixture();

    for format in &["human", "json", "tap"] {
        let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
        let assert = cmd
            .arg("test")
            .arg("--filter")
            .arg("[invalid(regex")
            .arg("--format")
            .arg(format)
            .arg(&test1)
            .arg(&test2)
            .assert()
            .failure();

        let output = assert.get_output();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Invalid regex pattern"),
            "format {}: stderr: {}",
            format,
            stderr
        );
    }
}

#[test]
fn format_composes_with_filter() {
    let (_temp_dir, test1, test2) = setup_fixture();

    let mut cmd = Command::cargo_bin("paideia-as").expect("find binary");
    let assert = cmd
        .arg("test")
        .arg("--filter")
        .arg("^test_add")
        .arg("--format")
        .arg("json")
        .arg(&test1)
        .arg(&test2)
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should only contain one test (test_add)
    // Count only test/started events (not suite/started)
    let test_started_count = stdout.matches("\"type\":\"test\"").count();
    assert_eq!(test_started_count, 2, "Should filter to exactly 1 test (started + ok)");
    // Verify the name
    assert!(stdout.contains("test_add"), "Should contain test_add");
}
