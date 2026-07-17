//! v0.18 #997: Option<u64> Some extract integration test
//!
//! Tests that the option_u64_some_extract.pdx fixture compiles successfully
//! and produces the expected ELF output.

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

#[test]
fn option_u64_some_extract_builds_successfully() {
    let input = build_emit_data("option_u64_some_extract.pdx");
    let output = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        "/tmp/test_option_u64_some_extract.o",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Build failed. stdout:\n{}\n\nstderr:\n{}",
        stdout,
        stderr
    );
}
