//! PA-r17-1075 (#1075): T0535 fn-ptr signature check for lambda return types.
//!
//! This test verifies that function-pointer signature mismatches are detected
//! when assigning lambdas with inferred return types to fn-ptr bindings.
//!
//! Test cases:
//! 1. Bool literal return in lambda typed as (u64) -> u64 → T0535
//! 2. Suffixed u32 literal return in lambda typed as (u64) -> u64 → T0535
//! 3. Compatible u64 literal return in lambda typed as (u64) -> u64 → compilation succeeds

use std::path::PathBuf;
use std::process::Command;

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/pa_r17_1075_t0535_return_type");
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Test 1: Bool literal return mismatch should produce T0535.
///
/// Lambda with body `true` is assigned to binding typed as (u64) -> u64.
/// This should fail with T0535 diagnostic (return type mismatch).
#[test]
fn t0535_return_type_mismatch_bool_detected() {
    let input = fixture_dir().join("pa_r17_1075_t0535_return_type_mismatch_bool.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Signature check must fire: T0535 for return type mismatch
    assert!(
        stderr.contains("T0535") || stderr.contains("535"),
        "Expected T0535 diagnostic for lambda return type mismatch (bool).\nStderr: {}",
        stderr
    );
}

/// Test 2: Suffixed integer return mismatch should produce T0535.
///
/// Lambda with body `42u32` is assigned to binding typed as (u64) -> u64.
/// This should fail with T0535 diagnostic (return type mismatch).
#[test]
fn t0535_return_type_mismatch_suffix_detected() {
    let input = fixture_dir().join("pa_r17_1075_t0535_return_type_mismatch_suffix.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Signature check must fire: T0535 for return type mismatch
    assert!(
        stderr.contains("T0535") || stderr.contains("535"),
        "Expected T0535 diagnostic for lambda return type mismatch (suffixed int).\nStderr: {}",
        stderr
    );
}

/// Test 3: Compatible return type should accept silently.
///
/// Lambda with body `42u64` is assigned to binding typed as (u64) -> u64.
/// This should succeed without diagnostic.
#[test]
fn t0535_return_type_compatible_accepts() {
    let input = fixture_dir().join("pa_r17_1075_t0535_return_type_compatible.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Compatible assignment must NOT produce T0535
    assert!(
        !stderr.contains("T0535") && !stderr.contains("535"),
        "Unexpected T0535 for compatible lambda return type.\nStderr: {}",
        stderr
    );
}
