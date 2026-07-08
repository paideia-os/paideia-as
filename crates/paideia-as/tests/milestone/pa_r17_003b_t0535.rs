//! PA-r17-003b (#1038): T0535 fn-ptr signature check wiring.
//!
//! This test verifies the infrastructure for function-pointer signature type checking.
//! It validates that mismatched function pointer assignments are detected and reported
//! as T0535 (fn-ptr signature mismatch) diagnostics.
//!
//! Test cases:
//! 1. Parameter type mismatch: `let f: (u32) -> u32 = &fn_expecting_u64` → T0535
//! 2. Arity mismatch: `let f: (u64) -> u64 = &fn_no_args` → T0535
//! 3. Return type mismatch: Type-checked when return type inference is available

use std::path::PathBuf;
use std::process::Command;

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/pa_r17_003b_t0535");
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Test 1: Parameter type mismatch should produce T0535.
///
/// Assigns a function expecting u64 to a binding typed as (u32) -> u32.
/// This should fail with T0535 diagnostic (parameter type mismatch).
#[test]
fn t0535_param_type_mismatch_detected() {
    let input = fixture_dir().join("t0535_param_mismatch.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap()]);

    // The build should complete (we continue on errors per task spec)
    // Check that stderr contains T0535 diagnostic
    let stderr = String::from_utf8_lossy(&out.stderr);

    // Signature check must fire: T0535 for parameter type mismatch
    assert!(
        stderr.contains("T0535") || stderr.contains("535"),
        "Expected T0535 diagnostic for param type mismatch.\nStderr: {}",
        stderr
    );
}

/// Test 2: Arity mismatch should produce T0535.
///
/// Assigns a function with no parameters to a binding typed as (u64) -> u64.
/// This should fail with T0535 diagnostic (arity mismatch).
#[test]
fn t0535_arity_mismatch_detected() {
    let input = fixture_dir().join("t0535_arity_mismatch.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap()]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Signature check must fire: T0535 for arity mismatch
    assert!(
        stderr.contains("T0535") || stderr.contains("535"),
        "Expected T0535 diagnostic for arity mismatch.\nStderr: {}",
        stderr
    );
}

/// Test 3: Compatible signature should accept silently.
///
/// Assigns a function with compatible signature (u64 params can widen to match annotation).
/// This should succeed without diagnostic.
#[test]
fn t0535_compatible_signature_accepts() {
    let input = fixture_dir().join("t0535_compatible.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap()]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Compatible assignment must NOT produce T0535
    assert!(
        !stderr.contains("T0535") && !stderr.contains("535"),
        "Unexpected T0535 for compatible signature.\nStderr: {}",
        stderr
    );
}
