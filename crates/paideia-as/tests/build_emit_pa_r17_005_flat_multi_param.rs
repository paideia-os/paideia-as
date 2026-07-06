//! PA-r17-005: Multi-parameter flat-syntax lambda byte-sequence tests.
//!
//! This test suite verifies that flat comma-separated parameters in a single
//! group work correctly. Issue #1041: Extending the parser to accept
//! `fn(a: u64, b: u64, c: u64)` in addition to curried `fn(a)(b)(c)`.
//!
//! Each test:
//! 1. Writes a real .pdx fixture with a flat-param lambda
//! 2. Invokes `cargo run -- build --emit elf64`
//! 3. Parses the emitted .o with the `object` crate
//! 4. Extracts the symbol's .text bytes
//! 5. Asserts EXACTLY on the byte sequence (byte-for-byte)
//!
//! Register→bytes mapping (verified x86-64 calling convention):
//! - RDI (RegId 7): `48 89 F8 C3` (mov rax, rdi; ret)
//! - RSI (RegId 6): `48 89 F0 C3` (mov rax, rsi; ret)
//! - RDX (RegId 2): `48 89 D0 C3` (mov rax, rdx; ret)
//! - RCX (RegId 1): `48 89 C8 C3` (mov rax, rcx; ret)
//! - R8  (RegId 8): `4C 89 C0 C3` (mov rax, r8; ret)
//! - R9  (RegId 9): `4C 89 C8 C3` (mov rax, r9; ret)

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

/// Extract function bytes from ELF by symbol name.
fn extract_symbol_bytes(file: &object::File<'_>, symbol_name: &str) -> Option<Vec<u8>> {
    let mut text_bytes = Vec::new();
    let mut found_text = false;

    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            found_text = true;
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    if !found_text {
        return None;
    }

    for sym in file.symbols() {
        if let Ok(name) = sym.name() {
            if name == symbol_name {
                let sym_addr = sym.address() as usize;
                let sym_size = sym.size() as usize;
                if sym_size > 0 && sym_addr + sym_size <= text_bytes.len() {
                    return Some(text_bytes[sym_addr..sym_addr + sym_size].to_vec());
                }
            }
        }
    }

    None
}

/// Test helper: build a fixture, extract symbol bytes, and assert.
fn test_flat_lambda(fixture_name: &str, symbol_name: &str, expected_bytes: &[u8]) {
    let input = build_emit_data(fixture_name);
    let tmp = std::env::temp_dir().join(format!("paideia_as_{}_emit.o", fixture_name));
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
        "build --emit elf64 failed for {}: {}",
        fixture_name,
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

    // Extract symbol bytes
    let actual_bytes = extract_symbol_bytes(&file, symbol_name)
        .unwrap_or_else(|| panic!("Symbol {} not found in {}", symbol_name, fixture_name));

    // Assert byte-for-byte match
    if actual_bytes != expected_bytes {
        eprintln!(
            "MISMATCH in {} (symbol {}): expected {:02X?}, got {:02X?}",
            fixture_name, symbol_name, expected_bytes, actual_bytes
        );
        panic!(
            "Symbol {} bytes do not match (expected {} bytes, got {})",
            symbol_name,
            expected_bytes.len(),
            actual_bytes.len()
        );
    }

    let _ = std::fs::remove_file(&tmp);
}

/// Test flat lambda: `fn(a: u64, b: u64) -> a`
/// Expected: RDI → RAX, bytes: `48 89 F8 C3`
#[test]
fn flat_two_returns_first() {
    test_flat_lambda(
        "flat_two_returns_first.pdx",
        "flat_two_returns_first",
        &[0x48, 0x89, 0xF8, 0xC3],
    );
}

/// Test flat lambda: `fn(a: u64, b: u64) -> b`
/// Expected: RSI → RAX, bytes: `48 89 F0 C3`
#[test]
fn flat_two_returns_second() {
    test_flat_lambda(
        "flat_two_returns_second.pdx",
        "flat_two_returns_second",
        &[0x48, 0x89, 0xF0, 0xC3],
    );
}

/// Test flat lambda: `fn(a: u64, b: u64, c: u64) -> b`
/// Expected: RSI → RAX, bytes: `48 89 F0 C3`
#[test]
fn flat_three_returns_second() {
    test_flat_lambda(
        "flat_three_returns_second.pdx",
        "flat_three_returns_second",
        &[0x48, 0x89, 0xF0, 0xC3],
    );
}

/// Test flat lambda: `fn(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64) -> c`
/// Expected: RDX → RAX, bytes: `48 89 D0 C3`
#[test]
fn flat_six_returns_third() {
    test_flat_lambda(
        "flat_six_returns_third.pdx",
        "flat_six_returns_third",
        &[0x48, 0x89, 0xD0, 0xC3],
    );
}

/// Test flat lambda with trailing comma: `fn(a: u64, b: u64,) -> b`
/// Expected: RSI → RAX, bytes: `48 89 F0 C3`
#[test]
fn flat_trailing_comma() {
    test_flat_lambda(
        "flat_trailing_comma.pdx",
        "flat_trailing_comma",
        &[0x48, 0x89, 0xF0, 0xC3],
    );
}

/// Test zero-param lambda: `fn() -> 42`
/// Expected: build succeeds and symbol exists (may have size 0 if constant-folded)
#[test]
fn zero_param() {
    let input = build_emit_data("zero_param.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_zero_param_emit.o");
    let _ = std::fs::remove_file(&tmp);

    // Build the fixture
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

    // Read and parse the ELF
    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    let file = object::File::parse(&*bytes).expect("object should parse");

    // Verify the symbol exists (size may be 0 if constant-folded by elaborator)
    let found = file
        .symbols()
        .any(|sym| sym.name().map_or(false, |n| n == "zero_param"));

    assert!(found, "Symbol zero_param should exist in ELF");

    let _ = std::fs::remove_file(&tmp);
}

/// Test curried regression: existing curried syntax still works
/// Reference: `fn(a)(b) -> b` should produce RSI → RAX
#[test]
fn curried_regression() {
    test_flat_lambda(
        "identity_second_of_two_curried.pdx",
        "identity_second_of_two_curried",
        &[0x48, 0x89, 0xF0, 0xC3],
    );
}

/// Test seven-param rejection: build should fail with P0276
#[test]
fn flat_seven_rejects() {
    let input = build_emit_data("flat_seven_rejects.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_flat_seven_rejects_emit.o");
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

    // Build must fail
    assert_ne!(
        out.status.code(),
        Some(0),
        "seven-param lambda must fail to build"
    );

    // Stderr must contain P0276
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("P0276"),
        "expected P0276 in stderr, got: {}",
        stderr
    );

    let _ = std::fs::remove_file(&tmp);
}
