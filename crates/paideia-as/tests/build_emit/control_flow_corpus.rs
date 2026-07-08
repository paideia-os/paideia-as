//! Phase 6 m4-006: Control-flow corpus integration test.
//!
//! This test verifies that all 6 control-flow fixtures build successfully
//! and emit correct .text section bytes:
//!
//! 1. cmp_reg_reg.pdx (CmpRegReg): cmp rax, rdi => 48 39 F8 (3 bytes)
//! 2. cmp_mem_reg.pdx (CmpMemReg): placeholder for cmp [rdi + 24], rcx (deferred to m4-009+)
//! 3. jne_forward.pdx (JneForward): forward branch placeholder (deferred to m4-007+)
//! 4. jne_backward.pdx (JneBackward): backward branch placeholder (deferred to m4-007+)
//! 5. call_sym.pdx (CallSym): cross-function call => E8 00 00 00 00 ... (5+ bytes)
//! 6. cap_verify_compound.pdx (CapVerifyCompound): multi-CMP hot path (6 bytes)
//!
//! The test:
//! 1. Invokes the build command for each fixture
//! 2. Reads the resulting .o (ELF) files
//! 3. Extracts .text section bytes
//! 4. Compares against expected_bytes.txt snapshots
//! 5. Asserts all fixtures build cleanly (exit code 0)

use object::{Object, ObjectSection};
use std::path::PathBuf;
use std::process::Command;

fn build_emit_data(subdir: &str, name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../tests/build-emit");
    p.push(subdir);
    p.push(name);
    p
}

fn cargo_run(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.arg("run").arg("--quiet").arg("--").args(args);
    cmd.env("NO_COLOR", "1");
    cmd.output().expect("failed to run cargo")
}

/// Parse expected bytes from a text file (format: hex bytes, space-separated or lines).
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

/// Helper to extract .text section from ELF.
fn extract_text_section(elf_bytes: &[u8]) -> Vec<u8> {
    match object::File::parse(elf_bytes) {
        Ok(file) => {
            for section in file.sections() {
                if section.name().unwrap_or("") == ".text" {
                    return section.data().unwrap_or(b"").to_vec();
                }
            }
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

/// Test build and byte-match for a single fixture.
fn test_fixture(name: &str, _pascal_case_module: &str) {
    let input = build_emit_data("control_flow", &format!("{}.pdx", name));
    let tmp = std::env::temp_dir().join(format!("paideia_as_control_flow_{}.o", name));
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

    assert_eq!(
        out.status.code(),
        Some(0),
        "build --emit elf64 failed for {}.pdx: {}",
        name,
        String::from_utf8_lossy(&out.stderr)
    );

    // Read the ELF file
    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert!(bytes.len() >= 64, "ELF header is 64 bytes minimum");

    // Verify ELF magic and format
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");
    assert_eq!(bytes[4], 2, "expected ELF64 (class 2)");
    assert_eq!(bytes[5], 1, "expected little-endian (data 1)");

    // Extract .text section bytes
    let text_bytes = extract_text_section(&bytes);
    assert!(!text_bytes.is_empty(), ".text section must exist in ELF");

    // Read expected bytes from expected_bytes.txt
    let expected_path = build_emit_data("control_flow", &format!("{}.expected_bytes.txt", name));
    let expected_text = std::fs::read_to_string(&expected_path)
        .expect(&format!("{}.expected_bytes.txt should exist", name));
    let expected_bytes = parse_expected_bytes(&expected_text);

    // Assert byte-for-byte match
    if text_bytes != expected_bytes {
        eprintln!(
            "MISMATCH in {}: emitted .text bytes do not match expected\n\
             Expected ({} bytes): {:02X?}\n\
             Got ({} bytes):      {:02X?}",
            name,
            expected_bytes.len(),
            expected_bytes,
            text_bytes.len(),
            text_bytes
        );
        panic!(
            ".text section mismatch in {}: expected {} bytes, got {}",
            name,
            expected_bytes.len(),
            text_bytes.len()
        );
    }

    let _ = std::fs::remove_file(&tmp);
}

/// Phase 6 m4-006: Test all 6 control-flow fixtures build and emit correct bytes.
#[test]
fn build_emit_control_flow_corpus() {
    // Test each fixture with (basename, PascalCaseModule)
    test_fixture("cmp_reg_reg", "CmpRegReg");
    test_fixture("cmp_mem_reg", "CmpMemReg");
    test_fixture("jne_forward", "JneForward");
    test_fixture("jne_backward", "JneBackward");
    test_fixture("call_sym", "CallSym");
    test_fixture("cap_verify_compound", "CapVerifyCompound");
}
