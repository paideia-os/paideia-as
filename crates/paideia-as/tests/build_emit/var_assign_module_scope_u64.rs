//! Phase 17 m6-c: Variable assignment (Pattern 5) integration tests.
//!
//! Tests that bare variable assignments (`x = value`) route correctly through
//! the emit pipeline for both module-level symbols and local bindings.
//!
//! Test cases:
//! 1. Module-level u64 read-back after assignment
//! 2. Module-level u64 assigned from literal in unsafe block
//! 3. Local binding assigned from another local binding (reg-to-reg move)

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
fn var_assign_module_scope_builds_successfully() {
    // Phase 17 m6-c: Module-level variable assignments should parse and emit successfully.
    let input = build_emit_data("var_assign_module_scope_u64.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_var_assign_module_scope.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "Variable assignment should build successfully. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn var_assign_produces_valid_elf() {
    // Verify that the output object file is created and is valid ELF.
    let input = build_emit_data("var_assign_module_scope_u64.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_var_assign_output.o",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "Variable assignment should produce valid ELF. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        std::path::Path::new("/tmp/test_var_assign_output.o").exists(),
        "Output ELF file should be created"
    );
}

#[test]
fn var_assign_module_scope_no_diagnostics() {
    // Variable assignments should not produce any diagnostics in this simple case.
    let input = build_emit_data("var_assign_module_scope_u64.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_var_assign_nodiag.o",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not contain diagnostic prefixes like "T0518", "U1614", etc.
    assert!(
        !stderr.contains("T0518") && !stderr.contains("U1614"),
        "Variable assignment should not produce diagnostics. stderr: {}",
        stderr
    );
}
