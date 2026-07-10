//! Integration tests for XSAVE family instructions — PA-R15-m4-005 (issue #1022).
//!
//! Covers:
//! - xsaveopt [mem]: 0F AE /6 (arity 1, memory operand, extended state save optimized)
//! - xrstor [mem]: 0F AE /5 (arity 1, memory operand, extended state restore)
//!
//! Test vectors were cross-checked against Intel SDM Vol 2A and NASM.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== xsaveopt [mem]: 0F AE /6 (arity 1) =====

#[test]
fn xsaveopt_rdi_emits_0f_ae_37() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xsaveopt,
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x37]);
}

#[test]
fn xsaveopt_rsp_emits_0f_ae_34_24() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xsaveopt,
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // SIB escape: ModRM=34, SIB=24
    assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x34, 0x24]);
}

#[test]
fn xsaveopt_r15_plus_0x100_emits_41_0f_ae_b7_00_01_00_00() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xsaveopt,
        operands: smallvec![
            Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 0x100 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // REX.B=41, ModRM=B7 (mod=10 disp32), disp32=0x100
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0xB7, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn xsaveopt_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xsaveopt,
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Xsaveopt);
    assert_eq!(decoded.len(), 3);
}

// ===== xrstor [mem]: 0F AE /5 (arity 1) =====

#[test]
fn xrstor_rdi_emits_0f_ae_2f() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xrstor,
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x2F]);
}

#[test]
fn xrstor_rsp_emits_0f_ae_2c_24() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xrstor,
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // SIB escape: ModRM=2C, SIB=24
    assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x2C, 0x24]);
}

#[test]
fn xrstor_r15_plus_0x100_emits_41_0f_ae_af_00_01_00_00() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xrstor,
        operands: smallvec![
            Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 0x100 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // REX.B=41, ModRM=AF (mod=10 disp32), disp32=0x100
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0xAF, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn xrstor_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xrstor,
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Xrstor);
    assert_eq!(decoded.len(), 3);
}

// ===== Arity errors =====

#[test]
fn xsaveopt_with_zero_operands_errors() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xsaveopt,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err(), "xsaveopt with zero operands must fail");
}

#[test]
fn xrstor_with_zero_operands_errors() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Xrstor,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err(), "xrstor with zero operands must fail");
}
