//! #1526 (PAS-DEBT-B4-004): `encode_mov` [MemSib, Imm64] arm — true 64-bit
//! immediate store lowered to `movabs r11, imm64` + `mov [mem], r11`.
//!
//! `MOV r/m64, imm32` (SDM Vol 2A: `REX.W C7 /0 id`) is the only single-form
//! immediate-to-memory store the ISA offers, and its immediate is a *32-bit*
//! sign-extended value. When the intended constant exceeds `i32` range the
//! encoder must synthesise the store by staging the constant through a
//! caller-saved GPR: `movabs r11, imm64` (`REX.W+B B8+3 io`, 10 bytes) then
//! `mov [mem], r11` (`REX.W+R 89 /r`, size depends on addressing).
//!
//! The scratch is R11 by convention across the emitter (documented in
//! `emit_store_record.rs`), matching the diagnostic string the encoder used to
//! return before this arm existed. These tests pin the two-instruction byte
//! sequence for both the `[base + disp]` and `[base + index*scale + disp]`
//! forms, plus retention of the compact `C7 /0 id` form when the immediate
//! still fits `i32`.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ── True imm64 into [rbp + 8] ────────────────────────────────────────────────
//
// `mov qword ptr [rbp+8], 0xDEADBEEFCAFEBABE` must lower to:
//   movabs r11, 0xDEADBEEFCAFEBABE   → 49 BB BE BA FE CA EF BE AD DE   (10 bytes)
//   mov    [rbp+8], r11              → 4C 89 5D 08                     ( 4 bytes)
// Total: 14 bytes. The imm64's 8 little-endian bytes must appear intact — a
// truncated 4-byte tail would silently zero-extend to 0x00000000CAFEBABE.
#[test]
fn mov_mem_base_disp_imm64_lowers_to_movabs_r11_plus_store_1526() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 8 },
            Operand::Imm64(0xDEAD_BEEF_CAFE_BABE_u64 as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [rbp+8], imm64");

    assert_eq!(
        buf.as_slice(),
        &[
            // movabs r11, 0xDEADBEEFCAFEBABE
            0x49, 0xBB, 0xBE, 0xBA, 0xFE, 0xCA, 0xEF, 0xBE, 0xAD, 0xDE,
            // mov qword ptr [rbp+8], r11
            0x4C, 0x89, 0x5D, 0x08,
        ],
        "true-imm64 store must emit movabs+store, not a truncated C7 /0 id",
    );
    assert_eq!(buf.as_slice().len(), 14);
}

// ── True imm64 into [rax + rcx*8 + 0x100] ────────────────────────────────────
//
// SIB-indexed form; still routes through the same r11 staging. The store's
// ModR/M+SIB+disp32 shape is 8 bytes, giving 10 + 8 = 18 total.
#[test]
fn mov_mem_sib_disp_imm64_lowers_to_movabs_r11_plus_store_1526() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib {
                base: RegId(0),
                index: Some(RegId(1)),
                scale: Scale::X8,
                disp: 0x100,
            },
            Operand::Imm64(0x1122_3344_5566_7788_u64 as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [rax + rcx*8 + 0x100], imm64");

    assert_eq!(
        buf.as_slice(),
        &[
            // movabs r11, 0x1122334455667788
            0x49, 0xBB, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11,
            // mov qword ptr [rax + rcx*8 + 0x100], r11
            // REX.W+R (4C), 89, ModR/M mod=10 reg=011 rm=100 (SIB) = 9C,
            // SIB scale=11 index=001 base=000 = C8, disp32 = 00 01 00 00.
            0x4C, 0x89, 0x9C, 0xC8, 0x00, 0x01, 0x00, 0x00,
        ],
        "SIB-indexed true-imm64 store must emit movabs+store with correct SIB byte",
    );
    assert_eq!(buf.as_slice().len(), 18);
}

// ── Compact form retained when imm fits i32 (regression guard) ───────────────
//
// The two-instruction lowering must not steal the sign-extended-i32 fast path.
// `mov qword ptr [rdi+0x10], 0` still emits the 8-byte `48 C7 47 10 00 00 00 00`
// form — the sole encoding the record-store recipe relied on before #1526.
#[test]
fn mov_mem_base_disp_imm_fits_i32_keeps_compact_form_1526() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0x10 },
            Operand::Imm64(0),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [rdi+0x10], 0");

    // REX.W (48), C7 /0 (opcode + ModR/M reg=000), ModR/M mod=01 rm=111 disp8,
    // disp8=0x10, imm32=0.
    assert_eq!(
        buf.as_slice(),
        &[0x48, 0xC7, 0x47, 0x10, 0x00, 0x00, 0x00, 0x00],
        "zero literal must still use compact C7 /0 id form (8 bytes)",
    );
    assert_eq!(buf.as_slice().len(), 8);
}

// ── estimated_bytes agrees with encode_instruction (drift guard) ─────────────
//
// The elaborator's `emit_inst` bumps `estimated_offset` by
// `paideia_as_encoder::estimated_bytes(&inst)`. If that ever drifted from the
// real encoder output the record-store recipe would relayout `.text` past
// its labels. Cross-check the true-imm64 path.
#[test]
fn estimated_bytes_matches_encoded_length_true_imm64_1526() {
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 8 },
            Operand::Imm64(0xDEAD_BEEF_CAFE_BABE_u64 as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).unwrap();
    assert_eq!(
        paideia_as_encoder::estimated_bytes(&inst) as usize,
        buf.as_slice().len(),
        "estimated_bytes must equal encode_instruction output length",
    );
}
