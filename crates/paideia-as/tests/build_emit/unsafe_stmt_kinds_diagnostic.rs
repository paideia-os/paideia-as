//! Issue #1077 follow-up: restore coverage retired when the boot_observable_smoke and
//! pa7c_plt32_witness fixtures were edited to work around U1614 firing on StmtLet/StmtWhile
//! inside unsafe blocks.
//!
//! Those two fixtures previously exercised (a) nested let-of-Var bindings and (b) a while
//! loop, both nested inside unsafe blocks. Once U1614 started firing on any unsafe-block
//! statement other than StmtInstruction/StmtLabel (see unsafe_walker.rs), those fixtures had
//! to be rewritten to straight-line asm to keep passing — silently dropping coverage for the
//! StmtLet and StmtWhile cases.
//!
//! This test pins that coverage back down, mirroring
//! build_emit_unsafe_call_stmt_diagnostic.rs's StmtExpr case:
//! - pa_r17_unsafe_let_stmt.pdx: `let x : u64 = 42` inside an unsafe block
//! - pa_r17_unsafe_while_stmt.pdx: `while false { call tgt }` inside an unsafe block
//!
//! Expected behavior for both fixtures:
//! - Parser accepts the statement inside the unsafe block's statement stream
//! - UnsafeWalker encounters a statement kind that isn't StmtInstruction/StmtLabel
//! - UnsafeWalker emits U1614 diagnostic for the unsupported statement kind
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
fn unsafe_let_stmt_emits_u1614_diagnostic() {
    // pa_r17_unsafe_let_stmt.pdx should fail with U1614 diagnostic.
    // Verifies that:
    // - Parser accepts the let statement inside the unsafe block
    // - UnsafeWalker detects the unsupported StmtLet statement
    // - Compilation fails (non-zero exit code)
    // - stderr contains "U1614" diagnostic code
    let input = build_emit_data("pa_r17_unsafe_let_stmt.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_unsafe_let_stmt.o",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_ne!(
        output.status.code(),
        Some(0),
        "build should fail with diagnostic error. stderr: {}",
        stderr
    );

    assert!(
        stderr.contains("U1614"),
        "stderr should contain U1614 diagnostic code. stderr: {}",
        stderr
    );
}

#[test]
fn unsafe_while_stmt_emits_u1614_diagnostic() {
    // pa_r17_unsafe_while_stmt.pdx should fail with U1614 diagnostic.
    // Verifies that:
    // - Parser accepts the while statement inside the unsafe block
    // - UnsafeWalker detects the unsupported StmtWhile statement
    // - Compilation fails (non-zero exit code)
    // - stderr contains "U1614" diagnostic code
    let input = build_emit_data("pa_r17_unsafe_while_stmt.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_unsafe_while_stmt.o",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_ne!(
        output.status.code(),
        Some(0),
        "build should fail with diagnostic error. stderr: {}",
        stderr
    );

    assert!(
        stderr.contains("U1614"),
        "stderr should contain U1614 diagnostic code. stderr: {}",
        stderr
    );
}
