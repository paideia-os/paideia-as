//! Integration tests for @abi calling convention attribute (PA19-r19-001).
//! Tests parsing and validation of @abi("ms") and @abi("sysv") directives on let bindings.

use crate::common::elf;
use std::fs;
use std::path::PathBuf;

/// Helper: run cargo with the paideia-as command
fn cargo_run(args: &[&str]) -> std::process::Output {
    std::process::Command::new("cargo")
        .arg("run")
        .arg("--release")
        .arg("--quiet")
        .arg("-p")
        .arg("paideia-as")
        .arg("--")
        .args(args)
        .output()
        .expect("failed to run cargo")
}

#[test]
fn abi_ms_lambda_emits_u1620() {
    // Test that @abi("ms") on a lambda binding emits U1620 (deferred-implementation gate)
    let tmp_file = PathBuf::from("/tmp/Test.pdx");
    let source = r#"module Test = structure {
  pub let my_init : (u64) -> u64 = fn(x : u64) -> x + 1 @abi("ms")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_ms_lambda_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should fail with U1620
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must fail
    assert_ne!(
        out.status.code(),
        Some(0),
        "@abi(\"ms\") lambda must fail to build (U1620 gate)"
    );

    // Stderr must contain U1620
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("U1620"),
        "expected U1620 in stderr, got: {}",
        stderr
    );

    // Check: message mentions ms or not yet emittable
    assert!(
        stderr.contains("\"ms\"") || stderr.to_lowercase().contains("not yet emittable"),
        "U1620 message should mention ms or not yet emittable: {}",
        stderr
    );
}

#[test]
fn abi_sysv_lambda_builds_cleanly() {
    // Test that @abi("sysv") on a lambda binding builds successfully (no U1620)
    let tmp_file = PathBuf::from("/tmp/Test2.pdx");
    let source = r#"module Test2 = structure {
  pub let my_init : (u64) -> u64 = fn(x : u64) -> x + 1 @abi("sysv")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_sysv_lambda_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should succeed
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must succeed
    assert_eq!(
        out.status.code(),
        Some(0),
        "@abi(\"sysv\") lambda must build successfully; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // No U1620 in stderr
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("U1620"),
        "should NOT emit U1620 for @abi(\"sysv\")"
    );

    // ELF must be produced and valid
    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn abi_absent_lambda_builds_cleanly() {
    // Test regression: unannotated lambda still emits under paideia default
    let tmp_file = PathBuf::from("/tmp/Test3.pdx");
    let source = r#"module Test3 = structure {
  pub let my_init : (u64) -> u64 = fn(x : u64) -> x + 1
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_absent_lambda_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should succeed
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must succeed
    assert_eq!(
        out.status.code(),
        Some(0),
        "unannotated lambda must build successfully; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // No diagnostics in stderr
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("U1620") && !stderr.contains("P0286"),
        "unannotated lambda must not emit diagnostics"
    );

    // ELF must be produced and valid
    assert!(out_file.exists(), "ELF output file must exist");
    let bytes = fs::read(&out_file).expect("read ELF output");
    elf::assert_elf64_magic(&bytes);
}

#[test]
fn abi_on_non_lambda_p0286() {
    // Test that @abi on a non-lambda binding emits P0286 error during build
    let tmp_file = PathBuf::from("/tmp/Test4.pdx");
    let source = r#"module Test4 = structure {
  pub let x : u64 = 42 @abi("ms")
}"#;
    fs::write(&tmp_file, source).expect("write test file");

    let out_file = PathBuf::from("/tmp/abi_non_lambda_test.o");
    let _ = fs::remove_file(&out_file);

    // Build the fixture - should fail with P0286
    let out = cargo_run(&[
        "build",
        tmp_file.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        out_file.to_str().unwrap(),
    ]);

    // Build must fail
    assert_ne!(
        out.status.code(),
        Some(0),
        "@abi on non-lambda must fail to build"
    );

    // Stderr must contain P0286
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("P0286"),
        "expected P0286 in stderr, got: {}",
        stderr
    );

    // Check: message mentions function-shaped or lambda
    assert!(
        stderr.to_lowercase().contains("function") ||
        stderr.to_lowercase().contains("lambda"),
        "P0286 message should mention function or lambda: {}",
        stderr
    );
}
