//! Issue #1198: enum-literal call arguments ambiguity detection (T0555)
//!
//! This test verifies that when a bare enum variant identifier matches variants
//! from multiple enums, the compiler emits a T0555 diagnostic (ambiguous identifier)
//! and fails to build.
//!
//! Expected behavior:
//! - Two enums are declared with overlapping variant names (A)
//! - A call using bare `A` as argument is ambiguous
//! - Compiler emits T0555 diagnostic
//! - Build fails with diagnostic message

use std::path::PathBuf;
use std::process::Command;

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

#[test]
fn enum_lit_call_arg_ambiguous_emits_t0555() {
    let input = build_emit_data("enum_lit_call_arg_ambiguous.pdx");
    let output_path = "/tmp/test_enum_lit_call_arg_ambiguous.o";
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        output_path,
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Build should fail
    assert!(
        !output.status.success(),
        "Build should have failed due to ambiguous variant, but succeeded"
    );

    // Diagnostic output should contain T0555
    assert!(
        stderr.contains("T0555") || stderr.contains("ambiguous"),
        "Expected T0555 diagnostic for ambiguous bare enum variant in stderr:\n{}",
        stderr
    );
}
