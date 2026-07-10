//! Integration tests for cache control instructions — PA-R14-005 (issue #948).
//!
//! Covers:
//! - wbinvd: 0F 09 (arity 0, privileged)
//! - invd: 0F 08 (arity 0, privileged)
//! - clflush [mem]: 0F AE /7 (arity 1, memory operand)
//! - clflushopt [mem]: 66 0F AE /7 (arity 1, memory operand with 0x66 prefix)
//!
//! Test vectors were cross-checked against Intel SDM Vol 2A and NASM.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== wbinvd: 0F 09 (arity 0) =====

#[test]
fn wbinvd_emits_0f_09() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Wbinvd,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.as_slice(), &[0x0F, 0x09]);
}

#[test]
fn wbinvd_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Wbinvd,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Wbinvd);
    assert_eq!(decoded.len(), 2);
}

#[test]
fn wbinvd_with_operands_errors() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Wbinvd,
        operands: smallvec![Operand::Reg(RegId(0))],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err(), "wbinvd with operands must fail");
}

// ===== invd: 0F 08 (arity 0) =====

#[test]
fn invd_emits_0f_08() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Invd,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.as_slice(), &[0x0F, 0x08]);
}

#[test]
fn invd_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Invd,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Invd);
    assert_eq!(decoded.len(), 2);
}

#[test]
fn invd_with_operands_errors() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Invd,
        operands: smallvec![Operand::Reg(RegId(0))],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err(), "invd with operands must fail");
}

// ===== clflush [mem]: 0F AE /7 (arity 1) =====

#[test]
fn clflush_rdi_emits_0f_ae_3f() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflush,
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
    assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x3F]);
}

#[test]
fn clflush_r12_plus_8_emits_41_0f_ae_7c_24_08() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflush,
        operands: smallvec![
            Operand::MemSib { base: RegId(12), index: None, scale: Scale::X1, disp: 8 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // REX.B (0x41) + 0F AE + SIB (7C 24) + disp8 (08)
    assert_eq!(buf.as_slice(), &[0x41, 0x0F, 0xAE, 0x7C, 0x24, 0x08]);
}

#[test]
fn clflush_rsi_emits_0f_ae_3e() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflush,
        operands: smallvec![
            Operand::MemSib { base: RegId(6), index: None, scale: Scale::X1, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x3E]);
}

#[test]
fn clflush_rax_emits_0f_ae_38() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflush,
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.as_slice(), &[0x0F, 0xAE, 0x38]);
}

#[test]
fn clflush_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflush,
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
    assert_eq!(decoded.mnemonic(), IcedMnem::Clflush);
    assert_eq!(decoded.len(), 3);
}

// ===== clflushopt [mem]: 66 0F AE /7 (arity 1 with 0x66 prefix) =====

#[test]
fn clflushopt_rdi_emits_66_0f_ae_3f() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflushopt,
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
    assert_eq!(buf.as_slice(), &[0x66, 0x0F, 0xAE, 0x3F]);
}

#[test]
fn clflushopt_r12_plus_8_emits_66_41_0f_ae_7c_24_08() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflushopt,
        operands: smallvec![
            Operand::MemSib { base: RegId(12), index: None, scale: Scale::X1, disp: 8 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // 0x66 + REX.B (0x41) + 0F AE + SIB (7C 24) + disp8 (08)
    assert_eq!(buf.as_slice(), &[0x66, 0x41, 0x0F, 0xAE, 0x7C, 0x24, 0x08]);
}

#[test]
fn clflushopt_rsi_emits_66_0f_ae_3e() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflushopt,
        operands: smallvec![
            Operand::MemSib { base: RegId(6), index: None, scale: Scale::X1, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.as_slice(), &[0x66, 0x0F, 0xAE, 0x3E]);
}

#[test]
fn clflushopt_rax_emits_66_0f_ae_38() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflushopt,
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.as_slice(), &[0x66, 0x0F, 0xAE, 0x38]);
}

#[test]
fn clflushopt_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflushopt,
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
    assert_eq!(decoded.mnemonic(), IcedMnem::Clflushopt);
    assert_eq!(decoded.len(), 4);
}

// ===== Arity errors =====

#[test]
fn clflush_with_zero_operands_errors() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflush,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err(), "clflush with zero operands must fail");
}

#[test]
fn clflushopt_with_zero_operands_errors() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Clflushopt,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err(), "clflushopt with zero operands must fail");
}
