//! PA10-006s: Array element width calculation.
//!
//! This integration test verifies that arrays with different element types
//! are packed with the correct per-element width, not a fixed 8-byte width.
//!
//! Test cases:
//! - [u8; 10] should emit 10 bytes (1 byte per element)
//! - [u16; 3] should emit 6 bytes (2 bytes per element)
//! - [u32; 4] should emit 16 bytes (4 bytes per element)
//! - [u64; 2] should emit 16 bytes (8 bytes per element)

use object::{Object, ObjectSymbol};
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

fn build_and_check_symbol_size(fixture_name: &str, symbol_name: &str, expected_size: u64) {
    let input = build_emit_data(fixture_name);
    let tmp_path = std::env::temp_dir().join(format!("paideia_as_{}.o", fixture_name));
    let _ = std::fs::remove_file(&tmp_path);

    // Build the fixture into ELF64 format
    let out = cargo_run(&[
        "build",
        input.to_str().unwrap(),
        "--emit",
        "elf64",
        "-o",
        tmp_path.to_str().unwrap(),
    ]);

    assert!(
        out.status.success(),
        "build --emit elf64 failed for {}.pdx: {}",
        fixture_name,
        String::from_utf8_lossy(&out.stderr)
    );

    // Read the ELF file
    let bytes = std::fs::read(&tmp_path).expect("output ELF should exist");
    assert!(bytes.len() >= 64, "ELF header is 64 bytes minimum");

    // Parse ELF via object crate
    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    // Find the symbol and verify its size
    let mut found = false;
    for symbol in file.symbols() {
        if symbol.name().unwrap_or("") == symbol_name {
            found = true;
            assert_eq!(
                symbol.size(),
                expected_size,
                "{}: {} symbol has size {}, expected {}",
                fixture_name,
                symbol_name,
                symbol.size(),
                expected_size
            );
            break;
        }
    }

    assert!(found, "{}: {} symbol not found", fixture_name, symbol_name);
}

#[test]
fn pa10_006s_u8_array_10_bytes() {
    // [u8; 10] should emit 10 bytes (1 byte per element)
    build_and_check_symbol_size("pa10_006s_u8_array.pdx", "gdt_ptr", 10);
}

#[test]
fn pa10_006s_u16_array_6_bytes() {
    // [u16; 3] should emit 6 bytes (2 bytes per element)
    build_and_check_symbol_size("pa10_006s_u16_array.pdx", "arr", 6);
}

#[test]
fn pa10_006s_u32_array_16_bytes() {
    // [u32; 4] should emit 16 bytes (4 bytes per element)
    build_and_check_symbol_size("pa10_006s_u32_array.pdx", "arr", 16);
}

#[test]
fn pa10_006s_u64_array_16_bytes() {
    // [u64; 2] should emit 16 bytes (8 bytes per element)
    build_and_check_symbol_size("pa10_006s_u64_array.pdx", "arr", 16);
}
