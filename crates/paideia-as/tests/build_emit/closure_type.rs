//! Issue #994 — closure-type grammar + elaborator dispatch surface tests.
//!
//! Six fixtures under `tests/build-emit/closure_type_*.pdx`:
//! - `closure_type_no_capture`: module-level closure type, no free variables.
//! - `closure_type_value_capture` / `closure_type_two_captures`: function-
//!   local closures capturing one / two outer locals — exercise the
//!   `IrKind::ClosureCons` conversion (piece 2) and `emit_closure_cons`
//!   (piece 3) end to end.
//! - `closure_type_linear_ref_capture`: a `linear`-classed capture, positive
//!   (must not fire S0907).
//! - `closure_type_arity_mismatch` (negative): closure type / lambda literal
//!   parameter-count mismatch — expects T0535.
//! - `closure_type_fnptr_with_captures` (negative): plain fn-ptr annotation
//!   on a lambda that actually captures — expects T0571.
//!
//! Positive fixtures are built with `--emit elf64` (full pipeline, including
//! the ELF/link-facing symbol + relocation machinery); negative fixtures use
//! `--emit placeholder` (diagnostic surface only, mirroring
//! `pa_r17_003b_t0535.rs`'s existing T0535 wiring pattern) since they are not
//! expected to reach a linkable object.

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

fn assert_build_succeeds(fixture: &str, output_name: &str) {
    let input = build_emit_data(fixture);
    let output_path = format!("/tmp/test_{output_name}.o");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        &output_path,
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "Build of {fixture} failed unexpectedly.\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
    );
    assert!(
        std::path::Path::new(&output_path).exists(),
        "Output ELF file not created for {fixture} at {output_path}"
    );
}

#[test]
fn closure_type_no_capture_builds() {
    assert_build_succeeds("closure_type_no_capture.pdx", "closure_type_no_capture");
}

#[test]
fn closure_type_value_capture_builds() {
    assert_build_succeeds("closure_type_value_capture.pdx", "closure_type_value_capture");
}

#[test]
fn closure_type_two_captures_builds() {
    assert_build_succeeds("closure_type_two_captures.pdx", "closure_type_two_captures");
}

#[test]
fn closure_type_linear_ref_capture_builds() {
    assert_build_succeeds(
        "closure_type_linear_ref_capture.pdx",
        "closure_type_linear_ref_capture",
    );
}

/// Negative: closure type declares 2 params, lambda literal has 1 → T0535.
#[test]
fn closure_type_arity_mismatch_rejects_t0535() {
    let input = build_emit_data("closure_type_arity_mismatch.pdx");
    let output = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("T0535") || stderr.contains("535"),
        "Expected T0535 diagnostic for closure arity mismatch.\nStderr: {stderr}"
    );
}

/// Negative: fn-ptr annotation on a lambda that actually captures → T0571.
#[test]
fn closure_type_fnptr_with_captures_rejects_t0571() {
    let input = build_emit_data("closure_type_fnptr_with_captures.pdx");
    let output = cargo_run(&["build", input.to_str().unwrap(), "--emit", "placeholder"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("T0571") || stderr.contains("571"),
        "Expected T0571 diagnostic for fn-ptr-annotated lambda with captures.\nStderr: {stderr}"
    );
}
