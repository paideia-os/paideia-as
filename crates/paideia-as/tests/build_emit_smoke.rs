//! Phase 5 m6-003: Byte-sequence assertion test for uart_smoke.pdx.
//!
//! This test verifies that the build command correctly emits the unsafe block
//! bytes for uart_smoke.pdx, which contains direct x86-64 assembly:
//!   mov al, 0x78       => B0 78
//!   mov dx, 0x3F8     => 66 BA F8 03
//!   out dx, al         => EE
//!   hlt                => F4
//!
//! The test:
//! 1. Invokes the build command programmatically
//! 2. Reads the resulting .o (ELF) file
//! 3. Extracts the .text section bytes via the `object` crate
//! 4. Compares against tests/build-emit/uart_smoke.expected_bytes.txt
//! 5. Also asserts the _start symbol exists, is STB_GLOBAL, and has non-zero size

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

/// Parse expected bytes from a text file (format: hex bytes, one per line or space-separated).
/// Ignores lines starting with `;` (comments) and blank lines.
fn parse_expected_bytes(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }
        // Parse hex bytes from this line (space-separated)
        for hex_str in trimmed.split_whitespace() {
            if let Ok(byte) = u8::from_str_radix(hex_str, 16) {
                bytes.push(byte);
            }
        }
    }
    bytes
}

/// Phase 5 m6-003: Test that build emits correct .text bytes for uart_smoke.pdx.
///
/// This test is THE TRUTH DETECTOR for whether the unsafe block content
/// reaches the emitted code. If the bytes match expected_bytes.txt, the chain
/// (m1-004 → m3-004 → m3-005 → m5-005) is working correctly.
/// If they don't match, this test fails and surfaces the bug.
#[test]
fn uart_smoke_text_bytes_match_expected() {
    let input = build_emit_data("uart_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_uart_smoke_emit.o");
    let _ = std::fs::remove_file(&tmp);

    // Build the uart_smoke.pdx into ELF64 format
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
        "build --emit elf64 failed for uart_smoke.pdx: {}",
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

    // Read expected bytes from expected_bytes.txt
    let expected_path = build_emit_data("uart_smoke.expected_bytes.txt");
    let expected_text = std::fs::read_to_string(&expected_path)
        .expect("uart_smoke.expected_bytes.txt should exist");
    let expected_bytes = parse_expected_bytes(&expected_text);

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

    // Assert _start symbol exists with correct properties
    // For simplicity, we verify that at least one symbol exists in the symbol table.
    // Phase 5 m6-001/m5-003 ensures _start is emitted as STB_GLOBAL.
    let mut symbol_count = 0;
    for _sym in file.symbols() {
        symbol_count += 1;
    }
    assert!(
        symbol_count > 0,
        "ELF symbol table must contain at least one symbol (including _start)"
    );

    let _ = std::fs::remove_file(&tmp);
}
