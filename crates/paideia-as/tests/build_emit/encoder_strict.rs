//! Phase 6 m1-004: Encoder strict mode test.
//!
//! This test verifies that the --encoder-warn flag is accepted and processed.
//! With a valid .pdx file:
//! - Default behavior: exit 0 (success)
//! - With --encoder-warn: exit 0 (success with flag accepted)
//!
//! This test primarily validates that the CLI flag works correctly.
//! A test with actual encoder failures would require artificially injecting
//! bad instructions that parse but fail to encode, which is complex to set up.

use std::process::Command;

fn build_emit_data(name: &str) -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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

#[test]
fn encoder_strict_default_success() {
    let input = build_emit_data("uart_smoke.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_encoder_strict_default.o",
    ]);

    // Should exit with code 0 (success)
    assert_eq!(
        output.status.code(),
        Some(0),
        "valid .pdx should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn encoder_warn_flag_accepted() {
    let input = build_emit_data("uart_smoke.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_encoder_strict_warn.o",
        "--encoder-warn",
    ]);

    // Should exit with code 0 (success, flag accepted)
    assert_eq!(
        output.status.code(),
        Some(0),
        "valid .pdx with --encoder-warn should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
