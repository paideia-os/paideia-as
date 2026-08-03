//! Issue #1251: unsafe_walker store-direction retarget integration test.
//!
//! This test verifies that `mov [mem], reg` with narrow-width source registers
//! (W8/W16/W32) are correctly retargeted to MovSized, emitting the correct bytes
//! to .text.
//!
//! Fixture: tests/build-emit/narrow_store_reg_source.pdx
//! Expected bytes (11 total):
//!   mov [rdi], edx   → 89 17
//!   mov [rdi], dx    → 66 89 17
//!   mov [rdi], dl    → 88 17
//!   mov [rdi], rdx   → 48 89 17
//!   ret              → C3

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

/// Test that narrow-width register stores are correctly encoded to .text.
#[test]
fn narrow_store_reg_source_emits_correct_bytes() {
    let input = build_emit_data("narrow_store_reg_source.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_narrow_store_emit.o");
    let _ = std::fs::remove_file(&tmp);

    // Build the narrow_store_reg_source.pdx into ELF64 format
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
        "build --emit elf64 failed for narrow_store_reg_source.pdx: {}",
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

    // Expected bytes:
    // mov [rdi], edx   → 89 17         (W32 store)
    // mov [rdi], dx    → 66 89 17      (W16 store)
    // mov [rdi], dl    → 88 17         (W8  store)
    // mov [rdi], rdx   → 48 89 17      (W64 unchanged; regression guard)
    // ret              → C3
    let expected_bytes = vec![
        0x89, 0x17,             // mov [rdi], edx     — W32
        0x66, 0x89, 0x17,       // mov [rdi], dx      — W16
        0x88, 0x17,             // mov [rdi], dl      — W8
        0x48, 0x89, 0x17,       // mov [rdi], rdx     — W64 (unchanged; regression guard)
        0xC3,                   // ret
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
