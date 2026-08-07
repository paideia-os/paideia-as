//! PA-r17-004: Multi-parameter identity lambda byte-sequence tests.
//!
//! This test suite verifies that emit_identity_lambda correctly resolves
//! parameter references in curried functions. Before the fix, all identity
//! lambdas hardcoded RDI (RegId 7), causing multi-parameter functions to
//! return the wrong parameter.
//!
//! Each test:
//! 1. Writes a real .pdx fixture with a curried lambda
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

/// paideia-as#1276 phase 3: default SysV frame prologue.
/// `55 48 89 E5` = `push rbp; mov rbp, rsp`.
const FRAME_PROLOGUE: &[u8] = &[0x55, 0x48, 0x89, 0xE5];

/// paideia-as#1276 phase 3: default SysV frame epilogue (excludes the terminal
/// `ret` which callers still supply as the last body byte).
/// `48 89 EC 5D` = `mov rsp, rbp; pop rbp`.
const FRAME_EPILOGUE: &[u8] = &[0x48, 0x89, 0xEC, 0x5D];

/// Wrap a body byte-sequence with the default SysV prologue/epilogue.
///
/// Fixtures in this test suite are not annotated `@no_frame`, so the
/// elaborator emits `push rbp; mov rbp, rsp` at entry and `mov rsp, rbp;
/// pop rbp` before the terminal `ret`. The input is expected to end with
/// the `ret` byte (`0xC3`); the epilogue is spliced in front of it.
fn wrap_frame(body_with_ret: &[u8]) -> Vec<u8> {
    let (body, ret) = body_with_ret.split_at(body_with_ret.len() - 1);
    assert_eq!(ret, [0xC3], "wrap_frame expects sequence to terminate in `ret`");
    let mut out = Vec::with_capacity(FRAME_PROLOGUE.len() + body.len() + FRAME_EPILOGUE.len() + 1);
    out.extend_from_slice(FRAME_PROLOGUE);
    out.extend_from_slice(body);
    out.extend_from_slice(FRAME_EPILOGUE);
    out.extend_from_slice(ret);
    out
}

/// Test helper: build a fixture, extract symbol bytes, and assert.
///
/// `body_bytes_with_ret` is the pre-#1276 raw body bytes (ending in `ret`);
/// this helper wraps them with the phase-3 frame prologue/epilogue before
/// comparing.
fn test_identity_lambda(fixture_name: &str, symbol_name: &str, body_bytes_with_ret: &[u8]) {
    let expected_bytes: Vec<u8> = wrap_frame(body_bytes_with_ret);
    let expected_bytes: &[u8] = &expected_bytes;
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

/// Test identity lambda: `fn(x: u64) -> x`
/// Expected: RDI → RAX, bytes: `48 89 F8 C3`
#[test]
fn identity_single_param() {
    test_identity_lambda(
        "identity_single_param.pdx",
        "identity_single_param",
        &[0x48, 0x89, 0xF8, 0xC3],
    );
}

/// Test curried identity: `fn(a)(b) -> a`
/// Expected: RDI (first param) → RAX, bytes: `48 89 F8 C3`
#[test]
fn identity_first_of_two_curried() {
    test_identity_lambda(
        "identity_first_of_two_curried.pdx",
        "identity_first_of_two_curried",
        &[0x48, 0x89, 0xF8, 0xC3],
    );
}

/// Test curried identity: `fn(a)(b) -> b`
/// Expected: RSI (second param) → RAX, bytes: `48 89 F0 C3`
#[test]
fn identity_second_of_two_curried() {
    test_identity_lambda(
        "identity_second_of_two_curried.pdx",
        "identity_second_of_two_curried",
        &[0x48, 0x89, 0xF0, 0xC3],
    );
}

/// Test curried identity: `fn(a)(b)(c) -> a`
/// Expected: RDI (first param) → RAX, bytes: `48 89 F8 C3`
#[test]
fn identity_first_of_three_curried() {
    test_identity_lambda(
        "identity_first_of_three_curried.pdx",
        "identity_first_of_three_curried",
        &[0x48, 0x89, 0xF8, 0xC3],
    );
}

/// Test curried identity: `fn(a)(b)(c) -> b`
/// Expected: RSI (second param) → RAX, bytes: `48 89 F0 C3`
#[test]
fn identity_second_of_three_curried() {
    test_identity_lambda(
        "identity_second_of_three_curried.pdx",
        "identity_second_of_three_curried",
        &[0x48, 0x89, 0xF0, 0xC3],
    );
}

/// Test curried identity: `fn(a)(b)(c) -> c`
/// Expected: RDX (third param) → RAX, bytes: `48 89 D0 C3`
#[test]
fn identity_third_of_three_curried() {
    test_identity_lambda(
        "identity_third_of_three_curried.pdx",
        "identity_third_of_three_curried",
        &[0x48, 0x89, 0xD0, 0xC3],
    );
}
