//! Byte-exact tests for `movzx`/`movsx` reg-to-reg — issue #1329.
//!
//! `Mnemonic::Movzx`/`Movsx` and their encoders (`encode_movzx`/
//! `encode_movsx` in `crates/paideia-as-encoder/src/encode_instruction.rs`)
//! have existed since Phase 13 m6-001 for field-access lowering, but no
//! `MNEMONIC_TABLE` row in `crates/paideia-as-elaborator/src/unsafe_walker.rs`
//! ever wired the canonical `movzx`/`movsx` spellings into the unsafe-block
//! parser. Pre-fix, `movzx rax, al;` in a `.pdx` unsafe block surfaced as
//! `U1605 unknown mnemonic: movzx` — the exact shape mkfs-pdxb's
//! decimal-parse loop (`src/tools/mkfs-pdxb/main.pdx`, paideia-os #1861)
//! hit. This test exercises the encoder directly with the `Instruction`
//! shape the elaborator fix now constructs (register-name-derived
//! `EncodingHint.operand_size`), so a regression in either the resolver
//! wiring or the width recovery surfaces as a byte diff here.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::instruction::{EncodingHint, Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::InstrMode;
use smallvec::smallvec;

fn encode(mnemonic: Mnemonic, dst: u8, src: u8, operand_size: u8) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic,
        operands: smallvec![Operand::Reg(RegId(dst)), Operand::Reg(RegId(src))],
        encoding_hint: Some(EncodingHint {
            opcode: 0,
            operand_size,
        }),
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed");
    buf.bytes
}

/// `movzx rax, al` — the exact mkfs-pdxb repro shape (#1861/#1329).
/// `REX.W 0F B6 /r` with mod=11, reg=rax(0), rm=al-as-RegId(0).
#[test]
fn movzx_rax_al_emits_48_0f_b6_c0() {
    assert_eq!(encode(Mnemonic::Movzx, 0, 0, 1), vec![0x48, 0x0F, 0xB6, 0xC0]);
}

/// `movzx rax, ax` — 2-byte source selects opcode 0xB7 instead of 0xB6.
#[test]
fn movzx_rax_ax_emits_48_0f_b7_c0() {
    assert_eq!(encode(Mnemonic::Movzx, 0, 0, 2), vec![0x48, 0x0F, 0xB7, 0xC0]);
}

/// `movzx r10, r8b` — REX.R (dst=r10) and REX.B (src=r8) both set.
/// rex = 0100_1_0_0_1 = 0x4D; modrm = 0xC0 | (2<<3) | 0 = 0xD0.
#[test]
fn movzx_r10_r8b_emits_4d_0f_b6_d0() {
    assert_eq!(
        encode(Mnemonic::Movzx, 10, 8, 1),
        vec![0x4D, 0x0F, 0xB6, 0xD0]
    );
}

/// `movsx rax, al` — `REX.W 0F BE /r` for a 1-byte source.
#[test]
fn movsx_rax_al_emits_48_0f_be_c0() {
    assert_eq!(encode(Mnemonic::Movsx, 0, 0, 1), vec![0x48, 0x0F, 0xBE, 0xC0]);
}

/// `movsx rax, ax` — `REX.W 0F BF /r` for a 2-byte source.
#[test]
fn movsx_rax_ax_emits_48_0f_bf_c0() {
    assert_eq!(encode(Mnemonic::Movsx, 0, 0, 2), vec![0x48, 0x0F, 0xBF, 0xC0]);
}

/// `movsx rax, eax` (MOVSXD) — `REX.W 63 /r` for a 4-byte source.
#[test]
fn movsx_rax_eax_emits_48_63_c0() {
    assert_eq!(encode(Mnemonic::Movsx, 0, 0, 4), vec![0x48, 0x63, 0xC0]);
}

/// Round-trip: `movzx rax, al` decodes back to Movzx through iced-x86.
#[test]
fn movzx_rax_al_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let bytes = encode(Mnemonic::Movzx, 0, 0, 1);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Movzx);
}

/// Round-trip: `movsx rax, eax` decodes back to Movsxd through iced-x86.
#[test]
fn movsx_rax_eax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let bytes = encode(Mnemonic::Movsx, 0, 0, 4);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Movsxd);
}
