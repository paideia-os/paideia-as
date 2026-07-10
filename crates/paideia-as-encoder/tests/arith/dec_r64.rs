//! Integration tests for `dec r64` and `inc r64` — PA-R13-005 (issue #934).
//!
//! Covers REX.W FF /1 (DEC) and REX.W FF /0 (INC) forms for all 16 GP registers,
//! plus iced-x86 round-trip validation to catch any subtle ModR/M / REX drift.
//!
//! In x86_64 long mode the 1-byte legacy `40+rd` / `48+rd` INC/DEC forms are
//! repurposed as REX prefixes, so `REX.W FF /0` (INC) and `REX.W FF /1` (DEC)
//! are the canonical encodings.
//!
//! Test vectors were cross-checked against NASM and Intel SDM Vol 2A.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

/// Encode a single-register instruction and return its bytes.
fn encode_reg1(mnemonic: Mnemonic, reg_id: u8) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic,
        operands: smallvec![Operand::Reg(RegId(reg_id))],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encode_instruction failed");
    buf.as_slice().to_vec()
}

// ===== DEC r64: byte-exact vectors =====

#[test]
fn dec_rax_emits_48_ff_c8() {
    // REX.W (0x48), FF, ModR/M = 11 001 000 = 0xC8
    assert_eq!(encode_reg1(Mnemonic::Dec, 0), &[0x48, 0xFF, 0xC8]);
}

#[test]
fn dec_rdi_emits_48_ff_cf() {
    // REX.W (0x48), FF, ModR/M = 11 001 111 = 0xCF
    assert_eq!(encode_reg1(Mnemonic::Dec, 7), &[0x48, 0xFF, 0xCF]);
}

#[test]
fn dec_r8_emits_49_ff_c8() {
    // REX.W|REX.B (0x49), FF, ModR/M = 11 001 000 = 0xC8 (rm low 3 bits)
    assert_eq!(encode_reg1(Mnemonic::Dec, 8), &[0x49, 0xFF, 0xC8]);
}

#[test]
fn dec_r15_emits_49_ff_cf() {
    // REX.W|REX.B (0x49), FF, ModR/M = 11 001 111 = 0xCF
    assert_eq!(encode_reg1(Mnemonic::Dec, 15), &[0x49, 0xFF, 0xCF]);
}

// ===== INC r64: byte-exact vectors (scope-expansion for symmetry with DEC) =====

#[test]
fn inc_rax_emits_48_ff_c0() {
    // REX.W (0x48), FF, ModR/M = 11 000 000 = 0xC0
    assert_eq!(encode_reg1(Mnemonic::Inc, 0), &[0x48, 0xFF, 0xC0]);
}

#[test]
fn inc_rdi_emits_48_ff_c7() {
    assert_eq!(encode_reg1(Mnemonic::Inc, 7), &[0x48, 0xFF, 0xC7]);
}

#[test]
fn inc_r8_emits_49_ff_c0() {
    assert_eq!(encode_reg1(Mnemonic::Inc, 8), &[0x49, 0xFF, 0xC0]);
}

#[test]
fn inc_r15_emits_49_ff_c7() {
    assert_eq!(encode_reg1(Mnemonic::Inc, 15), &[0x49, 0xFF, 0xC7]);
}

// ===== iced-x86 round-trip =====

#[test]
fn dec_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let bytes = encode_reg1(Mnemonic::Dec, 0);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Dec);
    assert_eq!(decoded.op0_register(), Register::RAX);
    assert_eq!(decoded.len(), 3);
}

#[test]
fn dec_r15_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let bytes = encode_reg1(Mnemonic::Dec, 15);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Dec);
    assert_eq!(decoded.op0_register(), Register::R15);
    assert_eq!(decoded.len(), 3);
}

#[test]
fn inc_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let bytes = encode_reg1(Mnemonic::Inc, 0);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Inc);
    assert_eq!(decoded.op0_register(), Register::RAX);
    assert_eq!(decoded.len(), 3);
}

#[test]
fn inc_r15_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let bytes = encode_reg1(Mnemonic::Inc, 15);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Inc);
    assert_eq!(decoded.op0_register(), Register::R15);
    assert_eq!(decoded.len(), 3);
}

// ===== All 16 registers survive round-trip =====

#[test]
fn dec_all_16_registers_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    for reg in 0u8..16 {
        let bytes = encode_reg1(Mnemonic::Dec, reg);
        assert_eq!(bytes.len(), 3, "dec r{} unexpected length", reg);
        let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
        let decoded = decoder.decode();
        assert_eq!(decoded.mnemonic(), IcedMnem::Dec, "dec r{} mnemonic mismatch", reg);
    }
}

#[test]
fn inc_all_16_registers_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    for reg in 0u8..16 {
        let bytes = encode_reg1(Mnemonic::Inc, reg);
        assert_eq!(bytes.len(), 3, "inc r{} unexpected length", reg);
        let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
        let decoded = decoder.decode();
        assert_eq!(decoded.mnemonic(), IcedMnem::Inc, "inc r{} mnemonic mismatch", reg);
    }
}

// ===== Arity errors =====

#[test]
fn dec_with_zero_operands_errors() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Dec,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err(), "dec with zero operands must fail");
}

#[test]
fn inc_with_zero_operands_errors() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Inc,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err(), "inc with zero operands must fail");
}
