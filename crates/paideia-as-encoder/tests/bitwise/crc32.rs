//! Byte-exact and round-trip tests for CRC32 instruction encoder.
//!
//! Tests the F2 REX.W 0F 38 F1 /r encoding patterns for 64-bit CRC32.

use paideia_as_encoder::encode::{crc32_reg64_reg64, crc32_reg64_mem_base_disp, CodeBuffer, Reg64};

// ============================================================================
// Byte-Exact Tests (5 tests)
// ============================================================================

/// Test `crc32 rax, rax` (CRC32 reg64, reg64).
/// Expected: [0xF2, 0x48, 0x0F, 0x38, 0xF1, 0xC0]
/// - 0xF2: F2 prefix
/// - 0x48: REX.W (W=1, R=0, X=0, B=0)
/// - 0x0F: escape
/// - 0x38: escape
/// - 0xF1: opcode
/// - 0xC0: ModR/M (mod=11, reg=000, r/m=000 for rax,rax)
#[test]
fn crc32_rax_rax_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    crc32_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rax);

    assert_eq!(buf.bytes, vec![0xF2, 0x48, 0x0F, 0x38, 0xF1, 0xC0]);
}

/// Test `crc32 rax, rbx` (CRC32 reg64, reg64).
/// Expected: [0xF2, 0x48, 0x0F, 0x38, 0xF1, 0xC3]
/// - 0xC3: ModR/M (mod=11, reg=000, r/m=011 for rbx)
#[test]
fn crc32_rax_rbx_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    crc32_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rbx);

    assert_eq!(buf.bytes, vec![0xF2, 0x48, 0x0F, 0x38, 0xF1, 0xC3]);
}

/// Test `crc32 rax, [rcx]` (CRC32 reg64, [base]).
/// Expected: [0xF2, 0x48, 0x0F, 0x38, 0xF1, 0x01]
/// - 0x01: ModR/M (mod=00, reg=000, r/m=001 for [rcx])
#[test]
fn crc32_rax_mem_rcx_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    crc32_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rcx, 0);

    assert_eq!(buf.bytes, vec![0xF2, 0x48, 0x0F, 0x38, 0xF1, 0x01]);
}

/// Test `crc32 rax, [rsp]` (CRC32 reg64, [base with SIB]).
/// Expected: [0xF2, 0x48, 0x0F, 0x38, 0xF1, 0x04, 0x24]
/// - 0x04: ModR/M (mod=00, reg=000, r/m=100 for SIB)
/// - 0x24: SIB (scale=00, index=100 (rsp), base=100 (rsp))
#[test]
fn crc32_rax_mem_rsp_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    crc32_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rsp, 0);

    assert_eq!(buf.bytes, vec![0xF2, 0x48, 0x0F, 0x38, 0xF1, 0x04, 0x24]);
}

/// Test `crc32 r8, r15` (CRC32 reg64, reg64 with high registers).
/// Expected: [0xF2, 0x4D, 0x0F, 0x38, 0xF1, 0xC7]
/// - 0x4D: REX.W (W=1, R=1, X=0, B=1) for R8 (reg=8) and R15 (r/m=15)
/// - 0xC7: ModR/M (mod=11, reg=000, r/m=111 for r15)
#[test]
fn crc32_r8_r15_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    crc32_reg64_reg64(&mut buf, Reg64::R8, Reg64::R15);

    assert_eq!(buf.bytes, vec![0xF2, 0x4D, 0x0F, 0x38, 0xF1, 0xC7]);
}

// ============================================================================
// iced-x86 Round-Trip Tests (2 tests)
// ============================================================================

/// Test iced-x86 round-trip for `crc32 rax, rcx`.
#[test]
fn crc32_rax_rcx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    crc32_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rcx);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Crc32);
}

/// Test iced-x86 round-trip for `crc32 rax, [rbx]`.
#[test]
fn crc32_rax_mem_rbx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    crc32_reg64_mem_base_disp(&mut buf, Reg64::Rax, Reg64::Rbx, 0);

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Crc32);
}
