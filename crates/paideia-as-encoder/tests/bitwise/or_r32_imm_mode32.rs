//! Integration tests for OR r32, imm encoding in Mode32 (Phase 15 m3-004).
//!
//! These tests verify that OR instructions with 32-bit register operands
//! and immediate operands are correctly encoded in Mode32 through the
//! encode pipeline, with byte-exact validation against expected test vectors.
//!
//! The encoder implements mode-aware dispatch:
//! - OR r32, imm8: 83 /1 ib (no REX.W prefix)
//! - OR r32, imm32: 81 /1 id (no REX.W prefix)
//! - Immediate must fit in the target range (8-bit or 32-bit).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::instruction::{InstrMode, Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

#[test]
fn or_eax_0x03_imm8_mode32_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x03)], // eax, 0x03
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or eax, 0x03");

    // Expected: 83 C8 03 (3 bytes, no REX.W)
    assert_eq!(buf.as_slice(), &[0x83, 0xc8, 0x03]);
}

#[test]
fn or_eax_0x100_imm32_mode32_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x100)], // eax, 0x100
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or eax, 0x100");

    // Expected: 81 C8 00 01 00 00 (6 bytes, no REX.W)
    assert_eq!(buf.as_slice(), &[0x81, 0xc8, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn or_r8d_0x20_imm8_mode32_encodes_with_rex_b() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(8)), Operand::Imm64(0x20)], // r8d, 0x20
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or r8d, 0x20");

    // Expected: 41 83 C8 20 (4 bytes, REX.B but no REX.W)
    assert_eq!(buf.as_slice(), &[0x41, 0x83, 0xc8, 0x20]);
}

#[test]
fn or_eax_0x7fffffff_imm32_mode32_encodes_correctly() {
    //! Test with i32::MAX (0x7fffffff) which round-trips through i32 correctly.
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x7fffffff)], // eax, 0x7fffffff
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or eax, 0x7fffffff");

    // Expected: 81 C8 FF FF FF 7F (6 bytes, maximum signed 32-bit value)
    assert_eq!(buf.as_slice(), &[0x81, 0xc8, 0xff, 0xff, 0xff, 0x7f]);
}

#[test]
fn or_rax_0x03_mode64_regression_guard_still_emits_rex_w() {
    //! Regression guard: Mode64 must still emit REX.W for or rax, imm8.
    //! This ensures the Mode32 change doesn't accidentally affect Mode64 behavior.
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x03)], // rax, 0x03
        byte_offset_in_text: None,
        mode: InstrMode::Mode64, // explicitly Mode64
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or rax, 0x03");

    // Expected: 48 83 C8 03 (4 bytes, with REX.W=0x48)
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xc8, 0x03]);
}

#[test]
fn or_eax_0x03_mode32_iced_x86_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x03)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Verify round-trip through iced-x86 decoder (32-bit mode)
    let mut decoder = Decoder::new(32, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Or);
}
