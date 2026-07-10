//! Integration tests for PUSH immediate encoding (Phase R15 m4-006).
//!
//! Tests verify that PUSH with immediate operands encode to the exact byte sequences:
//! - PUSH imm8: 6A ib (2 bytes) — the immediate is sign-extended by the CPU
//! - PUSH imm32: 68 id (5 bytes) — the immediate is sign-extended by the CPU
//!
//! Test coverage:
//! 1. Byte-exact encoding: PUSH 0x12 → [0x6A, 0x12]
//! 2. Byte-exact encoding: PUSH -1 → [0x6A, 0xFF]
//! 3. Boundary tests for i8 range (0x7F, 0x80, -0x80, -0x81)
//! 4. Byte-exact encoding for imm32 range (0x12345678, -1i32, -0x81)
//! 5. iced-x86 round-trip: decode should yield PUSH with correct immediate value

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use smallvec::smallvec;

/// Helper to encode an instruction and return the bytes.
fn encode_instruction_bytes(inst: &Instruction) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(inst, &mut buf, &mut stats).expect("encoding failed");
    buf.as_slice().to_vec()
}

#[test]
fn push_imm8_0x12_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(0x12)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    assert_eq!(
        bytes,
        &[0x6A, 0x12],
        "PUSH 0x12 should encode as 6A 12"
    );
}

#[test]
fn push_imm8_minus_1_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(-1)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    assert_eq!(
        bytes,
        &[0x6A, 0xFF],
        "PUSH -1 should encode as 6A FF (imm8 form, sign-extended)"
    );
}

#[test]
fn push_imm8_0x7f_boundary_fits_i8() {
    // 0x7F = 127, the maximum positive value for i8
    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(0x7F)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    assert_eq!(
        bytes,
        &[0x6A, 0x7F],
        "PUSH 0x7F should use imm8 form (6A 7F)"
    );
}

#[test]
fn push_imm32_0x80_boundary_needs_imm32() {
    // 0x80 = 128, which doesn't fit in signed i8 (-128..=127)
    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(0x80)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    assert_eq!(
        bytes,
        &[0x68, 0x80, 0x00, 0x00, 0x00],
        "PUSH 0x80 should use imm32 form (68 80 00 00 00)"
    );
}

#[test]
fn push_imm8_minus_0x80_boundary_fits_i8() {
    // -0x80 = -128, the minimum value for i8
    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(-0x80)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    assert_eq!(
        bytes,
        &[0x6A, 0x80],
        "PUSH -0x80 should use imm8 form (6A 80)"
    );
}

#[test]
fn push_imm32_minus_0x81_boundary_needs_imm32() {
    // -0x81 = -129, which doesn't fit in signed i8
    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(-0x81)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    // -0x81 in little-endian i32 = [0x7F, 0xFF, 0xFF, 0xFF]
    assert_eq!(
        bytes,
        &[0x68, 0x7F, 0xFF, 0xFF, 0xFF],
        "PUSH -0x81 should use imm32 form (68 7F FF FF FF)"
    );
}

#[test]
fn push_imm32_0x12345678_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(0x12345678)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    // 0x12345678 in little-endian = [0x78, 0x56, 0x34, 0x12]
    assert_eq!(
        bytes,
        &[0x68, 0x78, 0x56, 0x34, 0x12],
        "PUSH 0x12345678 should encode as 68 78 56 34 12"
    );
}

#[test]
fn push_imm32_minus_1i32_uses_imm8_form() {
    // -1 as i32 still fits in i8, so should use the shorter imm8 form
    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(-1i64)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    assert_eq!(
        bytes,
        &[0x6A, 0xFF],
        "PUSH -1 should use imm8 form (6A FF) since -1 fits in i8"
    );
}

#[test]
fn push_imm8_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(0x42)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    assert_eq!(decoded.mnemonic(), IcedMnem::Push);
    assert_eq!(decoded.immediate8(), 0x42);
}

#[test]
fn push_imm32_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(0x12345678)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    assert_eq!(decoded.mnemonic(), IcedMnem::Push);
    // immediate32() returns u32; verify the value
    assert_eq!(decoded.immediate32(), 0x12345678u32);
}

#[test]
fn push_imm32_negative_value_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Push,
        operands: smallvec![Operand::Imm64(-0x81)],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let bytes = encode_instruction_bytes(&inst);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    assert_eq!(decoded.mnemonic(), IcedMnem::Push);
    // immediate32() returns u32; -0x81 as u32 is 0xFFFFFF7F
    assert_eq!(decoded.immediate32(), (-0x81i32) as u32);
}
