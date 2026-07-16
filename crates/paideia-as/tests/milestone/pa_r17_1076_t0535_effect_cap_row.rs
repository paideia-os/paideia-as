//! PA-r17-1076 (#1076): T0535 fn-ptr signature check for effect and capability rows.
//!
//! This test verifies that function-pointer signature mismatches are detected
//! when assigning lambdas with declared effect/capability rows to fn-ptr bindings.
//!
//! Test cases:
//! 1. Lambda with Io effect assigned to pure fn-ptr slot → T0535
//! 2. Lambda with FileRead capability assigned to uncapped fn-ptr slot → T0535
//! 3. Pure lambda assigned to effectful/capped fn-ptr slot → no T0535 (compatible contravariance)

use std::path::PathBuf;
use std::process::Command;

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/pa_r17_1076_t0535_effect_cap_row");
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Test 1: Effect row mismatch should produce T0535.
///
/// Lambda with Io effect is assigned to binding typed as (u64) -> u64 !{} @{}.
/// This should fail with T0535 diagnostic (effect row mismatch).
#[test]
fn t0535_effect_mismatch_detected() {
    let input = fixture_dir().join("pa_r17_1076_t0535_effect_mismatch.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Signature check must fire: T0535 for effect row mismatch
    assert!(
        stderr.contains("T0535") || stderr.contains("535"),
        "Expected T0535 diagnostic for lambda effect row mismatch.\nStderr: {}",
        stderr
    );
}

/// Test 2: Capability set mismatch should produce T0535.
///
/// Lambda with FileRead capability is assigned to binding typed as (u64) -> u64 !{} @{}.
/// This should fail with T0535 diagnostic (capability set mismatch).
#[test]
fn t0535_cap_mismatch_detected() {
    let input = fixture_dir().join("pa_r17_1076_t0535_cap_mismatch.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Signature check must fire: T0535 for capability set mismatch
    assert!(
        stderr.contains("T0535") || stderr.contains("535"),
        "Expected T0535 diagnostic for lambda capability set mismatch.\nStderr: {}",
        stderr
    );
}

/// Test 3: Compatible effect/cap assignment should accept silently.
///
/// Pure lambda is assigned to binding typed as (u64) -> u64 !{Io} @{FileRead}.
/// This is contravariant - a pure lambda CAN be assigned to a slot requiring effects/caps.
/// This should succeed without diagnostic.
#[test]
fn t0535_effect_cap_compatible_accepts() {
    let input = fixture_dir().join("pa_r17_1076_t0535_effect_cap_compatible.pdx");

    if !input.exists() {
        panic!("Fixture not found: {:?}", input);
    }

    let out = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);

    let stderr = String::from_utf8_lossy(&out.stderr);

    // Compatible assignment must NOT produce T0535
    assert!(
        !stderr.contains("T0535") && !stderr.contains("535"),
        "Unexpected T0535 for compatible lambda effect/cap assignment.\nStderr: {}",
        stderr
    );
}
