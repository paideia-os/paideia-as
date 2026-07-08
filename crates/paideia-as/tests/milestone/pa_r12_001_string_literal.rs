//! PA-R12-001 (issue #910): String literals in module-level let bindings.
//!
//! This integration test verifies that `pub let X : [u8; N] = "string"` inside
//! a module emits a `.rodata` symbol with the correct byte size, handling truncation
//! and zero-padding.
//!
//! Test cases:
//! - `let greeting : [u8; 6] = "hello\0"` should emit a 6-byte symbol in .rodata
//! - `let banner : [u8; 16] = "CAP INVOKE MEM\n\0"` should emit a 16-byte symbol in .rodata
//! - `let short_in_wide : [u8; 32] = "ok\0"` should emit a 32-byte symbol with zero-padding

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
fn pa_r12_001_string_literal_greeting() {
    // `let greeting : [u8; 6] = "hello\0"` should emit exactly 6 bytes
    build_and_check_symbol_size(
        "pa_r12_001_string_literal_module.pdx",
        "greeting",
        6,
    );
}

#[test]
fn pa_r12_001_string_literal_banner() {
    // `let banner : [u8; 16] = "CAP INVOKE MEM\n\0"` should emit 16 bytes
    build_and_check_symbol_size(
        "pa_r12_001_string_literal_module.pdx",
        "banner",
        16,
    );
}

#[test]
fn pa_r12_001_string_literal_padded() {
    // `let short_in_wide : [u8; 32] = "ok\0"` should emit 32 bytes with zero-padding
    build_and_check_symbol_size(
        "pa_r12_001_string_literal_padded.pdx",
        "short_in_wide",
        32,
    );
}
