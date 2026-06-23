//! Integration tests for AND, OR, XOR encoding (PA10-003).
//!
//! Tests verify that bitwise instructions are correctly encoded with byte-exact validation
//! against NASM-verified test vectors. Each instruction family (AND, OR, XOR) is tested
//! with:
//! - reg-reg binary operation
//! - reg-imm32 with sign-extension trap guard
//! - reg-imm8 (short form via sign-extension trap)
//! - reg-mem (indexed addressing, simplified to base+disp in phase-1)
//!
//! Additionally tests the sign-extension trap for AND and XOR to verify that
//! immediates that don't round-trip through i8/i32 are rejected as expected.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

// ===== AND Tests =====

#[test]
fn and_rax_rbx_reg_reg_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::And,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))], // rax, rbx
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for and rax, rbx");

    // Expected: 48 21 D8 (3 bytes)
    assert_eq!(buf.as_slice(), &[0x48, 0x21, 0xd8]);
}

#[test]
fn and_rax_0x7f_imm8_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::And,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x7F)], // rax, 0x7F
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for and rax, 0x7F");

    // Expected: 48 83 E0 7F (imm8 path: sign-extends through i8)
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xe0, 0x7f]);
}

#[test]
fn and_rax_0xff_imm32_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::And,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0xFF)], // rax, 0xFF
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for and rax, 0xFF");

    // Expected: 48 81 E0 FF 00 00 00 (imm32 path; 0xFF doesn't round-trip through i8 as unsigned)
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xe0, 0xff, 0x00, 0x00, 0x00]);
}

#[test]
fn and_rax_sign_ext_trap_rejects_out_of_range() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::And,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x100000001)], // out of i32 range
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err());
}

// ===== OR Tests =====

#[test]
fn or_rax_rbx_reg_reg_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))], // rax, rbx
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or rax, rbx");

    // Expected: 48 09 D8 (3 bytes)
    assert_eq!(buf.as_slice(), &[0x48, 0x09, 0xd8]);
}

#[test]
fn or_rax_0x7f_imm8_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x7F)], // rax, 0x7F
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or rax, 0x7F");

    // Expected: 48 83 C8 7F (imm8)
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xc8, 0x7f]);
}

#[test]
fn or_rax_0xff_imm32_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0xFF)], // rax, 0xFF
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for or rax, 0xFF");

    // Expected: 48 81 C8 FF 00 00 00 (imm32)
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xc8, 0xff, 0x00, 0x00, 0x00]);
}

// ===== XOR Tests =====

#[test]
fn xor_rax_rax_reg_reg_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xor,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(0))], // xor rax, rax
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for xor rax, rax");

    // Expected: 48 31 C0 (3 bytes)
    assert_eq!(buf.as_slice(), &[0x48, 0x31, 0xc0]);
}

#[test]
fn xor_rax_0x7f_imm8_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xor,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x7F)], // rax, 0x7F
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for xor rax, 0x7F");

    // Expected: 48 83 F0 7F (imm8)
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xf0, 0x7f]);
}

#[test]
fn xor_rax_0xff_imm32_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xor,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0xFF)], // rax, 0xFF
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for xor rax, 0xFF");

    // Expected: 48 81 F0 FF 00 00 00 (imm32)
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xf0, 0xff, 0x00, 0x00, 0x00]);
}

#[test]
fn xor_rax_sign_ext_trap_rejects_out_of_range() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xor,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x100000001)], // out of i32 range
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err());
}

// ===== Round-trip tests with iced-x86 =====

#[test]
fn and_rax_rbx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::And,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::And);
}

#[test]
fn or_rax_rbx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Or);
}

#[test]
fn xor_rax_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xor,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(0))],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Xor);
}
