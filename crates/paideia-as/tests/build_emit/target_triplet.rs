//! Tests for `--target <triplet>` shortcut feature (issue #1107).
//!
//! This module verifies the new `--target` flag provides a shortcut for common
//! output formats, and that `--target` and `--emit` conflict as expected.

use crate::common::fixture::{crate_tests, ScratchDir};
use crate::common::harness::{BuildOpts, EmitFmt, TargetTriplet, run_build_with};

/// Test 1: `--target uefi-x86_64` produces a valid PE32+ EFI application.
///
/// Verifies:
/// - MZ magic at offset 0x00
/// - PE signature at offset 0x40
/// - Machine type (0x8664 = x86-64) at offset 0x3C+4 = 0x40
/// - PE32+ magic (0x020B) at offset 0x58
/// - Subsystem (10 = EFI application) at offset 0x9C
#[test]
fn target_uefi_x86_64_produces_pe32plus_efi_application() {
    let scratch = ScratchDir::new("target_uefi");
    let out_path = scratch.artifact("out.o");
    let opts = BuildOpts::new(crate_tests("data/hello.pdx"))
        .emit(EmitFmt::None)
        .target(TargetTriplet::UefiX86_64)
        .output(&out_path);
    let out = run_build_with(opts);
    out.assert_ok();

    let bytes = out.artifact_bytes();

    // Check MZ magic at offset 0
    assert_eq!(&bytes[0..2], b"MZ", "PE magic 'MZ' missing");

    // Check PE signature at offset 0x40
    assert_eq!(&bytes[0x40..0x44], b"PE\0\0", "PE signature missing at 0x40");

    // Check machine type (0x8664 = x86-64) at offset 0x44 (PE signature + 4)
    let machine_bytes = [bytes[0x44], bytes[0x45]];
    assert_eq!(u16::from_le_bytes(machine_bytes), 0x8664, "machine type should be x86-64");

    // Check PE32+ magic (0x020B) at offset 0x58
    let pe_magic = [bytes[0x58], bytes[0x59]];
    assert_eq!(u16::from_le_bytes(pe_magic), 0x020B, "PE32+ magic (0x020B) missing");

    // Check subsystem (10 = EFI application) at offset 0x9C
    let subsystem = [bytes[0x9C], bytes[0x9D]];
    assert_eq!(u16::from_le_bytes(subsystem), 10, "subsystem should be 10 (EFI application)");
}

/// Test 2: `--target uefi-x86_64` produces byte-identical output to `--emit pe-coff`.
///
/// Builds the same fixture twice: once with `--target uefi-x86_64`, once with
/// `--emit pe-coff`, and verifies the output files are byte-identical.
#[test]
fn target_uefi_x86_64_matches_emit_pe_coff_bytes() {
    let scratch1 = ScratchDir::new("target_uefi_v1");
    let out1 = scratch1.artifact("out.o");
    let opts1 = BuildOpts::new(crate_tests("data/hello.pdx"))
        .emit(EmitFmt::None)
        .target(TargetTriplet::UefiX86_64)
        .output(&out1);
    let result1 = run_build_with(opts1);
    result1.assert_ok();
    let bytes1 = result1.artifact_bytes();

    let scratch2 = ScratchDir::new("target_uefi_v2");
    let out2 = scratch2.artifact("out.o");
    let opts2 = BuildOpts::new(crate_tests("data/hello.pdx"))
        .emit(crate::common::harness::EmitFmt::PeCoff)
        .output(&out2);
    let result2 = run_build_with(opts2);
    result2.assert_ok();
    let bytes2 = result2.artifact_bytes();

    assert_eq!(bytes1.len(), bytes2.len(), "output sizes differ");
    assert_eq!(bytes1, bytes2, "output bytes differ between --target and --emit");
}

/// Test 3: `--target elf-kernel-x86_64` produces a valid ELF64 object.
///
/// Verifies:
/// - ELF magic (0x7F 'E' 'L' 'F') at offset 0x00
/// - EI_CLASS byte (4) == 2 (64-bit)
#[test]
fn target_elf_kernel_x86_64_produces_valid_elf64() {
    let scratch = ScratchDir::new("target_elf_kernel");
    let out_path = scratch.artifact("out.o");
    let opts = BuildOpts::new(crate_tests("data/hello.pdx"))
        .emit(EmitFmt::None)
        .target(TargetTriplet::ElfKernelX86_64)
        .output(&out_path);
    let out = run_build_with(opts);
    out.assert_ok();

    let bytes = out.artifact_bytes();

    // Check ELF magic
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    // Check EI_CLASS byte (offset 4) == 2 (64-bit)
    assert_eq!(bytes[4], 2, "expected ELF64 (class 2)");
}

/// Test 4: `--target elf-user-x86_64` produces a valid ELF64 object.
///
/// This test produces the same output format as elf-kernel (both map to Elf64).
/// The distinction is conceptual; both are valid ELF64 objects.
#[test]
fn target_elf_user_x86_64_produces_valid_elf64() {
    let scratch = ScratchDir::new("target_elf_user");
    let out_path = scratch.artifact("out.o");
    let opts = BuildOpts::new(crate_tests("data/hello.pdx"))
        .emit(EmitFmt::None)
        .target(TargetTriplet::ElfUserX86_64)
        .output(&out_path);
    let out = run_build_with(opts);
    out.assert_ok();

    let bytes = out.artifact_bytes();

    // Check ELF magic
    assert_eq!(&bytes[0..4], b"\x7FELF", "ELF magic missing");

    // Check EI_CLASS byte (offset 4) == 2 (64-bit)
    assert_eq!(bytes[4], 2, "expected ELF64 (class 2)");

    // Document that output is byte-identical to elf-kernel (same EmitFormat::Elf64)
    // This is expected behavior; the distinction between kernel and user is semantic only
}

/// Test 5: `--target pax-x86_64` produces a valid PAX object.
///
/// The PAX format has its own magic bytes; this test verifies them.
#[test]
fn target_pax_x86_64_produces_valid_pax() {
    let scratch = ScratchDir::new("target_pax");
    let out_path = scratch.artifact("out.o");
    let opts = BuildOpts::new(crate_tests("data/hello.pdx"))
        .emit(EmitFmt::None)
        .target(TargetTriplet::PaxX86_64)
        .output(&out_path);
    let out = run_build_with(opts);
    out.assert_ok();

    let bytes = out.artifact_bytes();

    // PAX magic is "PAX\0" (4 bytes) at offset 0
    // (based on paideia-as-emitter-pax header format)
    assert!(bytes.len() >= 4, "PAX object too short");
    assert_eq!(&bytes[0..3], b"PAX", "PAX magic missing");
}

/// Test 6: `--target` and `--emit` conflict and error.
///
/// Invokes with both `--target uefi-x86_64` and `--emit elf64`; expects:
/// - Exit code != 0
/// - stderr contains clap's "cannot be used with" phrasing
#[test]
fn target_conflicts_with_emit_errors() {
    let scratch = ScratchDir::new("target_emit_conflict");
    let out_path = scratch.artifact("out.o");
    let opts = BuildOpts::new(crate_tests("data/hello.pdx"))
        .target(TargetTriplet::UefiX86_64)
        .emit(EmitFmt::Elf64)
        .output(&out_path);
    let out = run_build_with(opts);

    // Must fail with clap's usage-error exit code (2), NOT a runtime panic (101).
    // Debugger #1107 pass: guard against the case where clap's conflicts_with is
    // absent — the `unreachable!("clap conflicts_with...")` fallback would fire
    // and stderr would still coincidentally contain "conflict" (from the panic
    // message text), producing a passing test on genuinely broken clap wiring.
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected clap usage-error exit code 2 (not runtime panic 101); got: {:?}\nstderr: {}",
        out.status.code(),
        out.stderr
    );

    // Must contain clap's exact "cannot be used with" phrasing — NOT a substring
    // of the unreachable!() panic ("conflicts_with").
    assert!(
        out.stderr.contains("cannot be used with"),
        "stderr should contain clap's 'cannot be used with' phrasing: {}",
        out.stderr
    );
}

/// Test 7: Invalid target value errors.
///
/// Invokes with `--target garbage`; expects:
/// - Exit code != 0
/// - stderr contains clap's "invalid value" phrasing
#[test]
fn target_invalid_value_errors() {
    let scratch = ScratchDir::new("target_invalid");
    let out_path = scratch.artifact("out.o");
    let opts = BuildOpts::new(crate_tests("data/hello.pdx"))
        .emit(EmitFmt::None)
        .target(TargetTriplet::Raw("garbage"))
        .output(&out_path);
    let out = run_build_with(opts);

    // Must fail
    assert!(!out.status.success(), "build should fail with invalid target");

    // Must contain clap error about invalid value
    assert!(
        out.stderr.contains("invalid value") || out.stderr.contains("possible values"),
        "stderr should mention invalid value: {}",
        out.stderr
    );
}

/// Test 8: Omitting both `--target` and `--emit` defaults to placeholder format.
///
/// Backward compatibility: invoking without either flag should produce a
/// `<stem>.placeholder` file, not fail.
#[test]
fn no_target_no_emit_defaults_to_placeholder() {
    let scratch = ScratchDir::new("target_no_flags");
    let out_path = scratch.artifact("hello.placeholder");

    // Create options with no target and no emit (defaults)
    let opts = BuildOpts::new(crate_tests("data/hello.pdx"))
        .emit(EmitFmt::None)
        .target(TargetTriplet::None)
        .output(&out_path);
    let out = run_build_with(opts);

    // Must succeed
    out.assert_ok();

    // The artifact should exist
    let bytes = out.artifact_bytes();
    assert!(!bytes.is_empty(), ".placeholder file should exist and have content");
}
