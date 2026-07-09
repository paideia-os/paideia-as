//! Phase 8 m1-004: Integration tests for typed encoder/emitter diagnostics (SARIF output).
//!
//! Verifies that:
//! - B1705 (encoder-error) fires on encoding failures
//! - B1706 (encoder-warn) fires on warnings with --encoder-warn
//! - U1610 (unresolved-label) fires on label fixup failures
//! - B1703 (symbol-layout-invalid) fires on symbol validation failures
//! - B1704 (function-symbol-no-offset) fires on missing function offsets
//!
//! These tests use SARIF output to verify diagnostic codes and locations.

use std::process::Command;
use std::fs;
use std::path::PathBuf;

fn build_emit_data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/build-emit");
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Extract results array from SARIF JSON.
/// Returns a serde_json::Value representing the results array, or None if not found.
fn extract_sarif_results(sarif_path: &str) -> Option<Vec<serde_json::Value>> {
    let content = fs::read_to_string(sarif_path).ok()?;
    let sarif_json: serde_json::Value = serde_json::from_str(&content).ok()?;

    // Navigate to runs[0].results[]
    sarif_json
        .pointer("/runs/0/results")
        .and_then(|v| v.as_array())
        .map(|arr| arr.clone())
}

#[test]
fn encoder_failure_typed_diagnostic_in_sarif() {
    // Phase 8 m1-004: Verify B1705 fires on encoder failure with --sarif.
    let input = build_emit_data("encoder_strict_unsupported.pdx");
    let sarif_path = "/tmp/test_encoder_failure_diagnostic.sarif.json";

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_encoder_failure.o",
        "--sarif",
        sarif_path,
    ]);

    // Exit code 2 (error)
    assert_eq!(
        output.status.code(),
        Some(2),
        "encoder failure should exit 2. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // SARIF file exists and contains B1705
    let results = extract_sarif_results(sarif_path)
        .expect("SARIF file should exist and be valid JSON");

    assert!(!results.is_empty(), "SARIF results should not be empty");

    let has_b1705 = results.iter().any(|result| {
        result
            .pointer("/ruleId")
            .and_then(|v| v.as_str())
            .map(|rid| rid.contains("B1705"))
            .unwrap_or(false)
    });

    assert!(has_b1705, "SARIF results should contain B1705 ruleId");
}

#[test]
fn encoder_warn_typed_diagnostic_in_sarif() {
    // Phase 8 m1-004: Verify B1706 fires on encoder warning with --encoder-warn --sarif.
    let input = build_emit_data("encoder_strict_unsupported.pdx");
    let sarif_path = "/tmp/test_encoder_warn_diagnostic.sarif.json";

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_encoder_warn.o",
        "--encoder-warn",
        "--sarif",
        sarif_path,
    ]);

    // Exit code 0 (success with warning)
    assert_eq!(
        output.status.code(),
        Some(0),
        "encoder with --encoder-warn should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // SARIF file exists and contains B1706 at warning level
    let results = extract_sarif_results(sarif_path)
        .expect("SARIF file should exist and be valid JSON");

    assert!(!results.is_empty(), "SARIF results should not be empty");

    let has_b1706_warning = results.iter().any(|result| {
        let has_rule = result
            .pointer("/ruleId")
            .and_then(|v| v.as_str())
            .map(|rid| rid.contains("B1706"))
            .unwrap_or(false);

        let has_level = result
            .pointer("/level")
            .and_then(|v| v.as_str())
            .map(|level| level == "warning")
            .unwrap_or(false);

        has_rule && has_level
    });

    assert!(
        has_b1706_warning,
        "SARIF results should contain B1706 at warning level"
    );
}

#[test]
#[ignore = "TODO(#1105-followup): needs label-resolution fixture"]
fn unresolved_label_typed_diagnostic_in_sarif() {
    // Phase 8 m1-004: Verify U1610 fires on unresolved labels in fixup pass.
    // This requires a fixture that reaches the label-fixup stage with an undefined label.
    panic!("TODO: create fixture with unresolved label reference");
}

#[test]
#[ignore = "TODO(#1105-followup): needs duplicate-symbol fixture"]
fn symbol_layout_invalid_typed_diagnostic_in_sarif() {
    // Phase 8 m1-004: Verify B1703 fires on symbol layout validation failures.
    // This requires a fixture that triggers symbol layout conflicts.
    panic!("TODO: create fixture with duplicate symbols");
}

#[test]
#[ignore = "TODO(#1105-followup): needs lambda-no-offset fixture"]
fn lambda_no_offset_typed_diagnostic_in_sarif() {
    // Phase 8 m1-004: Verify B1704 fires on missing function symbol offsets.
    // This should fire when a lambda is in emitted_lambdas but has no recorded offset.
    panic!("TODO: create fixture that triggers lambda offset mismatch");
}

#[test]
fn encoder_warn_diagnostic_appears_in_stderr_when_no_sarif() {
    // Phase 8 m1-004: Verify B1706 appears in stderr when --encoder-warn is used without --sarif.
    let input = build_emit_data("encoder_strict_unsupported.pdx");

    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_encoder_warn_stderr.o",
        "--encoder-warn",
    ]);

    // Exit code 0 (success)
    assert_eq!(
        output.status.code(),
        Some(0),
        "encoder with --encoder-warn should exit 0"
    );

    // Stderr contains warning[B1706]: (may have ANSI color codes)
    let stderr = String::from_utf8_lossy(&output.stderr);
    let has_b1706 = stderr.contains("B1706");
    assert!(
        has_b1706,
        "stderr should contain 'B1706'. stderr: {}",
        stderr
    );
}
