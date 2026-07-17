//! Regression pin #1234: RecordCons Borrow accepts data symbols, Site A still rejects.
//!
//! Tests that:
//! 1. (6a) `str_hello_const.pdx` (Site B: RecordCons field) compiles successfully
//! 2. (6b) `fnptr_addr_of_reject_data_symbol.pdx` (Site A: top-level Let) still fails with T0536
//!
//! Discriminator: 6a must fail on HEAD=856a2ce, pass after fix; 6b must pass on both.
//! This verifies that AddrOfPolicy parameterization is targeted (not over-relaxed).

use std::path::PathBuf;
use std::process::Command;

fn data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/data");
    p.push(name);
    p
}

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

/// Test 6a: RecordCons field with data-symbol Borrow now succeeds.
///
/// Verifies that Site B (RecordCons field with Borrow child) now accepts data symbols
/// via AddrOfPolicy::FunctionOrObject. This was the blocker for #998 module-Str constants.
#[test]
fn recordcons_data_symbol_borrow_compiles() {
    let input = build_emit_data("str_hello_const.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_recordcons_data.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    // Build should succeed
    assert!(
        out.status.success(),
        "build should succeed for RecordCons Borrow field targeting data symbol. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 6b: Top-level Let with data-symbol Borrow still fails (Site A strict policy preserved).
///
/// Verifies that Site A (top-level Let with fn-ptr LHS type) still rejects non-Function symbols
/// via AddrOfPolicy::FunctionOnly. This guard prevents accidental over-relaxation.
#[test]
fn fnptr_addr_of_reject_data_symbol_still_fails() {
    let input = data("fnptr_addr_of_reject_data_symbol.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_fnptr_reject_data.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    // Build should fail
    assert!(
        !out.status.success(),
        "build should fail for non-function symbol in top-level Let"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    // The diagnostic should contain T0536
    assert!(
        stderr.contains("T0536") || stderr.contains("address-of target must be a function"),
        "expected T0536 error, got: {}",
        stderr
    );

    let _ = std::fs::remove_file(&tmp);
}
