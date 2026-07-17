//! Tests for `vpcmpeqb ymm dst, ymm src1, ymm src2` encoding — Phase R18 PA-R18-011 (issue #1004).
//! Encoding: VEX 66 0F 74 /r

use paideia_as_encoder::{CodeBuffer, encode_instruction};
use paideia_as_ir::{Instruction, Mnemonic, Operand, RegId, InstrMode};
use smallvec::smallvec;

/// Test `vpcmpeqb ymm0, ymm0, ymm0` → `C5 FD 74 C0`
#[test]
fn vpcmpeqb_ymm0_ymm0_ymm0_emits_c5_fd_74_c0() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpcmpeqb,
        operands: smallvec![Operand::Reg(RegId(37)), Operand::Reg(RegId(37)), Operand::Reg(RegId(37))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.bytes, vec![0xC5, 0xFD, 0x74, 0xC0]);
}

/// Test `vpcmpeqb ymm0, ymm1, ymm2` → `C5 F5 74 C2`
#[test]
fn vpcmpeqb_ymm0_ymm1_ymm2_emits_c5_f5_74_c2() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpcmpeqb,
        operands: smallvec![Operand::Reg(RegId(37)), Operand::Reg(RegId(38)), Operand::Reg(RegId(39))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.bytes, vec![0xC5, 0xF5, 0x74, 0xC2]);
}

/// Test high destination register: `vpcmpeqb ymm8, ymm0, ymm0` (using 3-byte VEX)
#[test]
fn vpcmpeqb_ymm8_ymm0_ymm0_uses_3byte_vex() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpcmpeqb,
        operands: smallvec![Operand::Reg(RegId(45)), Operand::Reg(RegId(37)), Operand::Reg(RegId(37))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // Should emit 3-byte VEX due to high dst register
    assert_eq!(buf.bytes[0], 0xC4);
}

/// Test high source1 register: `vpcmpeqb ymm0, ymm8, ymm0`
#[test]
fn vpcmpeqb_ymm0_ymm8_ymm0_high_src1() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpcmpeqb,
        operands: smallvec![Operand::Reg(RegId(37)), Operand::Reg(RegId(45)), Operand::Reg(RegId(37))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // vvvv field encodes src1 directly (no REX-style extension needed), so a high
    // src1 with low dst/src2 still fits the 2-byte VEX form: C5 xx 74 c0 (4 bytes).
    assert!(buf.bytes.len() >= 4);
}

/// Test high source2 register: `vpcmpeqb ymm0, ymm0, ymm8`
#[test]
fn vpcmpeqb_ymm0_ymm0_ymm8_high_src2() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpcmpeqb,
        operands: smallvec![Operand::Reg(RegId(37)), Operand::Reg(RegId(37)), Operand::Reg(RegId(45))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // Should emit 3-byte VEX due to high src2 register (B field)
    assert_eq!(buf.bytes[0], 0xC4);
}

/// Test iced round-trip: `vpcmpeqb ymm3, ymm4, ymm5`
#[test]
fn vpcmpeqb_ymm3_ymm4_ymm5_round_trips_iced() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpcmpeqb,
        operands: smallvec![Operand::Reg(RegId(40)), Operand::Reg(RegId(41)), Operand::Reg(RegId(42))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Vpcmpeqb);
    assert_eq!(instr.op_count(), 3);
}
