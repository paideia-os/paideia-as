//! End-to-end CLI test for `paideia-as build --emit elf64`.
//!
//! Closes deliverables 8 + 9 (smoke level): runs the binary on a
//! trivial `.pdx`, asserts the resulting `.o` is parseable ELF64 with
//! the expected magic header and at least one section.

use std::path::PathBuf;
use std::process::Command;

fn data(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/data");
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
fn build_elf64_writes_parseable_object() {
    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_e2e_elf.o");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert!(bytes.len() >= 64, "ELF header is 64 bytes minimum");
    // ELF magic header.
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");
    // ELF64 class.
    assert_eq!(bytes[4], 2, "expected ELF64 (class 2)");
    // Little endian.
    assert_eq!(bytes[5], 1, "expected little-endian (data 1)");

    // The `object` crate parses it back without errors.
    use object::Object;
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");
    assert!(
        file.sections().count() > 0,
        "ELF should have at least one section"
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn build_unknown_emit_format_exits_2() {
    let input = data("hello.pdx");
    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "definitely-not-real",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown --emit format"),
        "expected unknown-format diagnostic, got: {stderr}"
    );
}

#[test]
fn build_placeholder_emit_still_works() {
    // Regression: --emit placeholder is the default and should still
    // produce a placeholder file when explicitly requested.
    let input = data("hello.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_e2e_placeholder.placeholder");
    let _ = std::fs::remove_file(&tmp);

    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "placeholder",
        "-o",
        tmp.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit placeholder failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(tmp.exists(), "placeholder file should be written");
    let _ = std::fs::remove_file(&tmp);
}
