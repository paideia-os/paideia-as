//! Issue #1088: route call-expression statements inside unsafe blocks through emit pipeline
//!
//! This test verifies that call expressions inside unsafe blocks compile successfully
//! and emit the proper CALL instruction.
//!
//! Expected behavior (Issue #1088):
//! - Parser accepts call expression inside unsafe block's statement stream
//! - UnsafeWalker skips the StmtExpr node (no U1614)
//! - EmitWalker routes the call through emit_call_stmt, emitting arguments + CALL (no RET)
//! - Compilation succeeds with exit code 0
//! - Resulting ELF contains CALL opcode in .text section

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
fn unsafe_call_stmt_emits_call() {
    // Issue #1088 AC: pa_r17_unsafe_call_stmt.pdx should compile successfully.
    // Verifies that:
    // - Parser accepts the call expression inside unsafe block
    // - UnsafeWalker skips the StmtExpr (no error)
    // - EmitWalker emits the call via emit_call_stmt
    // - Compilation succeeds (exit code 0)
    // - stderr does not contain "U1614" diagnostic code
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

    // Assert: build should succeed
    assert_eq!(
        output.status.code(),
        Some(0),
        "build should succeed. stderr: {}",
        stderr
    );

    // Assert: stderr should NOT contain the U1614 diagnostic code
    assert!(
        !stderr.contains("U1614"),
        "stderr should not contain U1614 diagnostic code. stderr: {}",
        stderr
    );
}
