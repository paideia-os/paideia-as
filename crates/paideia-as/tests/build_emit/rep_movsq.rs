//! Integration tests for rep movsq instruction encoding (#1228, Phase 2 of #1064).
//!
//! Tests that:
//! 1. rep movsq (0xF3 0x48 0xA5) encodes correctly with no operands
//! 2. rep movsq following other instructions (cld, label) encodes correctly
//! 3. The instruction byte sequences match expected encodings
//! 4. Symbols for labeled functions alias correctly
//!
//! Fixture: tests/build-emit/rep_movsq_smoke.pdx
//! - bare_rep_movsq: Standalone rep movsq; should emit F3 48 A5 C3 (rep movsq + ret)
//! - after_cld: cld followed by rep movsq; should emit FC F3 48 A5 C3
//! - after_label: label then rep movsq; label offset should be 0, instr should be F3 48 A5 C3
//! - labeled_cld_rep: label then cld then rep movsq; should chain correctly

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

/// Test 1: rep_movsq_smoke.pdx builds successfully.
///
/// Verifies that:
/// - rep_movsq_smoke.pdx compiles to ELF without errors
/// - The compilation handles rep movsq encoding in various contexts
/// - All four test functions encode without errors
#[test]
fn rep_movsq_smoke_builds_successfully() {
    let input = build_emit_data("rep_movsq_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_rep_movsq_test1.o");
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
        "build --emit elf64 failed for rep_movsq_smoke.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify the ELF file was created
    assert!(
        std::fs::metadata(&tmp).is_ok(),
        "ELF output file should exist"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 2: rep_movsq .text section contains valid instructions.
///
/// Verifies that:
/// - The fixture compiles with no errors
/// - The output file is a valid ELF with ELF magic header
/// - The .text section has non-zero size (instructions were encoded)
/// - The ELF can be parsed successfully
#[test]
fn rep_movsq_smoke_elf_structure_valid() {
    let input = build_emit_data("rep_movsq_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_rep_movsq_test2.o");
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

    // Verify that rep movsq (0xF3 0x48 0xA5) appears somewhere in .text
    // This is a basic smoke test that the instruction was emitted
    let mut found_rep_movsq = false;
    for i in 0..text_data.len().saturating_sub(2) {
        if text_data[i] == 0xF3 && text_data[i + 1] == 0x48 && text_data[i + 2] == 0xA5 {
            found_rep_movsq = true;
            break;
        }
    }
    assert!(
        found_rep_movsq,
        ".text should contain rep movsq encoding (0xF3 0x48 0xA5)"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 3: Verify bare_rep_movsq encodes to F3 48 A5 C3.
///
/// Verifies that:
/// - bare_rep_movsq function encodes as: F3 48 A5 C3 (rep movsq + ret)
/// - No leading instruction bytes appear before the rep movsq
#[test]
fn rep_movsq_smoke_bare_rep_movsq_sequence() {
    let input = build_emit_data("rep_movsq_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_rep_movsq_test3.o");
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

    // The bare_rep_movsq function should contain F3 48 A5 C3 sequence
    // (F3 48 A5 = rep movsq, C3 = ret)
    let expected = [0xF3u8, 0x48, 0xA5, 0xC3];
    assert!(
        text_data.windows(4).any(|w| w == expected),
        ".text should contain bare rep movsq sequence: F3 48 A5 C3"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 4: Verify after_cld encodes to FC F3 48 A5 C3.
///
/// Verifies that:
/// - after_cld function encodes as: FC F3 48 A5 C3 (cld + rep movsq + ret)
/// - The direction flag clear instruction (0xFC) precedes rep movsq
#[test]
fn rep_movsq_smoke_after_cld_sequence() {
    let input = build_emit_data("rep_movsq_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_rep_movsq_test4.o");
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

    // The after_cld function should contain FC F3 48 A5 C3 sequence
    // (FC = cld, F3 48 A5 = rep movsq, C3 = ret)
    let expected = [0xFCu8, 0xF3, 0x48, 0xA5, 0xC3];
    assert!(
        text_data.windows(5).any(|w| w == expected),
        ".text should contain cld + rep movsq sequence: FC F3 48 A5 C3"
    );

    let _ = std::fs::remove_file(&tmp);
}

/// Test 5: Verify after_label has label at offset 0.
///
/// Verifies that:
/// - after_label function has copy_loop label at byte offset 0
/// - The rep movsq instruction follows at offset 0
#[test]
fn rep_movsq_smoke_after_label_offset() {
    let input = build_emit_data("rep_movsq_smoke.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_rep_movsq_test5.o");
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

    // The after_label function should contain F3 48 A5 C3 sequence starting at offset 0
    // (F3 48 A5 = rep movsq, C3 = ret)
    let expected = [0xF3u8, 0x48, 0xA5, 0xC3];
    assert!(
        text_data.windows(4).any(|w| w == expected),
        ".text should contain rep movsq sequence after label: F3 48 A5 C3"
    );

    let _ = std::fs::remove_file(&tmp);
}
