//! PA-R12-003: Top-bit-set imm64 encoding (issue #912).
//!
//! This test verifies that 64-bit integer literals with the top bit set
//! (values > 0x7FFF_FFFF_FFFF_FFFF) are correctly encoded in mov instructions.
//! Previously, hex literals like 0xFFFFFFFFFFFFFFFD silently encoded as 0.
//!
//! The fix: both unsafe_walker::extract_integer_from_span and
//! cmd_build::parse_integer_literal now use u64::from_str_radix (instead of i64)
//! with bit-preserving `as i64` cast for hex/oct/bin bases.
//!
//! Expected bytes for `mov rax, 0xFFFFFFFFFFFFFFFD`:
//! 48 b8 fd ff ff ff ff ff ff ff
//! (REX.W + MOV r64, imm64 with little-endian imm)
//!
//! Test plan:
//! 1. Build each fixture with `cargo run -- build <fixture.pdx> --emit elf64 -o <tmp.o>`
//! 2. Parse the ELF file via the `object` crate
//! 3. Extract .text section bytes
//! 4. Assert first 10 bytes match expected encoding

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

fn build_and_extract_text(fixture_name: &str) -> Vec<u8> {
    let input = build_emit_data(&format!("{}.pdx", fixture_name));
    let tmp = std::env::temp_dir().join(format!("paideia_as_{}.o", fixture_name));
    let _ = std::fs::remove_file(&tmp);

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
        "build failed for {}.pdx: {}",
        fixture_name,
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert!(bytes.len() >= 64, "ELF header is 64 bytes minimum");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    let file = object::File::parse(&*bytes).expect("object should parse the ELF");

    let mut text_bytes = Vec::new();
    let mut found_text = false;
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            found_text = true;
            let data = section.data().expect(".text data should exist");
            text_bytes.extend_from_slice(data);
            break;
        }
    }

    assert!(found_text, ".text section should exist in {}.pdx", fixture_name);
    assert!(!text_bytes.is_empty(), ".text section should not be empty");

    text_bytes
}

#[test]
fn imm64_top_bit_fd() {
    // mov rax, 0xFFFFFFFFFFFFFFFD
    // Expected: 48 b8 fd ff ff ff ff ff ff ff
    let text_bytes = build_and_extract_text("imm64_top_bit_fd");
    let expected = vec![0x48, 0xb8, 0xfd, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
    assert_eq!(
        &text_bytes[..10], &expected[..],
        "imm64_top_bit_fd: expected {:02X?}, got {:02X?}",
        expected, &text_bytes[..10]
    );
}

#[test]
fn imm64_top_bit_fc() {
    // mov rax, 0xFFFFFFFFFFFFFFFC
    // Expected: 48 b8 fc ff ff ff ff ff ff ff
    let text_bytes = build_and_extract_text("imm64_top_bit_fc");
    let expected = vec![0x48, 0xb8, 0xfc, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
    assert_eq!(
        &text_bytes[..10], &expected[..],
        "imm64_top_bit_fc: expected {:02X?}, got {:02X?}",
        expected, &text_bytes[..10]
    );
}

#[test]
fn imm64_deadbeef() {
    // mov rax, 0xDEADBEEF00000007
    // Expected: 48 b8 07 00 00 00 ef be ad de (little-endian)
    let text_bytes = build_and_extract_text("imm64_deadbeef");
    let expected = vec![0x48, 0xb8, 0x07, 0x00, 0x00, 0x00, 0xef, 0xbe, 0xad, 0xde];
    assert_eq!(
        &text_bytes[..10], &expected[..],
        "imm64_deadbeef: expected {:02X?}, got {:02X?}",
        expected, &text_bytes[..10]
    );
}

#[test]
fn imm64_all_ones() {
    // mov rax, 0xFFFFFFFFFFFFFFFF
    // Expected: 48 b8 ff ff ff ff ff ff ff ff
    let text_bytes = build_and_extract_text("imm64_all_ones");
    let expected = vec![0x48, 0xb8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
    assert_eq!(
        &text_bytes[..10], &expected[..],
        "imm64_all_ones: expected {:02X?}, got {:02X?}",
        expected, &text_bytes[..10]
    );
}

#[test]
fn imm64_i64_max() {
    // mov rax, 0x7FFFFFFFFFFFFFFF (i64::MAX, regression test)
    // Expected: 48 b8 ff ff ff ff ff ff ff 7f
    let text_bytes = build_and_extract_text("imm64_i64_max");
    let expected = vec![0x48, 0xb8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f];
    assert_eq!(
        &text_bytes[..10], &expected[..],
        "imm64_i64_max: expected {:02X?}, got {:02X?}",
        expected, &text_bytes[..10]
    );
}
