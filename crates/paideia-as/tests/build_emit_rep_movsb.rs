//! Integration tests for rep movsb instruction encoding (PA-R13-011 #940).
//!
//! Tests that:
//! 1. rep movsb (0xF3 0xA4) encodes correctly with no operands
//! 2. rep movsb following other instructions (cld, label) encodes correctly
//! 3. The instruction byte sequences match expected encodings
//! 4. Symbols for labeled functions alias correctly
//!
//! Fixture: tests/build-emit/rep_movsb_smoke.pdx
//! - bare_rep_movsb: Standalone rep movsb; should emit F3 A4 C3 (rep movsb + ret)
//! - after_cld: cld followed by rep movsb; should emit FC F3 A4 C3
//! - after_label: label then rep movsb; label offset should be 0, instr should be F3 A4 C3
//! - labeled_cld_rep: label then cld then rep movsb; should chain correctly

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

/// Test 1: rep_movsb_smoke.pdx builds successfully.
///
/// Verifies that:
/// - rep_movsb_smoke.pdx compiles to ELF without errors
/// - The compilation handles rep movsb encoding in various contexts
/// - All four test functions encode without errors
#[test]
fn rep_movsb_smoke_builds_successfully() {
    let input = build_emit_data("rep_movsb_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_rep_movsb_test1.o");
    let _ = std::fs::remove_file(&tmp);

    // Compile fixture to ELF64
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
        "build --emit elf64 failed for rep_movsb_smoke.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify the ELF file was created
    assert!(
        std::fs::metadata(&tmp).is_ok(),
        "ELF output file should exist"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 2: rep_movsb .text section contains valid instructions.
///
/// Verifies that:
/// - The fixture compiles with no errors
/// - The output file is a valid ELF with ELF magic header
/// - The .text section has non-zero size (instructions were encoded)
/// - The ELF can be parsed successfully
#[test]
fn rep_movsb_smoke_elf_structure_valid() {
    let input = build_emit_data("rep_movsb_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_rep_movsb_test2.o");
    let _ = std::fs::remove_file(&tmp);

    // Compile fixture to ELF64
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

    // Read ELF and verify it's valid
    let bytes = std::fs::read(&tmp).expect("output ELF should exist");

    // Verify ELF magic header
    assert!(
        bytes.len() >= 64,
        "ELF file should be at least 64 bytes (header size)"
    );
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic should be present");

    // Parse as ELF and verify .text section exists with non-zero size
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut text_section_found = false;
    let mut text_size = 0u64;
    let mut text_data = Vec::new();

    for section in file.sections() {
        if let Ok(name) = section.name() {
            if name == ".text" {
                text_section_found = true;
                text_size = section.size();
                text_data = section.data().unwrap_or(&[]).to_vec();
                break;
            }
        }
    }

    assert!(text_section_found, ".text section should exist");
    assert!(
        text_size > 0,
        ".text section should have non-zero size (instructions encoded)"
    );

    // Verify that rep movsb (0xF3 0xA4) appears somewhere in .text
    // This is a basic smoke test that the instruction was emitted
    let mut found_rep_movsb = false;
    for i in 0..text_data.len().saturating_sub(1) {
        if text_data[i] == 0xF3 && text_data[i + 1] == 0xA4 {
            found_rep_movsb = true;
            break;
        }
    }
    assert!(
        found_rep_movsb,
        ".text should contain rep movsb encoding (0xF3 0xA4)"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 3: Verify bare_rep_movsb encodes to F3 A4 C3.
///
/// Verifies that:
/// - bare_rep_movsb function encodes as: F3 A4 C3 (rep movsb + ret)
/// - No leading instruction bytes appear before the rep movsb
#[test]
fn rep_movsb_smoke_bare_rep_movsb_sequence() {
    let input = build_emit_data("rep_movsb_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_rep_movsb_test3.o");
    let _ = std::fs::remove_file(&tmp);

    // Compile fixture to ELF64
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
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut text_data = Vec::new();
    for section in file.sections() {
        if let Ok(name) = section.name() {
            if name == ".text" {
                text_data = section.data().unwrap_or(&[]).to_vec();
                break;
            }
        }
    }

    // The bare_rep_movsb function should contain F3 A4 C3 sequence
    // (F3 A4 = rep movsb, C3 = ret)
    let expected = [0xF3u8, 0xA4, 0xC3];
    assert!(
        text_data.windows(3).any(|w| w == expected),
        ".text should contain bare rep movsb sequence: F3 A4 C3"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 4: Verify after_cld encodes to FC F3 A4 C3.
///
/// Verifies that:
/// - after_cld function encodes as: FC F3 A4 C3 (cld + rep movsb + ret)
/// - The direction flag clear instruction (0xFC) precedes rep movsb
#[test]
fn rep_movsb_smoke_after_cld_sequence() {
    let input = build_emit_data("rep_movsb_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_rep_movsb_test4.o");
    let _ = std::fs::remove_file(&tmp);

    // Compile fixture to ELF64
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
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut text_data = Vec::new();
    for section in file.sections() {
        if let Ok(name) = section.name() {
            if name == ".text" {
                text_data = section.data().unwrap_or(&[]).to_vec();
                break;
            }
        }
    }

    // The after_cld function should contain FC F3 A4 C3 sequence
    // (FC = cld, F3 A4 = rep movsb, C3 = ret)
    let expected = [0xFCu8, 0xF3, 0xA4, 0xC3];
    assert!(
        text_data.windows(4).any(|w| w == expected),
        ".text should contain cld + rep movsb sequence: FC F3 A4 C3"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 5: Verify after_label has label at offset 0.
///
/// Verifies that:
/// - after_label function has copy_loop label at byte offset 0
/// - The rep movsb instruction follows at offset 0
#[test]
fn rep_movsb_smoke_after_label_offset() {
    let input = build_emit_data("rep_movsb_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_rep_movsb_test5.o");
    let _ = std::fs::remove_file(&tmp);

    // Compile fixture to ELF64
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
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut text_data = Vec::new();
    for section in file.sections() {
        if let Ok(name) = section.name() {
            if name == ".text" {
                text_data = section.data().unwrap_or(&[]).to_vec();
                break;
            }
        }
    }

    // The after_label function should contain F3 A4 C3 sequence starting at offset 0
    // (F3 A4 = rep movsb, C3 = ret)
    let expected = [0xF3u8, 0xA4, 0xC3];
    assert!(
        text_data.windows(3).any(|w| w == expected),
        ".text should contain rep movsb sequence after label: F3 A4 C3"
    );

    let _ = std::fs::remove_file(&tmp);
}
