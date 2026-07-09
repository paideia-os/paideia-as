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

    // Locate _start symbol and its section-relative address.
    // .text[0] is NOT _start: the fixture declares `target` (mov+ret, 7 bytes)
    // before `_start`, so we must resolve _start's offset within .text before
    // inspecting the ljmp opcode.
    let start_sym = file
        .symbols()
        .find(|s| s.name().unwrap_or("") == "_start")
        .expect("_start symbol should exist");
    assert!(start_sym.size() > 0, "_start should have non-zero size");
    let start_addr = start_sym.address();

    let text = file
        .sections()
        .find(|s| s.name().unwrap_or("") == ".text")
        .expect(".text section should exist");
    let data = text.data().expect(".text data should exist");
    assert!(!data.is_empty(), ".text section should not be empty");
    let text_addr = text.address();
    let start_offset = (start_addr - text_addr) as usize;

    // Verify _start begins with the 7-byte ljmp EA form: EA imm32(=0 pre-reloc) sel16(=0x0008).
    // The imm32 is the offset field patched later by R_X86_64_32 targeting `target`.
    let expected = [0xEA, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
    assert!(
        start_offset + expected.len() <= data.len(),
        "_start (offset {start_offset}) + ljmp bytes ({}) exceed .text length {}",
        expected.len(),
        data.len()
    );
    assert_eq!(
        &data[start_offset..start_offset + expected.len()],
        &expected,
        "_start bytes at .text[{}..{}] should be EA 00 00 00 00 08 00 (ljmp $0x8,target)",
        start_offset,
        start_offset + expected.len()
    );
}
