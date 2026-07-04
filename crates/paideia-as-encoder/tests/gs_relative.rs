//! PA-R13-002 (#915): GS/FS-relative memory operand tests.
//!
//! Tests verify that segment prefix (0x64/0x65) is correctly emitted before
//! the memory operand, with byte-exact validation against x86_64 encoding.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId, SegPrefix, Scale};
use smallvec::smallvec;

fn encode_one(inst: Instruction) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::default();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).unwrap();
    buf.bytes.clone()
}

#[test]
fn mov_rax_gs_rax_disp0() {
    // mov rax, [gs:rax + 0]
    // Expected: 65 48 8B 00 (4 bytes) - [ModR/M: 00-000-000]
    let inner = Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 };
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSeg { seg: SegPrefix::Gs, inner: Box::new(inner) },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x65, 0x48, 0x8B, 0x00]);
}

#[test]
fn mov_r8_gs_r8_disp0() {
    // mov r8, [gs:r8 + 0]
    // Expected: 65 4D 8B 00 (4 bytes, REX.W|REX.R|REX.B for r8 dest + r8 base)
    let inner = Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 };
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(8)),
            Operand::MemSeg { seg: SegPrefix::Gs, inner: Box::new(inner) },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x65, 0x4D, 0x8B, 0x00]);
}

#[test]
fn mov_rax_gs_rsi_disp8() {
    // mov rax, [gs:rsi + 8]
    // Expected: 65 48 8B 46 08 (5 bytes)
    let inner = Operand::MemSib { base: RegId(6), index: None, scale: Scale::X1, disp: 8 };
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSeg { seg: SegPrefix::Gs, inner: Box::new(inner) },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x65, 0x48, 0x8B, 0x46, 0x08]);
}

#[test]
fn mov_rax_fs_rax_disp0() {
    // mov rax, [fs:rax + 0]
    // Expected: 64 48 8B 00 (4 bytes, 0x64 for fs)
    let inner = Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 };
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSeg { seg: SegPrefix::Fs, inner: Box::new(inner) },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x64, 0x48, 0x8B, 0x00]);
}

#[test]
fn mov_rax_gs_rsi_index_rdi_scale4_disp8() {
    // mov rax, [gs:rsi + rdi*4 + 16] (disp8 fit)
    // Expected: 65 48 8B 44 BE 10 (6 bytes)
    let inner = Operand::MemSib { base: RegId(6), index: Some(RegId(7)), scale: Scale::X4, disp: 16 };
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSeg { seg: SegPrefix::Gs, inner: Box::new(inner) },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x65, 0x48, 0x8B, 0x44, 0xBE, 0x10]);
}

#[test]
fn mov_rbx_gs_rax_index_rcx_scale2_disp_neg8() {
    // mov rbx, [gs:rax + rcx*2 - 8]
    // Expected: 65 48 8B 5C 48 F8 (6 bytes)
    let inner = Operand::MemSib { base: RegId(0), index: Some(RegId(1)), scale: Scale::X2, disp: -8_i32 };
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(3)),
            Operand::MemSeg { seg: SegPrefix::Gs, inner: Box::new(inner) },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x65, 0x48, 0x8B, 0x5C, 0x48, 0xF8]);
}




#[test]
fn lea_rax_gs_rsi_disp8() {
    // lea rax, [gs:rsi + 8]
    // Expected: 65 48 8D 46 08 (5 bytes)
    let inner = Operand::MemSib { base: RegId(6), index: None, scale: Scale::X1, disp: 8 };
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSeg { seg: SegPrefix::Gs, inner: Box::new(inner) },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x65, 0x48, 0x8D, 0x46, 0x08]);
}

#[test]
fn mov_r10_gs_r9_disp8_rex_r_and_b() {
    // mov r10, [gs:r9 + 8] — exercises REX.R (dest r10) and REX.B (base r9)
    // together, distinct from the earlier tests which only ever set one of
    // REX.R or REX.B in isolation.
    // Expected: 65 4D 8B 51 08 (5 bytes)
    let inner = Operand::MemSib { base: RegId(9), index: None, scale: Scale::X1, disp: 8 };
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(10)),
            Operand::MemSeg { seg: SegPrefix::Gs, inner: Box::new(inner) },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x65, 0x4D, 0x8B, 0x51, 0x08]);
}

#[test]
fn mov_gs_rdi_disp8_rax_store_form() {
    // mov [gs:rdi + 8], rax — store direction (memory operand as destination).
    // All other tests here are load direction (memory as source); this
    // confirms the segment-prefix pre-pass and its +1 reloc/label shift are
    // direction-agnostic.
    // Expected: 65 48 89 47 08 (5 bytes)
    let inner = Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 };
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSeg { seg: SegPrefix::Gs, inner: Box::new(inner) },
            Operand::Reg(RegId(0)),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x65, 0x48, 0x89, 0x47, 0x08]);
}

#[test]
fn mov_rax_gs_rsi_disp8_round_trips_through_iced_x86_with_gs_segment() {
    // mov rax, [gs:rsi + 8] decoded by iced-x86 must report GS as the
    // effective segment override, not just byte-exact-match the encoding.
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register as IcedReg};

    let inner = Operand::MemSib { base: RegId(6), index: None, scale: Scale::X1, disp: 8 };
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSeg { seg: SegPrefix::Gs, inner: Box::new(inner) },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x65, 0x48, 0x8B, 0x46, 0x08]);

    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
    assert_eq!(decoded.segment_prefix(), IcedReg::GS);
}

#[test]
fn mov_rax_fs_rax_disp0_round_trips_through_iced_x86_with_fs_segment() {
    // mov rax, [fs:rax + 0] — same round-trip check for the FS prefix (0x64),
    // to guard against a Fs/Gs byte swap regression.
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register as IcedReg};

    let inner = Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 };
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSeg { seg: SegPrefix::Fs, inner: Box::new(inner) },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x64, 0x48, 0x8B, 0x00]);

    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
    assert_eq!(decoded.segment_prefix(), IcedReg::FS);
}

// ── Known limitation (documented, not a test) ──────────────────────────────
//
// The disp32-only / "no base" SIB form — needed for `[gs:0]` and
// `[gs:absolute_addr]` (ModR/M mod=00 rm=100, SIB base=101, disp32) — cannot
// be covered here. `Operand::MemSib` requires a `base: RegId` (not
// `Option<RegId>`), and the sibling `Operand::MemDisp { disp }` variant that
// *would* represent a no-base displacement exists in paideia-as-ir but has
// no encoder support for `mov`/`lea` at all (see
// `encode_unsupported_mov_shape_returns_error` in
// crates/paideia-as-encoder/src/encode_instruction.rs, which asserts that
// encoding `mov reg, [MemDisp]` returns `EncodeError::Unsupported`). This is
// a pre-existing gap in the encoder unrelated to segment prefixes — adding
// disp-only support belongs to a follow-up ticket on `MemDisp`/SIB-no-base
// encoding, not PA-R13-002. Once that lands, `[gs:0]`/`[gs:8]` disp-only
// load and store forms should be added here.
