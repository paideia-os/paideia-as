//! Integration tests for `rol/ror r32/r64, imm8/cl` — PA-R15-004 (issue #959).
//!
//! Covers rotate left and rotate right instructions for 32-bit and 64-bit registers,
//! with immediate and CL (variable) forms. Includes special short encoding for imm=1.
//!
//! Test vectors were cross-checked against Intel SDM Vol 2A and NASM.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId, IntWidth};
use smallvec::smallvec;

/// Encode a two-operand instruction and return its bytes.
fn encode_reg_imm(mnemonic: Mnemonic, reg_id: u8, imm: i64) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic,
        operands: smallvec![Operand::Reg(RegId(reg_id)), Operand::Imm64(imm)],
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

/// Encode a two-operand instruction with register source and return its bytes.
fn encode_reg_reg(mnemonic: Mnemonic, dst_id: u8, src_id: u8) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic,
        operands: smallvec![Operand::Reg(RegId(dst_id)), Operand::Reg(RegId(src_id))],
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

// ===== ROL r64: byte-exact vectors =====

#[test]
fn rol_rax_1_emits_48_d1_c0() {
    // Short form: REX.W (0x48), D1, ModR/M = 11 000 000 = 0xC0
    assert_eq!(
        encode_reg_imm(Mnemonic::Rol { width: IntWidth::W64 }, 0, 1),
        &[0x48, 0xD1, 0xC0]
    );
}

#[test]
fn rol_rax_4_emits_48_c1_c0_04() {
    // General form: REX.W (0x48), C1, ModR/M = 11 000 000 = 0xC0, imm8 = 0x04
    assert_eq!(
        encode_reg_imm(Mnemonic::Rol { width: IntWidth::W64 }, 0, 4),
        &[0x48, 0xC1, 0xC0, 0x04]
    );
}

#[test]
fn rol_rax_cl_emits_48_d3_c0() {
    // CL form: REX.W (0x48), D3, ModR/M = 11 000 000 = 0xC0
    // Reg 0 = RAX, Reg 1 = RCX (CL)
    assert_eq!(
        encode_reg_reg(Mnemonic::Rol { width: IntWidth::W64 }, 0, 1),
        &[0x48, 0xD3, 0xC0]
    );
}

#[test]
fn ror_rax_1_emits_48_d1_c8() {
    // Short form: REX.W (0x48), D1, ModR/M = 11 001 000 = 0xC8
    assert_eq!(
        encode_reg_imm(Mnemonic::Ror { width: IntWidth::W64 }, 0, 1),
        &[0x48, 0xD1, 0xC8]
    );
}

#[test]
fn ror_rax_4_emits_48_c1_c8_04() {
    // General form: REX.W (0x48), C1, ModR/M = 11 001 000 = 0xC8, imm8 = 0x04
    assert_eq!(
        encode_reg_imm(Mnemonic::Ror { width: IntWidth::W64 }, 0, 4),
        &[0x48, 0xC1, 0xC8, 0x04]
    );
}

#[test]
fn ror_rax_cl_emits_48_d3_c8() {
    // CL form: REX.W (0x48), D3, ModR/M = 11 001 000 = 0xC8
    assert_eq!(
        encode_reg_reg(Mnemonic::Ror { width: IntWidth::W64 }, 0, 1),
        &[0x48, 0xD3, 0xC8]
    );
}

#[test]
fn rol_r8_1_emits_49_d1_c0() {
    // REX.W|REX.B (0x49), D1, ModR/M = 11 000 000 = 0xC0
    assert_eq!(
        encode_reg_imm(Mnemonic::Rol { width: IntWidth::W64 }, 8, 1),
        &[0x49, 0xD1, 0xC0]
    );
}

#[test]
fn rol_r15_63_emits_49_c1_c7_3f() {
    // REX.W|REX.B (0x49), C1, ModR/M = 11 000 111 = 0xC7, imm8 = 0x3F (63)
    assert_eq!(
        encode_reg_imm(Mnemonic::Rol { width: IntWidth::W64 }, 15, 63),
        &[0x49, 0xC1, 0xC7, 0x3F]
    );
}

#[test]
fn ror_r8_cl_emits_49_d3_c8() {
    // REX.W|REX.B (0x49), D3, ModR/M = 11 001 000 = 0xC8
    assert_eq!(
        encode_reg_reg(Mnemonic::Ror { width: IntWidth::W64 }, 8, 1),
        &[0x49, 0xD3, 0xC8]
    );
}

// ===== ROL r32: byte-exact vectors (W32 encodings) =====

#[test]
fn rol_eax_1_emits_d1_c0() {
    // No REX.W, D1, ModR/M = 11 000 000 = 0xC0
    assert_eq!(
        encode_reg_imm(Mnemonic::Rol { width: IntWidth::W32 }, 0, 1),
        &[0xD1, 0xC0]
    );
}

#[test]
fn ror_eax_1_emits_d1_c8() {
    // No REX.W, D1, ModR/M = 11 001 000 = 0xC8
    assert_eq!(
        encode_reg_imm(Mnemonic::Ror { width: IntWidth::W32 }, 0, 1),
        &[0xD1, 0xC8]
    );
}

#[test]
fn rol_r8d_cl_emits_41_d3_c0() {
    // REX.B (0x41), D3, ModR/M = 11 000 000 = 0xC0
    assert_eq!(
        encode_reg_reg(Mnemonic::Rol { width: IntWidth::W32 }, 8, 1),
        &[0x41, 0xD3, 0xC0]
    );
}

// ===== iced-x86 round-trip =====

#[test]
fn rol_rax_1_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let bytes = encode_reg_imm(Mnemonic::Rol { width: IntWidth::W64 }, 0, 1);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Rol);
}

#[test]
fn ror_rcx_cl_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let bytes = encode_reg_reg(Mnemonic::Ror { width: IntWidth::W64 }, 1, 1);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Ror);
}

#[test]
fn rol_r15_4_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let bytes = encode_reg_imm(Mnemonic::Rol { width: IntWidth::W64 }, 15, 4);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Rol);
}

// ===== ROL r16: byte-exact vectors (W16 encodings) — PA-R16-007 =====

#[test]
fn rol_ax_1_emits_66_d1_c0() {
    // Operand-size override (0x66), D1, ModR/M = 11 000 000 = 0xC0
    assert_eq!(
        encode_reg_imm(Mnemonic::Rol { width: IntWidth::W16 }, 0, 1),
        &[0x66, 0xD1, 0xC0]
    );
}

#[test]
fn rol_ax_8_emits_66_c1_c0_08() {
    // Operand-size override (0x66), C1, ModR/M = 11 000 000 = 0xC0, imm8 = 0x08
    assert_eq!(
        encode_reg_imm(Mnemonic::Rol { width: IntWidth::W16 }, 0, 8),
        &[0x66, 0xC1, 0xC0, 0x08]
    );
}

#[test]
fn rol_ax_cl_emits_66_d3_c0() {
    // Operand-size override (0x66), D3, ModR/M = 11 000 000 = 0xC0
    // Reg 0 = AX, Reg 1 = CX (CL)
    assert_eq!(
        encode_reg_reg(Mnemonic::Rol { width: IntWidth::W16 }, 0, 1),
        &[0x66, 0xD3, 0xC0]
    );
}

#[test]
fn rol_r10w_8_emits_66_41_c1_c2_08() {
    // Operand-size override (0x66), REX.B (0x41), C1, ModR/M = 11 000 010 = 0xC2, imm8 = 0x08
    assert_eq!(
        encode_reg_imm(Mnemonic::Rol { width: IntWidth::W16 }, 10, 8),
        &[0x66, 0x41, 0xC1, 0xC2, 0x08]
    );
}

#[test]
fn rol_r15w_cl_emits_66_41_d3_c7() {
    // Operand-size override (0x66), REX.B (0x41), D3, ModR/M = 11 000 111 = 0xC7
    assert_eq!(
        encode_reg_reg(Mnemonic::Rol { width: IntWidth::W16 }, 15, 1),
        &[0x66, 0x41, 0xD3, 0xC7]
    );
}

#[test]
fn rol_r10w_8_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let bytes = encode_reg_imm(Mnemonic::Rol { width: IntWidth::W16 }, 10, 8);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Rol);
}
