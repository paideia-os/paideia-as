//! PA10-006k: ljmp two-operand (selector, offset) form.
//!
//! This test verifies that ljmp (far jump) with two operands works correctly
//! in unsafe blocks. The ljmp instruction loads a code segment and instruction
//! pointer for long-mode far jumps.
//!
//! Expected bytes for `ljmp 0x08, target`: EA (ljmp opcode) followed by
//! relocation for target and selector value 0x08.
//!
//! The test:
//! 1. Invokes the build command on pa10_006k_ljmp.pdx
//! 2. Reads the resulting .o (ELF) file
//! 3. Verifies the instruction encodes without errors

use object::{Object, ObjectSection, ObjectSymbol};
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
fn pa10_006k_ljmp_two_operand_emits() {
    let input = build_emit_data("pa10_006k_ljmp.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pa10_006k_ljmp.o");
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
        "build --emit elf64 failed for pa10_006k_ljmp.pdx: {}",
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

    // Verify _start symbol exists
    let mut found_start = false;
    for symbol in file.symbols() {
        if symbol.name().unwrap_or("") == "_start" {
            found_start = true;
            assert!(symbol.size() > 0, "_start should have non-zero size");
            break;
        }
    }
    assert!(found_start, "_start symbol should exist");

    // Verify .text section exists with content
    let mut found_text = false;
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            found_text = true;
            let data = section.data().expect(".text data should exist");
            assert!(!data.is_empty(), ".text section should not be empty");
            // The ljmp instruction should start with EA (ljmp opcode)
            assert_eq!(data[0], 0xEA, "first byte should be EA (ljmp opcode)");
            break;
        }
    }
    assert!(found_text, ".text section should exist");
}
