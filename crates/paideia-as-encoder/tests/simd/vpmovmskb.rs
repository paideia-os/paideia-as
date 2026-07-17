//! Tests for `vpmovmskb r32 dst, ymm src` encoding — Phase R18 PA-R18-011 (issue #1004).
//! Encoding: VEX 66 0F D7 /r

use paideia_as_encoder::{CodeBuffer, encode_instruction};
use paideia_as_ir::{Instruction, Mnemonic, Operand, RegId, InstrMode};
use smallvec::smallvec;

/// Test `vpmovmskb eax, ymm0` → `C5 FD D7 C0`
#[test]
fn vpmovmskb_eax_ymm0_emits_c5_fd_d7_c0() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpmovmskb,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(37))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.bytes, vec![0xC5, 0xFD, 0xD7, 0xC0]);
}

/// Test `vpmovmskb eax, ymm7` → `C5 FD D7 C7`
#[test]
fn vpmovmskb_eax_ymm7_emits_c5_fd_d7_c7() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpmovmskb,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(44))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.bytes, vec![0xC5, 0xFD, 0xD7, 0xC7]);
}

/// Test `vpmovmskb r8d, ymm0` → `C4 61 7D D7 C0`
/// High destination register (r8d = RegId 8) requires 3-byte VEX
#[test]
fn vpmovmskb_r8d_ymm0_emits_c4_61_7d_d7_c0() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpmovmskb,
        operands: smallvec![Operand::Reg(RegId(8)), Operand::Reg(RegId(37))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // dst=8 (high) → R=0, no index → X=1, src=0 (low) → B=1
    // Byte 0: 0110 0001 = 61
    assert_eq!(buf.bytes, vec![0xC4, 0x61, 0x7D, 0xD7, 0xC0]);
}

/// Test `vpmovmskb eax, ymm8` → `C4 C1 7D D7 C0`
/// High source register (ymm8) requires 3-byte VEX
#[test]
fn vpmovmskb_eax_ymm8_emits_c4_c1_7d_d7_c0() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpmovmskb,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(45))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // dst=0 (low), src=8 (high) → R=1, X=1, B=0
    // Byte 0: 1100 0001 = C1
    assert_eq!(buf.bytes, vec![0xC4, 0xC1, 0x7D, 0xD7, 0xC0]);
}

/// Test iced round-trip: `vpmovmskb eax, ymm3`
#[test]
fn vpmovmskb_eax_ymm3_round_trips_iced() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpmovmskb,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(40))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Vpmovmskb);
    assert_eq!(instr.op_count(), 2);
}
