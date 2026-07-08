//! Integration tests for `test r64, imm32` — PA-R13-006 (issue #935).
//!
//! Covers the general `REX.W F7 /0 id` encoding for R8..R15 + all
//! non-RAX low registers, and the `REX.W A9 id` short form special-case
//! for RAX. iced-x86 round-trip pins ModR/M + REX + imm layout.
//!
//! Retires the R14B `and r64, imm` + `cmp r64, 0` workaround: the encoder
//! now emits a single-instruction TEST with a sign-extended imm32.
//!
//! Test vectors were cross-checked against NASM and Intel SDM Vol 2A (TEST).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

/// Encode a `test r64, imm64` instruction and return its bytes.
fn encode_test_reg_imm(reg_id: u8, imm: i64) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Test,
        operands: smallvec![Operand::Reg(RegId(reg_id)), Operand::Imm64(imm)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encode_instruction failed");
    buf.as_slice().to_vec()
}

/// Encode a `test r64, imm64` instruction and return the encoder result
/// (used for negative tests that expect `EncodeError`).
fn try_encode_test_reg_imm(
    reg_id: u8,
    imm: i64,
) -> Result<Vec<u8>, paideia_as_encoder::EncodeError> {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Test,
        operands: smallvec![Operand::Reg(RegId(reg_id)), Operand::Imm64(imm)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)?;
    Ok(buf.as_slice().to_vec())
}

// ===== byte-exact vectors =====

#[test]
fn test_rax_imm_uses_short_form_48_a9() {
    // Short form: REX.W A9 <imm32 LE> → 6 bytes total
    // 0x100 = 00 01 00 00
    assert_eq!(
        encode_test_reg_imm(0, 0x100),
        &[0x48, 0xA9, 0x00, 0x01, 0x00, 0x00]
    );
}

#[test]
fn test_rbx_imm_uses_general_form_48_f7_c3() {
    // General form: REX.W F7 /0 <imm32 LE> → 7 bytes total
    // ModR/M = 11 000 011 = 0xC3 (rbx = 3)
    assert_eq!(
        encode_test_reg_imm(3, 0x100),
        &[0x48, 0xF7, 0xC3, 0x00, 0x01, 0x00, 0x00]
    );
}

#[test]
fn test_r8_imm_uses_general_form_with_rex_b_49_f7_c0() {
    // General form: REX.W|REX.B F7 /0 <imm32 LE> → 7 bytes total
    // r8 → REX = 49 (REX.W|REX.B), ModR/M = 11 000 000 = 0xC0 (rm low 3 bits)
    assert_eq!(
        encode_test_reg_imm(8, 1),
        &[0x49, 0xF7, 0xC0, 0x01, 0x00, 0x00, 0x00]
    );
}

#[test]
fn test_rdi_imm_negative_one_sign_extended() {
    // General form: REX.W F7 /0 <imm32 LE> → 7 bytes total
    // ModR/M = 11 000 111 = 0xC7 (rdi = 7)
    // -1 as i32 → 0xFFFFFFFF little-endian (sign-extended to i64 by CPU)
    assert_eq!(
        encode_test_reg_imm(7, -1),
        &[0x48, 0xF7, 0xC7, 0xFF, 0xFF, 0xFF, 0xFF]
    );
}

#[test]
fn test_r15_imm_uses_general_form_with_rex_b_49_f7_c7() {
    // General form: REX.W|REX.B F7 /0 <imm32 LE> → 7 bytes total
    // r15 → REX = 49 (REX.W|REX.B), ModR/M = 11 000 111 = 0xC7
    assert_eq!(
        encode_test_reg_imm(15, 0x7F),
        &[0x49, 0xF7, 0xC7, 0x7F, 0x00, 0x00, 0x00]
    );
}

// ===== short-form is only used for RAX =====

#[test]
fn test_rcx_does_not_use_short_form() {
    // rcx must NOT emit A9 — that opcode is RAX-specific.
    let bytes = encode_test_reg_imm(1, 0x100);
    assert_ne!(bytes[1], 0xA9, "rcx must use F7 /0, not the A9 short form");
    // ModR/M = 11 000 001 = 0xC1 (rcx = 1)
    assert_eq!(bytes, &[0x48, 0xF7, 0xC1, 0x00, 0x01, 0x00, 0x00]);
}

// ===== range enforcement =====

#[test]
fn test_reg_imm_over_i32_max_errors() {
    // TEST has no imm64 form; encoder must refuse and steer callers back to
    // the and+cmp workaround rather than silently truncate.
    let result = try_encode_test_reg_imm(0, (i32::MAX as i64) + 1);
    assert!(result.is_err(), "imm > i32::MAX must fail");
}

#[test]
fn test_reg_imm_under_i32_min_errors() {
    let result = try_encode_test_reg_imm(3, (i32::MIN as i64) - 1);
    assert!(result.is_err(), "imm < i32::MIN must fail");
}

#[test]
fn test_reg_imm_at_i32_max_ok() {
    // Boundary: i32::MAX is representable — must encode successfully.
    let bytes = encode_test_reg_imm(3, i32::MAX as i64);
    assert_eq!(
        bytes,
        &[0x48, 0xF7, 0xC3, 0xFF, 0xFF, 0xFF, 0x7F],
        "i32::MAX must round-trip as the full-32-bit imm"
    );
}

#[test]
fn test_reg_imm_at_i32_min_ok() {
    // Boundary: i32::MIN is representable and encodes as 0x80000000 LE.
    let bytes = encode_test_reg_imm(3, i32::MIN as i64);
    assert_eq!(bytes, &[0x48, 0xF7, 0xC3, 0x00, 0x00, 0x00, 0x80]);
}

// ===== iced-x86 round-trip =====

#[test]
fn test_rax_imm_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, OpKind, Register};

    let bytes = encode_test_reg_imm(0, 0x100);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Test);
    assert_eq!(decoded.op0_register(), Register::RAX);
    assert_eq!(decoded.op1_kind(), OpKind::Immediate32to64);
    assert_eq!(decoded.immediate32to64(), 0x100);
    assert_eq!(decoded.len(), 6, "test rax, imm32 short form is 6 bytes");
}

#[test]
fn test_r8_imm_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, OpKind, Register};

    let bytes = encode_test_reg_imm(8, 1);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Test);
    assert_eq!(decoded.op0_register(), Register::R8);
    assert_eq!(decoded.op1_kind(), OpKind::Immediate32to64);
    assert_eq!(decoded.immediate32to64(), 1);
    assert_eq!(decoded.len(), 7, "test r8, imm32 general form is 7 bytes");
}

#[test]
fn test_rdi_negative_imm_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let bytes = encode_test_reg_imm(7, -1);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Test);
    assert_eq!(decoded.op0_register(), Register::RDI);
    // Sign-extended: full 64-bit operand reads as -1
    assert_eq!(decoded.immediate32to64(), -1);
}

#[test]
fn test_all_16_registers_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    for reg in 0u8..16 {
        let bytes = encode_test_reg_imm(reg, 0x12345678);
        let expected_len = if reg == 0 { 6 } else { 7 };
        assert_eq!(
            bytes.len(),
            expected_len,
            "test r{} unexpected length",
            reg
        );
        let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
        let decoded = decoder.decode();
        assert_eq!(
            decoded.mnemonic(),
            IcedMnem::Test,
            "test r{} mnemonic mismatch",
            reg
        );
        assert_eq!(
            decoded.immediate32to64(),
            0x12345678,
            "test r{} immediate mismatch",
            reg
        );
    }
}
