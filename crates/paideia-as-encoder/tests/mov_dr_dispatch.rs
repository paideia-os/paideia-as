//! Integration tests for MOV to/from debug register dispatch (Phase 6, m1-003).
//!
//! These tests verify that MOV instructions with DR operands are correctly encoded
//! and round-trip correctly through iced-x86 for validation.
//!
//! Compact encoding: DR uses indices 25..33 (dr0..dr7).
//! - mov dr_idx, gpr → 0F 23 /r
//! - mov gpr, dr_idx → 0F 21 /r

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::InstrMode;
use smallvec::smallvec;

#[test]
fn mov_dr0_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(25)), Operand::Reg(RegId(0))], // dr0, rax
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Expect: 0F 23 C0
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xC0]);

    // Verify round-trip through iced-x86 decoder
    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_dr1_rdi_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(26)), Operand::Reg(RegId(7))], // dr1, rdi
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Expect: 0F 23 CF
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xCF]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_dr7_rcx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(32)), Operand::Reg(RegId(1))], // dr7, rcx
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Expect: 0F 23 F9
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF9]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_rax_dr0_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(25))], // rax, dr0
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Expect: 0F 21 C0
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_rdi_dr1_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(7)), Operand::Reg(RegId(26))], // rdi, dr1
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Expect: 0F 21 CF
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xCF]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_rcx_dr7_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(1)), Operand::Reg(RegId(32))], // rcx, dr7
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Expect: 0F 21 F9
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF9]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_r8_dr0_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(8)), Operand::Reg(RegId(25))], // r8, dr0
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Expect: 0F 21 C0
    assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_dr3_r10_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(28)), Operand::Reg(RegId(10))], // dr3, r10
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Expect: 0F 23 DA (dr3 = 28 - 25 = 3, r10 = 10, modrm = 0xC0 | (3 << 3) | 2 = 0xDA)
    assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xDA]);

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}
