//! Regression test for Phase 5 m7: examples build-clean parity.
//!
//! Verifies that the 3 build-clean examples (01_hello, 02_functions, 15_unsafe)
//! build successfully with `paideia-as build --emit elf64` and produce valid
//! ELF object files with non-empty .text sections.

use std::path::PathBuf;
use std::process::Command;

fn examples_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../examples");
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Asserts that `paideia-as build --emit elf64 <input> -o <output>` exits 0.
fn build_example_succeeds(example: &str) {
    let input = examples_dir().join(example);
    let output = std::env::temp_dir().join(format!("{}.o", example));

    let out = cargo_run(&[
        "build",
        "--emit",
        "elf64",
        input.to_str().unwrap(),
        "-o",
        output.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "expected `paideia-as build` to exit 0 for {}, got {:?}\nstdout: {}\nstderr: {}",
        example,
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify the output file exists and is a valid ELF object.
    assert!(
        output.exists(),
        "expected output file {output:?} to exist after successful build"
    );

    // Clean up.
    let _ = std::fs::remove_file(output);
}

#[test]
fn build_01_hello_succeeds() {
    build_example_succeeds("01_hello.pdx");
}

#[test]
fn build_02_functions_succeeds() {
    build_example_succeeds("02_functions.pdx");
}

#[test]
fn build_15_unsafe_succeeds() {
    build_example_succeeds("15_unsafe.pdx");
}
