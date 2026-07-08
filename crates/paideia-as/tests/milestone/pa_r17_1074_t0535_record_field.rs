//! PA-r17-1074 (#1074): T0535 fn-ptr signature check for record-literal fields.
//!
//! This test verifies that function-pointer signature mismatches are detected
//! when assigning fn-ptr values to record-literal fields, not just top-level
//! let bindings.
//!
//! Test cases:
//! 1. Arity mismatch in record field: `struct VTable { read: (u64) -> u64 }` with `&fn_no_args` → T0535
//! 2. Parameter type mismatch in record field: `VTable { read: &takes_u32 }` → T0535
//! 3. Compatible signature in record field: matching types → compilation succeeds

use std::path::PathBuf;
use std::process::Command;

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/pa_r17_1074_t0535_record_field");
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Test 1: Arity mismatch in record field should produce T0535.
///
/// Assigns a function with no parameters to a record field typed as (u64) -> u64.
/// This should fail with T0535 diagnostic (arity mismatch).
#[test]
fn t0535_record_field_arity_mismatch_detected() {
    let input = fixture_dir().join("pa_r17_1074_t0535_record_field_arity_mismatch.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap()]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Signature check must fire: T0535 for arity mismatch in record field
    assert!(
        stderr.contains("T0535") || stderr.contains("535"),
        "Expected T0535 diagnostic for record field arity mismatch.\nStderr: {}",
        stderr
    );
}

/// Test 2: Parameter type mismatch in record field should produce T0535.
///
/// Assigns a function with parameter type mismatch to a record field.
/// This should fail with T0535 diagnostic (parameter type mismatch).
#[test]
fn t0535_record_field_param_type_mismatch_detected() {
    let input = fixture_dir().join("pa_r17_1074_t0535_record_field_param_mismatch.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap()]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Signature check must fire: T0535 for parameter type mismatch in record field
    assert!(
        stderr.contains("T0535") || stderr.contains("535"),
        "Expected T0535 diagnostic for record field param type mismatch.\nStderr: {}",
        stderr
    );
}

/// Test 3: Compatible signature in record field should accept silently.
///
/// Assigns a function with compatible signature to a record field.
/// This should succeed without diagnostic.
#[test]
fn t0535_record_field_compatible_signature_accepts() {
    let input = fixture_dir().join("pa_r17_1074_t0535_record_field_compatible.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap()]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Compatible assignment must NOT produce T0535
    assert!(
        !stderr.contains("T0535") && !stderr.contains("535"),
        "Unexpected T0535 for compatible signature in record field.\nStderr: {}",
        stderr
    );
}
