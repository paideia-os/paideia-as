//! Integration tests for three-operand IMUL (pa-r13-009 / issue #938).
//!
//! Three-operand `imul dst, src, imm` lets us write `imul rax, rsi, 8`
//! for index scaling without clobbering the source. This unblocks the
//! paideia-os `elf_lite_load` path that previously worked around the
//! missing form by materialising the constant via
//!   `mov r11, 56; imul r10, r11`
//! for `phdr_offset = i * 56` instead of the natural
//!   `imul r10, r15, 56`.
//!
//! Encodings covered:
//! - `REX.W 6B /r ib` (imm8, sign-extended) when imm ∈ [-128, 127]
//! - `REX.W 69 /r id` (imm32, sign-extended) otherwise (i32 range)
//!
//! ModR/M is the IMUL-inverted layout: `dst` occupies the reg field
//! (REX.R extends dst) and `src` occupies the r/m field (REX.B extends src).
//! Test vectors below cover the extension-bit corners (rax/rbx baseline,
//! r8-as-dst → REX.R, r8-as-src → REX.B) and the imm8↔imm32 boundary
//! at ±128, plus a negative-immediate sign-extension case and one
//! iced-x86 semantic round-trip per encoding form.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

/// Build a three-operand IMUL instruction with the requested dst/src/imm.
fn imul3(dst: u8, src: u8, imm: i64) -> Instruction {
    Instruction {
        mnemonic: Mnemonic::Imul,
        operands: smallvec![
            Operand::Reg(RegId(dst)),
            Operand::Reg(RegId(src)),
            Operand::Imm64(imm),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
}
}

/// Encode `inst` and return the emitted byte slice as an owned Vec.
fn encode(inst: &Instruction) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(inst, &mut buf, &mut stats)
        .expect("encoding failed");
    buf.as_slice().to_vec()
}

// ===== Baseline (no REX.R / REX.B) =====

#[test]
fn imul_rax_rbx_4_uses_imm8_form() {
    // imul rax, rbx, 4 → 48 6B C3 04
    assert_eq!(encode(&imul3(0, 3, 4)), &[0x48, 0x6B, 0xC3, 0x04]);
}

#[test]
fn imul_rax_rbx_neg1_sign_extends_in_imm8_form() {
    // imul rax, rbx, -1 → 48 6B C3 FF (i8 → 0xFF, sign-extended by CPU)
    assert_eq!(encode(&imul3(0, 3, -1)), &[0x48, 0x6B, 0xC3, 0xFF]);
}

#[test]
fn imul_rax_rbx_1000_uses_imm32_form() {
    // imul rax, rbx, 1000 → 48 69 C3 E8 03 00 00 (0x3E8 little-endian)
    assert_eq!(
        encode(&imul3(0, 3, 1000)),
        &[0x48, 0x69, 0xC3, 0xE8, 0x03, 0x00, 0x00]
    );
}

// ===== REX prefix extensions =====

#[test]
fn imul_r8_rbx_4_sets_rex_r_for_extended_dst() {
    // imul r8, rbx, 4 → 4C 6B C3 04
    // dst=r8 (id 8) → REX.R=1; src=rbx (id 3) → REX.B=0
    // REX = 0100 W R X B = 0100 1100 = 0x4C
    assert_eq!(encode(&imul3(8, 3, 4)), &[0x4C, 0x6B, 0xC3, 0x04]);
}

#[test]
fn imul_rax_r8_4_sets_rex_b_for_extended_src() {
    // imul rax, r8, 4 → 49 6B C0 04
    // dst=rax (id 0) → REX.R=0; src=r8 (id 8) → REX.B=1
    // REX = 0100 W R X B = 0100 1001 = 0x49
    // ModR/M = 0xC0 | (0 << 3) | 0 = 0xC0
    assert_eq!(encode(&imul3(0, 8, 4)), &[0x49, 0x6B, 0xC0, 0x04]);
}

#[test]
fn imul_r15_r15_neg128_sets_both_rex_bits() {
    // imul r15, r15, -128 → 4D 6B FF 80
    // REX.W=1, REX.R=1 (dst=r15), REX.B=1 (src=r15) → 0x4D
    // ModR/M = 0xC0 | ((15 & 7) << 3) | (15 & 7) = 0xC0 | 0x38 | 0x07 = 0xFF
    assert_eq!(encode(&imul3(15, 15, -128)), &[0x4D, 0x6B, 0xFF, 0x80]);
}

// ===== imm8 ↔ imm32 boundary =====

#[test]
fn imul_boundary_pos_127_uses_imm8_form() {
    // 127 = i8::MAX → still fits imm8 → 48 6B C3 7F
    assert_eq!(encode(&imul3(0, 3, 127)), &[0x48, 0x6B, 0xC3, 0x7F]);
}

#[test]
fn imul_boundary_pos_128_promotes_to_imm32_form() {
    // 128 > i8::MAX → must widen to imm32 → 48 69 C3 80 00 00 00
    assert_eq!(
        encode(&imul3(0, 3, 128)),
        &[0x48, 0x69, 0xC3, 0x80, 0x00, 0x00, 0x00]
    );
}

#[test]
fn imul_boundary_neg_128_uses_imm8_form() {
    // -128 = i8::MIN → still fits imm8 → 48 6B C3 80
    assert_eq!(encode(&imul3(0, 3, -128)), &[0x48, 0x6B, 0xC3, 0x80]);
}

#[test]
fn imul_boundary_neg_129_promotes_to_imm32_form() {
    // -129 < i8::MIN → must widen to imm32.
    // -129 as i32 = 0xFFFFFF7F → little-endian 7F FF FF FF
    assert_eq!(
        encode(&imul3(0, 3, -129)),
        &[0x48, 0x69, 0xC3, 0x7F, 0xFF, 0xFF, 0xFF]
    );
}

// ===== iced-x86 semantic round-trip =====

#[test]
fn imul_three_op_imm8_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, OpKind, Register};

    // imul rax, rbx, 4 → decoded operands: (rax, rbx, 4)
    let bytes = encode(&imul3(0, 3, 4));
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    assert_eq!(decoded.mnemonic(), IcedMnem::Imul);
    assert_eq!(decoded.op_count(), 3);
    assert_eq!(decoded.op0_register(), Register::RAX);
    assert_eq!(decoded.op1_register(), Register::RBX);
    // iced sign-extends imm8 → i64
    assert_eq!(decoded.op2_kind(), OpKind::Immediate8to64);
    assert_eq!(decoded.immediate8to64(), 4);
}

#[test]
fn imul_three_op_imm32_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, OpKind, Register};

    // imul rax, rbx, 1000 → decoded operands: (rax, rbx, 1000)
    let bytes = encode(&imul3(0, 3, 1000));
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    assert_eq!(decoded.mnemonic(), IcedMnem::Imul);
    assert_eq!(decoded.op_count(), 3);
    assert_eq!(decoded.op0_register(), Register::RAX);
    assert_eq!(decoded.op1_register(), Register::RBX);
    // iced sign-extends imm32 → i64
    assert_eq!(decoded.op2_kind(), OpKind::Immediate32to64);
    assert_eq!(decoded.immediate32to64(), 1000);
}

#[test]
fn imul_three_op_rex_r_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    // imul r8, rbx, 4 exercises REX.R on the dst reg field.
    let bytes = encode(&imul3(8, 3, 4));
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    assert_eq!(decoded.mnemonic(), IcedMnem::Imul);
    assert_eq!(decoded.op0_register(), Register::R8);
    assert_eq!(decoded.op1_register(), Register::RBX);
}

#[test]
fn imul_three_op_rex_b_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    // imul rax, r8, 4 exercises REX.B on the src r/m field.
    let bytes = encode(&imul3(0, 8, 4));
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    assert_eq!(decoded.mnemonic(), IcedMnem::Imul);
    assert_eq!(decoded.op0_register(), Register::RAX);
    assert_eq!(decoded.op1_register(), Register::R8);
}
