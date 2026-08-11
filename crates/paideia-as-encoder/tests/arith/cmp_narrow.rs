//! Byte-exact tests for `Mnemonic::CmpSized` narrow encodings — #1254.
//!
//! Completes the CmpSized family that #1248 opened for the W8 case. Locks
//! the W8/W16/W32 reg-imm, reg-reg, and [mem]-reg encodings so the
//! narrow-cmp path cannot silently regress to the 64-bit REX.W form and
//! reintroduce the miscompile #1248 was filed for.
//!
//! Reference table (from issue #1254):
//!   cmp al, 0x63           → 3C 63           (AL short form)
//!   cmp cl, 0x7F           → 80 F9 7F        (ModR/M form)
//!   cmp r8b, 0x01          → 41 80 F8 01     (REX.B for r8b)
//!   cmp ax, 0x1234         → 66 3D 34 12     (AX short form)
//!   cmp eax, 0x11223344    → 3D 44 33 22 11  (EAX short form)
//!   cmp edx, 0x40          → 83 FA 40        (imm8-sxt short form)
//!   cmp al, cl             → 38 C8
//!   cmp r10b, r11b         → 45 38 DA
//!   cmp ax, cx             → 66 39 C8
//!   cmp eax, ecx           → 39 C8
//!   cmp byte ptr [rdi], al       → 38 07
//!   cmp dword ptr [rbp-8], eax   → 39 45 F8

use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

use crate::common;

fn cmp_reg_imm(width: IntWidth, reg: u8, imm: i64) -> Instruction {
    Instruction {
        mnemonic: Mnemonic::CmpSized { width },
        operands: smallvec![Operand::Reg(RegId(reg)), Operand::Imm64(imm)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    }
}

fn cmp_reg_reg(width: IntWidth, dst: u8, src: u8) -> Instruction {
    Instruction {
        mnemonic: Mnemonic::CmpSized { width },
        operands: smallvec![Operand::Reg(RegId(dst)), Operand::Reg(RegId(src))],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    }
}

fn cmp_mem_reg(width: IntWidth, base: u8, disp: i32, src: u8) -> Instruction {
    Instruction {
        mnemonic: Mnemonic::CmpSized { width },
        operands: smallvec![
            Operand::MemSib {
                base: RegId(base),
                index: None,
                scale: Scale::X1,
                disp,
            },
            Operand::Reg(RegId(src)),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    }
}

// ── W8 reg-imm ──────────────────────────────────────────────────────

#[test]
fn cmp_al_0x63_emits_al_short_form() {
    // cmp al, 0x63 → 3C 63
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W8, 0, 0x63));
    assert_eq!(bytes, vec![0x3C, 0x63]);
}

#[test]
fn cmp_cl_0x7f_emits_modrm_form() {
    // cmp cl, 0x7F → 80 F9 7F
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W8, 1, 0x7F));
    assert_eq!(bytes, vec![0x80, 0xF9, 0x7F]);
}

#[test]
fn cmp_r8b_0x01_emits_rex_b() {
    // cmp r8b, 0x01 → 41 80 F8 01
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W8, 8, 0x01));
    assert_eq!(bytes, vec![0x41, 0x80, 0xF8, 0x01]);
}

// ── W16 reg-imm ─────────────────────────────────────────────────────

#[test]
fn cmp_ax_0x1234_emits_ax_short_form() {
    // cmp ax, 0x1234 → 66 3D 34 12 (imm too large for imm8; uses ax short form)
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W16, 0, 0x1234));
    assert_eq!(bytes, vec![0x66, 0x3D, 0x34, 0x12]);
}

#[test]
fn cmp_cx_0x1234_emits_general_form() {
    // cmp cx, 0x1234 → 66 81 F9 34 12
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W16, 1, 0x1234));
    assert_eq!(bytes, vec![0x66, 0x81, 0xF9, 0x34, 0x12]);
}

#[test]
fn cmp_ax_0x40_uses_imm8_sxt_short_form() {
    // Small immediate fits in i8 → 66 83 F8 40 (3-byte form beats 4-byte ax short).
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W16, 0, 0x40));
    assert_eq!(bytes, vec![0x66, 0x83, 0xF8, 0x40]);
}

#[test]
fn cmp_r10w_0x1234_emits_rex_b_prefix() {
    // r10w is not a valid asm mnemonic but the encoder is width-driven — RegId(10)
    // + W16 → 66 41 81 FA 34 12.
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W16, 10, 0x1234));
    assert_eq!(bytes, vec![0x66, 0x41, 0x81, 0xFA, 0x34, 0x12]);
}

// ── W32 reg-imm ─────────────────────────────────────────────────────

#[test]
fn cmp_eax_0x11223344_emits_eax_short_form() {
    // cmp eax, 0x11223344 → 3D 44 33 22 11
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W32, 0, 0x1122_3344));
    assert_eq!(bytes, vec![0x3D, 0x44, 0x33, 0x22, 0x11]);
}

#[test]
fn cmp_ecx_0x11223344_emits_general_form() {
    // cmp ecx, 0x11223344 → 81 F9 44 33 22 11
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W32, 1, 0x1122_3344));
    assert_eq!(bytes, vec![0x81, 0xF9, 0x44, 0x33, 0x22, 0x11]);
}

#[test]
fn cmp_edx_0x40_uses_imm8_sxt_short_form() {
    // cmp edx, 0x40 → 83 FA 40 (imm fits i8)
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W32, 2, 0x40));
    assert_eq!(bytes, vec![0x83, 0xFA, 0x40]);
}

#[test]
fn cmp_r10d_0x1234_emits_rex_b_prefix() {
    // Extended r10d + W32, imm too large for i8 → 41 81 FA 34 12 00 00
    let bytes = common::encode_bytes(&cmp_reg_imm(IntWidth::W32, 10, 0x1234));
    assert_eq!(bytes, vec![0x41, 0x81, 0xFA, 0x34, 0x12, 0x00, 0x00]);
}

// ── W8 reg-reg ──────────────────────────────────────────────────────

#[test]
fn cmp_al_cl_emits_38_c8() {
    // cmp al, cl → 38 C8
    let bytes = common::encode_bytes(&cmp_reg_reg(IntWidth::W8, 0, 1));
    assert_eq!(bytes, vec![0x38, 0xC8]);
}

#[test]
fn cmp_r10b_r11b_emits_rex_rb_prefix() {
    // cmp r10b, r11b → 45 38 DA (REX.R | REX.B, src=r11, dst=r10)
    let bytes = common::encode_bytes(&cmp_reg_reg(IntWidth::W8, 10, 11));
    assert_eq!(bytes, vec![0x45, 0x38, 0xDA]);
}

// ── W16 reg-reg ─────────────────────────────────────────────────────

#[test]
fn cmp_ax_cx_emits_66_39_c8() {
    // cmp ax, cx → 66 39 C8
    let bytes = common::encode_bytes(&cmp_reg_reg(IntWidth::W16, 0, 1));
    assert_eq!(bytes, vec![0x66, 0x39, 0xC8]);
}

// ── W32 reg-reg ─────────────────────────────────────────────────────

#[test]
fn cmp_eax_ecx_emits_39_c8() {
    // cmp eax, ecx → 39 C8
    let bytes = common::encode_bytes(&cmp_reg_reg(IntWidth::W32, 0, 1));
    assert_eq!(bytes, vec![0x39, 0xC8]);
}

// ── W8 mem-reg (store shape) ────────────────────────────────────────

#[test]
fn cmp_mem_rdi_al_emits_38_07() {
    // cmp byte ptr [rdi], al → 38 07
    let bytes = common::encode_bytes(&cmp_mem_reg(IntWidth::W8, 7, 0, 0));
    assert_eq!(bytes, vec![0x38, 0x07]);
}

// ── W32 mem-reg (store shape) ───────────────────────────────────────

#[test]
fn cmp_mem_rbp_minus_8_eax_emits_39_45_f8() {
    // cmp dword ptr [rbp - 8], eax → 39 45 F8
    let bytes = common::encode_bytes(&cmp_mem_reg(IntWidth::W32, 5, -8, 0));
    assert_eq!(bytes, vec![0x39, 0x45, 0xF8]);
}

// ── Regression: existing 64-bit cmp untouched ───────────────────────

#[test]
fn cmp_generic_rax_zero_still_emits_64bit_form() {
    // Guard: the walker only retargets narrow widths; plain Cmp with 64-bit reg
    // is unchanged. cmp rax, 0 → 48 83 F8 00.
    let inst = Instruction {
        mnemonic: Mnemonic::Cmp,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let bytes = common::encode_bytes(&inst);
    assert_eq!(bytes, vec![0x48, 0x83, 0xF8, 0x00]);
}

#[test]
fn cmp_sized_w64_delegates_to_generic_cmp() {
    // CmpSized{W64} delegates to the generic 64-bit cmp encoder; behavior
    // must be byte-identical to Mnemonic::Cmp with the same operands.
    let sized = common::encode_bytes(&cmp_reg_imm(IntWidth::W64, 0, 0));
    assert_eq!(sized, vec![0x48, 0x83, 0xF8, 0x00]);
}
