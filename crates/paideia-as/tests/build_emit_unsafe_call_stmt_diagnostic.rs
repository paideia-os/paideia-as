//! Issue #1077: surface the silent unsafe-block statement drop as a loud U1614 diagnostic
//!
//! This test verifies that call expressions inside unsafe blocks emit a U1614 diagnostic.
//! The unsafe block currently only supports instruction statements and labels; other statement
//! kinds (like call expressions) are silently dropped in codegen. This diagnostic makes that
//! drop explicit.
//!
//! Expected behavior:
//! - Parser accepts call expression inside unsafe block's statement stream
//! - UnsafeWalker encounters the StmtExpr node (not StmtInstruction or StmtLabel)
//! - UnsafeWalker emits U1614 diagnostic for unsupported statement kind
//! - Compilation fails with non-zero exit code

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
fn unsafe_call_stmt_emits_u1614_diagnostic() {
    // Issue #1077 AC: pa_r17_unsafe_call_stmt.pdx should fail with U1614 diagnostic.
    // Verifies that:
    // - Parser accepts the call expression inside unsafe block
    // - UnsafeWalker detects the unsupported StmtExpr statement
    // - Compilation fails (non-zero exit code)
    // - stderr contains "U1614" diagnostic code
    let input = build_emit_data("pa_r17_unsafe_call_stmt.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_unsafe_call_stmt.o",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Assert: build should fail
    assert_ne!(
        output.status.code(),
        Some(0),
        "build should fail with diagnostic error. stderr: {}",
        stderr
    );

    // Assert: stderr should contain the U1614 diagnostic code
    assert!(
        stderr.contains("U1614"),
        "stderr should contain U1614 diagnostic code. stderr: {}",
        stderr
    );
}
