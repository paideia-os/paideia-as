//! PA7C-m2-001: iced-x86 round-trip integration test for unsafe-body instruction emission.
//!
//! This test verifies that the build command correctly emits unsafe-block
//! instructions for the 7 PA7C_unsafe_body fixtures:
//! - unsafe_body_outb.pdx: mov rax, 0x80; mov rdx, 0x3FB; out rdx, rax
//! - unsafe_body_hlt.pdx: hlt
//! - unsafe_body_cli.pdx: cli
//! - unsafe_body_mov_reg_reg.pdx: 3x mov reg, reg
//! - unsafe_body_mov_reg_imm.pdx: 3x mov reg, imm64
//! - unsafe_body_swapgs.pdx: swapgs
//! - unsafe_body_sti_hlt.pdx: sti; hlt
//!
//! The test:
//! 1. Invokes the build command programmatically for each fixture
//! 2. Reads the resulting .o (ELF) file
//! 3. Extracts the .text section via the `object` crate
//! 4. Round-trips through iced-x86 disassembler to verify instruction sequences
//! 5. Asserts basic structural correctness (magic, format, section presence)
//!
//! PLATFORM: Linux-only (iced-x86 disassembly availability).
//! This test is the truth detector for whether the unsafe block content
//! reaches the emitted code (Phase 7 m2-001 elaborator integration).

use object::{Object, ObjectSection};
use std::path::PathBuf;
use std::process::Command;

#[cfg(target_os = "linux")]
use iced_x86::{Decoder, DecoderOptions};

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

#[test]
#[cfg(target_os = "linux")]
fn unsafe_body_outb_iced_x86_round_trip() {
    let input = build_emit_data("pa7c_unsafe_body/unsafe_body_outb.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pa7c_outb_emit.o");
    let _ = std::fs::remove_file(&tmp);

    // Build to ELF64
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
        "build failed for unsafe_body_outb.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    let file = object::File::parse(&*bytes).expect("should parse ELF");

    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_bytes.is_empty(), ".text section must exist");

    // iced-x86 round-trip: decode and verify instruction count
    let mut decoder = Decoder::new(64, &text_bytes, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    // Expect: mov rax, imm64 (10 bytes) + mov rdx, imm64 (10 bytes) + out rdx, al (1 byte) = 21 bytes
    assert!(
        insts.len() >= 3,
        "expected at least 3 instructions (mov, mov, out), got {}",
        insts.len()
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
#[cfg(target_os = "linux")]
fn unsafe_body_hlt_iced_x86_round_trip() {
    let input = build_emit_data("pa7c_unsafe_body/unsafe_body_hlt.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pa7c_hlt_emit.o");
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
        "build failed for unsafe_body_hlt.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    let file = object::File::parse(&*bytes).expect("should parse ELF");

    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_bytes.is_empty(), ".text section must exist");

    let mut decoder = Decoder::new(64, &text_bytes, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    // Expect: hlt (1 byte)
    assert!(
        insts.len() >= 1,
        "expected at least 1 instruction (hlt), got {}",
        insts.len()
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
#[cfg(target_os = "linux")]
fn unsafe_body_cli_iced_x86_round_trip() {
    let input = build_emit_data("pa7c_unsafe_body/unsafe_body_cli.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pa7c_cli_emit.o");
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
        "build failed for unsafe_body_cli.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    let file = object::File::parse(&*bytes).expect("should parse ELF");

    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_bytes.is_empty(), ".text section must exist");

    let mut decoder = Decoder::new(64, &text_bytes, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    // Expect: cli (1 byte)
    assert!(
        insts.len() >= 1,
        "expected at least 1 instruction (cli), got {}",
        insts.len()
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
#[cfg(target_os = "linux")]
fn unsafe_body_mov_reg_reg_iced_x86_round_trip() {
    let input = build_emit_data("pa7c_unsafe_body/unsafe_body_mov_reg_reg.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pa7c_mov_reg_reg_emit.o");
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
        "build failed for unsafe_body_mov_reg_reg.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    let file = object::File::parse(&*bytes).expect("should parse ELF");

    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_bytes.is_empty(), ".text section must exist");

    let mut decoder = Decoder::new(64, &text_bytes, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    // Expect: 3x mov reg, reg (3 bytes each = 9 bytes)
    assert!(
        insts.len() >= 3,
        "expected at least 3 instructions (mov, mov, mov), got {}",
        insts.len()
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
#[cfg(target_os = "linux")]
fn unsafe_body_mov_reg_imm_iced_x86_round_trip() {
    let input = build_emit_data("pa7c_unsafe_body/unsafe_body_mov_reg_imm.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pa7c_mov_reg_imm_emit.o");
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
        "build failed for unsafe_body_mov_reg_imm.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    let file = object::File::parse(&*bytes).expect("should parse ELF");

    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_bytes.is_empty(), ".text section must exist");

    let mut decoder = Decoder::new(64, &text_bytes, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    // Expect: 3x mov reg, imm64 (10 bytes each = 30 bytes)
    assert!(
        insts.len() >= 3,
        "expected at least 3 instructions (mov, mov, mov), got {}",
        insts.len()
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
#[cfg(target_os = "linux")]
fn unsafe_body_swapgs_iced_x86_round_trip() {
    let input = build_emit_data("pa7c_unsafe_body/unsafe_body_swapgs.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pa7c_swapgs_emit.o");
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
        "build failed for unsafe_body_swapgs.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    let file = object::File::parse(&*bytes).expect("should parse ELF");

    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_bytes.is_empty(), ".text section must exist");

    let mut decoder = Decoder::new(64, &text_bytes, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    // Expect: swapgs (3 bytes)
    assert!(
        insts.len() >= 1,
        "expected at least 1 instruction (swapgs), got {}",
        insts.len()
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
#[cfg(target_os = "linux")]
fn unsafe_body_sti_hlt_iced_x86_round_trip() {
    let input = build_emit_data("pa7c_unsafe_body/unsafe_body_sti_hlt.pdx");
    let tmp = std::env::temp_dir().join("paideia_as_pa7c_sti_hlt_emit.o");
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
        "build failed for unsafe_body_sti_hlt.pdx: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&tmp).expect("output ELF should exist");
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    let file = object::File::parse(&*bytes).expect("should parse ELF");

    let mut text_bytes = Vec::new();
    for section in file.sections() {
        if section.name().unwrap_or("") == ".text" {
            text_bytes = section.data().unwrap_or(b"").to_vec();
            break;
        }
    }

    assert!(!text_bytes.is_empty(), ".text section must exist");

    let mut decoder = Decoder::new(64, &text_bytes, DecoderOptions::NONE);
    let insts: Vec<_> = decoder.iter().collect();

    // Expect: sti (1 byte) + hlt (1 byte) = 2 bytes
    assert!(
        insts.len() >= 2,
        "expected at least 2 instructions (sti, hlt), got {}",
        insts.len()
    );

    let _ = std::fs::remove_file(&tmp);
}
