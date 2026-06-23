//! Integration tests for narrow-form MOV encoding (PA10-004).
//!
//! Tests verify that narrow MOV instructions (mov reg8, imm8; mov reg16, imm16) are
//! correctly encoded with byte-exact validation against test vectors and iced-x86 round-trip.
//!
//! Suite A: Byte-exact encoding validation (3 test vectors from softarch).
//! Suite B: 16-variant iced-x86 round-trip matrix covering all 8-bit register forms.
//!
//! Registers tested:
//! - Low-byte (al, cl, dl, bl): RegId(0–3), no REX
//! - High-byte (ah, ch, dh, bh): RegId(4–7), no REX (high-byte form)
//! - Extended low-byte (r8b–r15b): RegId(8–15), REX.B = 0x41
//!
//! Immediates: small (0x01, 0x07) and large (0x80, 0xF8, 0x3F8 for 16-bit).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (softarch test vectors) =====

#[test]
fn mov_al_0x80_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W8,
        },
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x80)], // al, 0x80
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov al, 0x80");

    // Expected: B0 80 (2 bytes)
    assert_eq!(buf.as_slice(), &[0xB0, 0x80]);
}

#[test]
fn mov_dx_0x3f8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W16,
        },
        operands: smallvec![Operand::Reg(RegId(2)), Operand::Imm64(0x3F8)], // dx, 0x3F8
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov dx, 0x3F8");

    // Expected: 66 BA F8 03 (4 bytes, W16 requires 0x66 prefix)
    assert_eq!(buf.as_slice(), &[0x66, 0xBA, 0xF8, 0x03]);
}

#[test]
fn mov_r10b_0x07_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W8,
        },
        operands: smallvec![Operand::Reg(RegId(10)), Operand::Imm64(0x07)], // r10b, 0x07
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r10b, 0x07");

    // Expected: 41 B2 07 (3 bytes, REX.B = 0x41, 0xB0 + (10 & 7) = 0xB2)
    assert_eq!(buf.as_slice(), &[0x41, 0xB2, 0x07]);
}

// ===== Suite B: iced-x86 Round-trip Matrix (16 8-bit variants) =====

// Helper to test a single register + immediate pair via iced round-trip
fn test_mov_reg8_imm_round_trip(reg_id: u8, imm: u8) {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W8,
        },
        operands: smallvec![Operand::Reg(RegId(reg_id)), Operand::Imm64(imm as i64)],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect(&format!("encoding failed for reg_id={}", reg_id));

    // Decode and validate
    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();

    // Validate mnemonic
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);

    // Validate immediate
    assert_eq!(decoded.immediate8() as u8, imm);

    // Validate operand count
    assert_eq!(decoded.op_count(), 2);

    // Expected byte count validation:
    // - RegId(0–3) and RegId(4–7): 2 bytes (no REX)
    // - RegId(8–15): 3 bytes (REX.B)
    match reg_id {
        0..=7 => assert_eq!(buf.len(), 2, "low-byte/high-byte reg should be 2 bytes"),
        8..=15 => assert_eq!(buf.len(), 3, "extended reg should be 3 bytes with REX.B"),
        _ => panic!("invalid reg_id"),
    }

    // Validate REX.B presence/absence
    if reg_id >= 8 {
        // REX.B required: first byte should be 0x41
        assert_eq!(buf.as_slice()[0], 0x41, "REX.B required for reg_id >= 8");
    } else {
        // No REX: first byte should be 0xB0 + (reg_id & 7)
        assert_eq!(
            buf.as_slice()[0],
            0xB0 + (reg_id & 7),
            "no REX for reg_id < 8"
        );
    }
}

// Test all 16 registers with two immediate values each (small: 0x01, large: 0x80)
#[test]
fn mov_al_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(0, 0x01);
}

#[test]
fn mov_al_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(0, 0x80);
}

#[test]
fn mov_cl_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(1, 0x01);
}

#[test]
fn mov_cl_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(1, 0x80);
}

#[test]
fn mov_dl_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(2, 0x01);
}

#[test]
fn mov_dl_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(2, 0x80);
}

#[test]
fn mov_bl_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(3, 0x01);
}

#[test]
fn mov_bl_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(3, 0x80);
}

#[test]
fn mov_ah_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(4, 0x01);
}

#[test]
fn mov_ah_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(4, 0x80);
}

#[test]
fn mov_ch_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(5, 0x01);
}

#[test]
fn mov_ch_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(5, 0x80);
}

#[test]
fn mov_dh_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(6, 0x01);
}

#[test]
fn mov_dh_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(6, 0x80);
}

#[test]
fn mov_bh_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(7, 0x01);
}

#[test]
fn mov_bh_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(7, 0x80);
}

#[test]
fn mov_r8b_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(8, 0x01);
}

#[test]
fn mov_r8b_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(8, 0x80);
}

#[test]
fn mov_r9b_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(9, 0x01);
}

#[test]
fn mov_r9b_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(9, 0x80);
}

#[test]
fn mov_r10b_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(10, 0x01);
}

#[test]
fn mov_r10b_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(10, 0x80);
}

#[test]
fn mov_r11b_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(11, 0x01);
}

#[test]
fn mov_r11b_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(11, 0x80);
}

#[test]
fn mov_r12b_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(12, 0x01);
}

#[test]
fn mov_r12b_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(12, 0x80);
}

#[test]
fn mov_r13b_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(13, 0x01);
}

#[test]
fn mov_r13b_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(13, 0x80);
}

#[test]
fn mov_r14b_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(14, 0x01);
}

#[test]
fn mov_r14b_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(14, 0x80);
}

#[test]
fn mov_r15b_0x01_round_trip() {
    test_mov_reg8_imm_round_trip(15, 0x01);
}

#[test]
fn mov_r15b_0x80_round_trip() {
    test_mov_reg8_imm_round_trip(15, 0x80);
}
