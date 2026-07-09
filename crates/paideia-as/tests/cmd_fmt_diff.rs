use assert_cmd::Command;
use std::fs;
use tempfile::NamedTempFile;
use std::io::Write;

#[test]
fn fmt_diff_and_check_conflict() {
    // Verify that --diff and --check cannot be used together.
    // Use a real temp file with content to ensure clap validation error, not file-not-found.
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"let x = 1\n").unwrap();
    tmp.flush().unwrap();

    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let output = cmd
        .arg("fmt")
        .arg("--check")
        .arg("--diff")
        .arg(tmp.path())
        .output();

    assert!(output.is_ok());
    let result = output.unwrap();
    // Exit code should be 2 (clap usage error from conflicts_with).
    assert_eq!(result.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&result.stderr);
    // Verify clap's error message about conflicting arguments.
    assert!(stderr.contains("cannot be used with"), "stderr: {}", stderr);
}

#[test]
fn fmt_diff_stdin_prints_unified_diff() {
    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let unformatted = "let x = 1  \nlet y = 2  ";

    let output = cmd
        .arg("fmt")
        .arg("--stdin")
        .arg("--diff")
        .write_stdin(unformatted)
        .output();

    assert!(output.is_ok());
    let result = output.unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);

    // Should contain unified diff markers.
    assert!(stdout.contains("@@"));
    assert!(stdout.contains("<stdin>"));
    // Exit code should be 1 (formatting would change).
    assert_eq!(result.status.code(), Some(1));
}

#[test]
fn fmt_diff_stdin_no_change_silent() {
    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let already_formatted = "let x = 1\nlet y = 2\n";

    let output = cmd
        .arg("fmt")
        .arg("--stdin")
        .arg("--diff")
        .write_stdin(already_formatted)
        .output();

    assert!(output.is_ok());
    let result = output.unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);

    // Should not print anything if no changes.
    assert!(stdout.is_empty());
    // Exit code should be 0 (no changes).
    assert_eq!(result.status.code(), Some(0));
}

#[test]
fn fmt_diff_file_headers_git_compatible() {
    let mut tmp = NamedTempFile::new().unwrap();
    let unformatted = "let x = 1  \nlet y = 2  ";
    tmp.write_all(unformatted.as_bytes()).unwrap();
    tmp.flush().unwrap();

    let path = tmp.path();
    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let output = cmd
        .arg("fmt")
        .arg("--diff")
        .arg(path)
        .output();

    assert!(output.is_ok());
    let result = output.unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);

    // Should use a/ and b/ prefixes for git compatibility.
    assert!(stdout.contains("a/"));
    assert!(stdout.contains("b/"));
    assert!(stdout.contains("@@"));

    // Verify diff polarity: original (unformatted) is on - side, formatted on + side.
    // The unformatted version has trailing spaces, formatted version does not.
    assert!(
        stdout.contains("-let x = 1  "),
        "Diff should show original (with trailing spaces) prefixed with '-'"
    );
    assert!(
        stdout.contains("+let x = 1"),
        "Diff should show formatted (without trailing spaces) prefixed with '+'"
    );
}

#[test]
fn fmt_diff_file_no_writeback() {
    let mut tmp = NamedTempFile::new().unwrap();
    let unformatted = "let x = 1  \nlet y = 2  ";
    tmp.write_all(unformatted.as_bytes()).unwrap();
    tmp.flush().unwrap();

    let path = tmp.path();
    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    let _ = cmd
        .arg("fmt")
        .arg("--diff")
        .arg(path)
        .output();

    // Verify file was not modified.
    let contents = fs::read_to_string(path).unwrap();
    assert_eq!(contents, unformatted);
}

#[test]
fn fmt_diff_context_radius() {
    let mut cmd = Command::cargo_bin("paideia-as").unwrap();
    // Create a multi-line input where formatting affects middle lines.
    // Line 3 has trailing spaces; others are already formatted.
    let unformatted = "let a = 1\nlet b = 2\nlet x = 3  \nlet d = 4\nlet e = 5\n";

    let output = cmd
        .arg("fmt")
        .arg("--stdin")
        .arg("--diff")
        .write_stdin(unformatted)
        .output();

    assert!(output.is_ok());
    let result = output.unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);

    // Verify context_radius(3) shows surrounding context.
    // The diff should include lines before and after the change.
    assert!(stdout.contains("@@"));

    // With context_radius(3), we should see context lines around the changed line.
    // The unformatted line "let x = 3  " is being changed to "let x = 3".
    // Assert that context lines before and after the change are present.
    assert!(
        stdout.contains("let a = 1"),
        "Diff should include context line before the change"
    );
    assert!(
        stdout.contains("let e = 5"),
        "Diff should include context line after the change"
    );
}
