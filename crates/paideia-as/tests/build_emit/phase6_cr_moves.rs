//! Phase 6 m1-002: CR move dispatch integration test.
//!
//! This test verifies that `mov cr*, gpr` and `mov gpr, cr*` instructions
//! are correctly routed through the classifier and encode_mov_cr, emitting
//! the correct bytes to .text.
//!
//! Fixture: tests/build-emit/long_mode_cr_moves.pdx
//! Expected bytes (9 total):
//!   mov cr3, rdi → 0F 22 DF
//!   mov cr4, rcx → 0F 22 E1
//!   mov cr0, rax → 0F 22 C0

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

/// Test that CR moves are correctly encoded to .text.
#[test]
fn long_mode_cr_moves_emits_correct_bytes() {
    let input = build_emit_data("long_mode_cr_moves.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_cr_moves_emit.o");
    let _ = std::fs::remove_file(&tmp);

    // Build the long_mode_cr_moves.pdx into ELF64 format
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
        "build --emit elf64 failed for long_mode_cr_moves.pdx: {}",
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
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(found_text, ".text section must exist in ELF");

    // Expected bytes: mov cr3, rdi (0F 22 DF) + mov cr4, rcx (0F 22 E1) + mov cr0, rax (0F 22 C0)
    let expected_bytes = vec![
        0x0F, 0x22, 0xDF, // mov cr3, rdi
        0x0F, 0x22, 0xE1, // mov cr4, rcx
        0x0F, 0x22, 0xC0, // mov cr0, rax
    ];

    // Assert byte-for-byte match
    if text_bytes != expected_bytes {
        eprintln!(
            "MISMATCH: emitted .text bytes do not match expected\n\
             Expected ({} bytes): {:02X?}\n\
             Got ({} bytes):      {:02X?}",
            expected_bytes.len(),
            expected_bytes,
            text_bytes.len(),
            text_bytes
        );
        panic!(
            ".text section mismatch: expected {} bytes, got {}",
            expected_bytes.len(),
            text_bytes.len()
        );
    }

    let _ = std::fs::remove_file(&tmp);
}
