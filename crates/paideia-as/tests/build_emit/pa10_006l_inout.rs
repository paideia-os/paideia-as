//! PA10-006l: in_al/out_al I/O port instructions.
//!
//! This test verifies that I/O port read and write instructions (in/out) work
//! correctly in unsafe blocks. The in_al and out_al mnemonics read from and write
//! to I/O ports accessed via the DX register.
//!
//! Expected byte sequences:
//! - `mov al, 0x42`: B0 42
//! - `out_al al`: EE
//! - `in_al al`: EC
//!
//! The test:
//! 1. Invokes the build command on pa10_006l_inout.pdx
//! 2. Reads the resulting .o (ELF) file
//! 3. Extracts the .text section bytes and verifies instruction encoding

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

#[test]
fn pa10_006l_inout_instructions_emit() {
    let input = build_emit_data("pa10_006l_inout.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pa10_006l_inout.o");
    let _ = std::fs::remove_file(&tmp);

    // Build the fixture into ELF64 format
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
        "build --emit elf64 failed for pa10_006l_inout.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Read the ELF file
    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert!(bytes.len() >= 64, "ELF header is 64 bytes minimum");

    // Verify ELF magic and format
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");
    assert_eq!(bytes[4], 2, "expected ELF64 (class 2)");
    assert_eq!(bytes[5], 1, "expected little-endian (data 1)");

    // Parse ELF via object crate
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Extract .text section bytes
    let mut text_bytes = Vec::new();
    let mut found_text = false;
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            found_text = true;
            let data = section.data().expect(".text data should exist");
            text_bytes.extend_from_slice(data);
            break;
        }
    }

    assert!(found_text, ".text section should exist");
    assert!(!text_bytes.is_empty(), ".text section should not be empty");

    // Expected byte sequence:
    // B0 42 - mov al, 0x42
    // EE    - out_al al
    // EC    - in_al al
    let expected = vec![0xB0, 0x42, 0xEE, 0xEC];
    assert_eq!(
        text_bytes, expected,
        "text bytes mismatch: expected {:02X?}, got {:02X?}",
        expected, text_bytes
    );
}
