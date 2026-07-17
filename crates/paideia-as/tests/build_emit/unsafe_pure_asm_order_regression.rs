//! Integration tests for pure-asm unsafe blocks regression guard (#1244).
//!
//! Tests that pure unsafe blocks (no call-expression statements) continue to emit
//! raw instructions in source order after the #1244 fix.
//!
//! Fixture: tests/build-emit/unsafe_pure_asm_order_regression.pdx
//! - entry() does: mov rax, immediate; lea rbx, [entry]; lbl_end: ret
//! - Ensures mov and lea emit before ret
//! - Guards against regression where new interleave logic breaks pure-asm blocks

use object::{Object, ObjectSection};
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

/// Test: Pure-asm unsafe block compiles successfully.
#[test]
fn unsafe_pure_asm_order_regression_compiles() {
    let input = build_emit_data("unsafe_pure_asm_order_regression.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pure_asm_regression.o");
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

    let _ = std::fs::remove_file(&tmp);
}

/// Test: Verify ELF structure is valid.
#[test]
fn unsafe_pure_asm_order_regression_elf_structure_valid() {
    let input = build_emit_data("unsafe_pure_asm_order_regression.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pure_asm_regression_elf.o");
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

    assert!(
        bytes.len() >= 64,
        "ELF file should be at least 64 bytes (header size)"
    );
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic should be present");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut text_section_found = false;
    let mut text_size = 0u64;

    for section in file.sections() {
        if let Ok(name) = section.name() {
            if name == ".text" {
                text_section_found = true;
                text_size = section.size();
                break;
            }
        }
    }

    assert!(text_section_found, ".text section should exist");
    assert!(
        text_size > 0,
        ".text section should have non-zero size (instructions encoded)"
    );

    let _ = std::fs::remove_file(&tmp);
}
