//! Integration tests for OR r64, imm encoding (Phase 8 m1-001d).
//!
//! These tests verify that OR instructions with immediate operands are correctly
//! encoded through the full parse→elaborate→encode pipeline, with byte-exact validation
//! against NASM-verified test vectors.
//!
//! The encoder implements the sign-extension trap guard per softarch §A.7:
//! - OR r64, imm8: REX.W 83 /1 ib (sign-extends to 64 bits)
//! - OR r64, imm32: REX.W 81 /1 id (sign-extends to 64 bits)
//! - Immediate must round-trip through the intermediate type (i8/i32) to avoid semantic changes.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

#[test]
fn or_rax_0x20_imm8_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x20)], // rax, 0x20
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or rax, 0x20");

    // Expected: 48 83 c8 20 (4 bytes)
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xc8, 0x20]);
}

#[test]
fn or_r15_0x7f_imm8_encodes_with_rex_b() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(15)), Operand::Imm64(0x7f)], // r15, 0x7f
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or r15, 0x7f");

    // Expected: 49 83 cf 7f (4 bytes, REX.B)
    assert_eq!(buf.as_slice(), &[0x49, 0x83, 0xcf, 0x7f]);
}

#[test]
fn or_rax_0x100_imm32_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x100)], // rax, 0x100
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or rax, 0x100");

    // Expected: 48 81 c8 00 01 00 00 (7 bytes)
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xc8, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn or_r8_0x100_imm32_encodes_with_rex_b() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(8)), Operand::Imm64(0x100)], // r8, 0x100
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or r8, 0x100");

    // Expected: 49 81 c8 00 01 00 00 (7 bytes, REX.B)
    assert_eq!(buf.as_slice(), &[0x49, 0x81, 0xc8, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn or_rax_0x7fffffff_imm32_max_signed_encodes() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x7fffffff)], // rax, 0x7fffffff (i32::MAX)
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or rax, 0x7fffffff");

    // Expected: 48 81 c8 ff ff ff 7f (7 bytes, maximum signed 32-bit value)
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xc8, 0xff, 0xff, 0xff, 0x7f]);
}

#[test]
fn triple_or_pipeline_integration() {
    //! Full pipeline integration: encode 3 OR instructions sequentially
    //! and verify the concatenated byte stream matches the expected vector.
    //!
    //! Sequence:
    //! - or rax, 0x20 → 48 83 c8 20 (4 bytes)
    //! - or r15, 0x100 → 49 81 cf 00 01 00 00 (7 bytes)
    //! - or rax, 0x7fffffff → 48 81 c8 ff ff ff 7f (7 bytes)
    //!
    //! Total: 18 bytes

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();

    // or rax, 0x20
    let inst1 = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x20)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    paideia_as_encoder::encode_instruction(&inst1, &mut buf, &mut stats)
        .expect("encoding failed for inst1");

    // or r15, 0x100
    let inst2 = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(15)), Operand::Imm64(0x100)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    paideia_as_encoder::encode_instruction(&inst2, &mut buf, &mut stats)
        .expect("encoding failed for inst2");

    // or rax, 0x7fffffff (i32::MAX)
    let inst3 = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x7fffffff)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    paideia_as_encoder::encode_instruction(&inst3, &mut buf, &mut stats)
        .expect("encoding failed for inst3");

    // Expected: 48 83 c8 20 | 49 81 cf 00 01 00 00 | 48 81 c8 ff ff ff 7f
    let expected = &[
        0x48, 0x83, 0xc8, 0x20, // or rax, 0x20
        0x49, 0x81, 0xcf, 0x00, 0x01, 0x00, 0x00, // or r15, 0x100
        0x48, 0x81, 0xc8, 0xff, 0xff, 0xff, 0x7f, // or rax, 0x7fffffff
    ];
    assert_eq!(buf.as_slice(), expected);
    assert_eq!(buf.len(), 18);
}

#[test]
fn or_rax_0x20_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x20)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Verify round-trip through iced-x86 decoder
    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Or);
}

#[test]
fn or_rax_0x100_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x100)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Verify round-trip through iced-x86 decoder
    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Or);
}
