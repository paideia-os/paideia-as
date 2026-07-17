//! Tests for `vpxor ymm dst, ymm src1, ymm src2` encoding — Phase R18 PA-R18-011 (issue #1004).
//! Encoding: VEX 66 0F EF /r

use paideia_as_encoder::{CodeBuffer, encode_instruction};
use paideia_as_ir::{Instruction, Mnemonic, Operand, RegId, InstrMode};
use smallvec::smallvec;

/// Test `vpxor ymm0, ymm0, ymm0` → `C5 FD EF C0`
/// 2-byte VEX: C5 FD (R=0, vvvv=~0=F, L=1, pp=1)
#[test]
fn vpxor_ymm0_ymm0_ymm0_emits_c5_fd_ef_c0() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpxor,
        operands: smallvec![Operand::Reg(RegId(37)), Operand::Reg(RegId(37)), Operand::Reg(RegId(37))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.bytes, vec![0xC5, 0xFD, 0xEF, 0xC0]);
}

/// Test `vpxor ymm0, ymm0, ymm7` → `C5 FD EF C7`
#[test]
fn vpxor_ymm0_ymm0_ymm7_emits_c5_fd_ef_c7() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpxor,
        operands: smallvec![Operand::Reg(RegId(37)), Operand::Reg(RegId(37)), Operand::Reg(RegId(44))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.bytes, vec![0xC5, 0xFD, 0xEF, 0xC7]);
}

/// Test `vpxor ymm0, ymm0, ymm8` → `C4 C1 7D EF C0`
/// 3-byte VEX: C4 C1 7D (R=1, X=1, B=0 [high src2], map=1, W=0, vvvv=F, L=1, pp=1)
#[test]
fn vpxor_ymm0_ymm0_ymm8_emits_c4_c1_7d_ef_c0() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpxor,
        operands: smallvec![Operand::Reg(RegId(37)), Operand::Reg(RegId(37)), Operand::Reg(RegId(45))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // dst=0 (low), src1=0 (low), src2=8 (high) → R=1, X=1, B=0
    // Byte 0: 1100 0001 = C1
    assert_eq!(buf.bytes, vec![0xC4, 0xC1, 0x7D, 0xEF, 0xC0]);
}

/// Test `vpxor ymm8, ymm0, ymm0` → `C4 61 7D EF C0`
/// 3-byte VEX: C4 61 7D (R=0 [dst high], X=1 [no index], B=1 [src2 low], map=1, W=0, vvvv=F, L=1, pp=1)
#[test]
fn vpxor_ymm8_ymm0_ymm0_emits_c4_61_7d_ef_c0() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpxor,
        operands: smallvec![Operand::Reg(RegId(45)), Operand::Reg(RegId(37)), Operand::Reg(RegId(37))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // dst=8 (high) → R=0, no index → X=1, src2=0 (low) → B=1
    // Byte 0: 0110 0001 = 61
    assert_eq!(buf.bytes, vec![0xC4, 0x61, 0x7D, 0xEF, 0xC0]);
}

/// Test `vpxor ymm15, ymm15, ymm15` → `C4 41 05 EF FF`
/// All high registers (15 = 8 + 7)
#[test]
fn vpxor_ymm15_ymm15_ymm15_emits_c4_41_05_ef_ff() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpxor,
        operands: smallvec![Operand::Reg(RegId(52)), Operand::Reg(RegId(52)), Operand::Reg(RegId(52))],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // dst=15 (high) → R=0, no index → X=1, src2=15 (high) → B=0
    // Byte 0: 0100 0001 = 41
    assert_eq!(buf.bytes, vec![0xC4, 0x41, 0x05, 0xEF, 0xFF]);
}

/// Test iced round-trip: `vpxor ymm3, ymm4, ymm5`
#[test]
fn vpxor_ymm3_ymm4_ymm5_round_trips_iced() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vpxor,
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

    assert_eq!(instr.mnemonic(), IcedMnem::Vpxor);
    assert_eq!(instr.op_count(), 3);
}
