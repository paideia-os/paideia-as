//! Byte-exact tests for `invpcid r64, m128` — v0.21-009-followup (#1297).
//!
//! Reference encoding (Intel SDM Vol 2A INVPCID): `66 0F 38 82 /r`.
//! The register operand holds the INVPCID type (0/1/2/3 in low 2 bits of r64):
//!   0 = individual-address       (TlbOps::invpcid_single)
//!   1 = single-context            (TlbOps::invpcid_all_nonglobal)
//!   2 = all-context including globals
//!   3 = all-context excluding globals
//! Type discrimination is a runtime property of the register — the encoding
//! is identical regardless of type. What we lock here is the byte-exact
//! encoding across register/memory shape variations so that a future
//! encoder edit cannot silently miscompile INVPCID.

use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

use crate::common;

fn invpcid_inst(reg: u8, base: u8, disp: i32) -> Instruction {
    Instruction {
        mnemonic: Mnemonic::Invpcid,
        operands: smallvec![
            Operand::Reg(RegId(reg)),
            Operand::MemSib {
                base: RegId(base),
                index: None,
                scale: Scale::X1,
                disp,
            },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    }
}

#[test]
fn invpcid_rax_mem_rbx_emits_66_0f_38_82_03() {
    // Reference case from #1297 issue text: `invpcid rax, [rbx]` = 66 0F 38 82 03.
    // rax = RegId(0), rbx = RegId(3), no REX (both low-8), no SIB (rbx isn't RSP), disp=0.
    // Type 0 → individual-address invalidation.
    let bytes = common::encode_bytes(&invpcid_inst(0, 3, 0));
    assert_eq!(bytes, vec![0x66, 0x0F, 0x38, 0x82, 0x03]);
}

#[test]
fn invpcid_rax_mem_rsp_emits_sib_form() {
    // `invpcid rax, [rsp]` requires SIB escape because rsp = 100b in ModR/M.rm.
    // 66 0F 38 82 04 24 (04 = ModR/M mod=00 reg=000 rm=100 SIB; 24 = SIB scale=00 index=100 base=100).
    // Ties to type 0 (individual-address); paideia-os R18-M5 shootdown builds
    // the descriptor on the stack via `sub rsp, 16 ; ... ; invpcid rax, [rsp]`.
    let bytes = common::encode_bytes(&invpcid_inst(0, 4, 0));
    assert_eq!(bytes, vec![0x66, 0x0F, 0x38, 0x82, 0x04, 0x24]);
}

#[test]
fn invpcid_type1_rcx_mem_rbx_shares_encoding_with_rax() {
    // Type discrimination is runtime — the register content selects type 0/1/2/3.
    // Register CHOICE (rax vs rcx) changes the reg field bits. rcx = RegId(1),
    // reg field = 001b. `invpcid rcx, [rbx]` = 66 0F 38 82 0B (0B = 00_001_011).
    // Type 1 (single-context) is what TlbOps::invpcid_all_nonglobal uses.
    let bytes = common::encode_bytes(&invpcid_inst(1, 3, 0));
    assert_eq!(bytes, vec![0x66, 0x0F, 0x38, 0x82, 0x0B]);
}

#[test]
fn invpcid_type2_rdx_mem_rbx() {
    // Type 2 (all-context including globals) — reg=rdx yields reg field = 010.
    // 66 0F 38 82 13 (13 = 00_010_011).
    let bytes = common::encode_bytes(&invpcid_inst(2, 3, 0));
    assert_eq!(bytes, vec![0x66, 0x0F, 0x38, 0x82, 0x13]);
}

#[test]
fn invpcid_type3_rbx_mem_rax() {
    // Type 3 (all-context excluding globals) — reg=rbx yields reg field = 011.
    // Also flips base to rax: rm field = 000. 66 0F 38 82 18.
    let bytes = common::encode_bytes(&invpcid_inst(3, 0, 0));
    assert_eq!(bytes, vec![0x66, 0x0F, 0x38, 0x82, 0x18]);
}

#[test]
fn invpcid_r10_mem_rbx_emits_rex_r() {
    // Extended reg r10 → REX.R = 1. reg field = r10 & 7 = 010.
    // 66 44 0F 38 82 13 (44 = REX with R=1, base rbx=011 in rm).
    let bytes = common::encode_bytes(&invpcid_inst(10, 3, 0));
    assert_eq!(bytes, vec![0x66, 0x44, 0x0F, 0x38, 0x82, 0x13]);
}

#[test]
fn invpcid_rax_mem_r11_emits_rex_b() {
    // Extended base r11 → REX.B = 1. rm field = r11 & 7 = 011.
    // 66 41 0F 38 82 03 (41 = REX with B=1).
    let bytes = common::encode_bytes(&invpcid_inst(0, 11, 0));
    assert_eq!(bytes, vec![0x66, 0x41, 0x0F, 0x38, 0x82, 0x03]);
}

#[test]
fn invpcid_r10_mem_r11_emits_rex_rb() {
    // Both extended → REX.R | REX.B (0x45). 66 45 0F 38 82 13.
    let bytes = common::encode_bytes(&invpcid_inst(10, 11, 0));
    assert_eq!(bytes, vec![0x66, 0x45, 0x0F, 0x38, 0x82, 0x13]);
}

#[test]
fn invpcid_rax_mem_rbx_disp8() {
    // 8-bit displacement path: mod=01, disp byte.
    // `invpcid rax, [rbx + 8]` = 66 0F 38 82 43 08 (43 = 01_000_011, 08 = disp).
    let bytes = common::encode_bytes(&invpcid_inst(0, 3, 8));
    assert_eq!(bytes, vec![0x66, 0x0F, 0x38, 0x82, 0x43, 0x08]);
}

#[test]
fn invpcid_rax_mem_rbp_zero_disp_uses_disp8_escape() {
    // rbp base with disp=0 must be encoded as disp8=0 (mod=01) because
    // mod=00 rm=101 means [RIP+disp32] instead of [rbp]. Shared BP-escape
    // path in emit_mem_base_disp. 66 0F 38 82 45 00.
    let bytes = common::encode_bytes(&invpcid_inst(0, 5, 0));
    assert_eq!(bytes, vec![0x66, 0x0F, 0x38, 0x82, 0x45, 0x00]);
}
