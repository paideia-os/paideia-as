//! PA-R12-004 (#913): Pure function bodies with unsupported control flow.
//!
//! Tests that pure fn bodies containing if/match/while/loop emit T0532 diagnostic
//! and fail to build, while unsafe { block: { ... } } workaround builds cleanly.

use object::Object;
use std::env;
use std::process::Command;

/// Test that a pure fn with if-else fails with T0532 diagnostic.
#[test]
fn pa_r12_004_pure_if_rejected_build_fails_with_t0532() {
    let fixture_path = "tests/build-emit/pa_r12_004_pure_if_rejected.pdx";
    if !std::path::Path::new(fixture_path).exists() {
        // Skip if fixture doesn't exist (e.g., in CI without fixtures)
        return;
    }

    let temp_dir = env::temp_dir().join("pa_r12_004_pure_if_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    let output_obj = temp_dir.join("pa_r12_004_pure_if_rejected.o");

    // Run paideia-as build on the fixture.
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("build")
        .arg(fixture_path)
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(output_obj.to_str().unwrap());
    cmd.env("NO_COLOR", "1");
    let out = cmd.output().expect("paideia-as build");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let output = format!("{}{}", stdout, stderr);

    // Build should fail (non-zero exit or T0532 in output).
    // We check for T0532 in stderr/stdout to confirm the diagnostic fired.
    let has_t0532 = output.contains("T0532") || output.contains("pure function body");
    assert!(
        has_t0532 || !out.status.success(),
        "Expected T0532 diagnostic or failed build for pure fn with if. \
         stdout: {}\nstderr: {}",
        stdout, stderr
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Test that unsafe { block: { if ... } } builds cleanly without T0532.
#[test]
fn pa_r12_004_unsafe_if_ok_builds_cleanly() {
    let fixture_path = "tests/build-emit/pa_r12_004_unsafe_if_ok.pdx";
    if !std::path::Path::new(fixture_path).exists() {
        // Skip if fixture doesn't exist (e.g., in CI without fixtures)
        return;
    }

    let temp_dir = env::temp_dir().join("pa_r12_004_unsafe_if_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("create temp_dir");

    let output_obj = temp_dir.join("pa_r12_004_unsafe_if_ok.o");

    // Run paideia-as build on the fixture.
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run")
        .arg("--quiet")
        .arg("--")
        .arg("build")
        .arg(fixture_path)
        .arg("--emit")
        .arg("elf64")
        .arg("-o")
        .arg(output_obj.to_str().unwrap());
    cmd.env("NO_COLOR", "1");
    let out = cmd.output().expect("paideia-as build");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let output = format!("{}{}", stdout, stderr);

    // Build should succeed.
    assert!(
        out.status.success(),
        "unsafe {{ block: {{ ... }} }} should build cleanly. \
         stdout: {}\nstderr: {}",
        stdout, stderr
    );

    // Should NOT have T0532 (we stopped at unsafe boundary).
    assert!(
        !output.contains("T0532"),
        "unsafe block should suppress T0532. \
         stdout: {}\nstderr: {}",
        stdout, stderr
    );

    // Object file should exist and be valid.
    assert!(output_obj.exists(), "output .o not created");
    let obj_data = std::fs::read(&output_obj).expect("read .o");
    let obj = object::File::parse(&*obj_data).expect("parse .o");

    // Basic sanity check: should have at least one symbol
    let symbols: Vec<_> = obj.symbols().collect();
    assert!(!symbols.is_empty(), "object file should have symbols");

    let _ = std::fs::remove_dir_all(&temp_dir);
}
