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
        emission_order: 0,
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
        emission_order: 0,
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
        emission_order: 0,
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
        emission_order: 0,
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
        emission_order: 0,
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
        emission_order: 0,
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
        emission_order: 0,
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
        emission_order: 0,
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
        emission_order: 0,
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
        emission_order: 0,
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
        emission_order: 0,
};
    let bytes = encode_one(inst);
    assert_eq!(bytes, &[0x64, 0x48, 0x8B, 0x00]);

    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
    assert_eq!(decoded.segment_prefix(), IcedReg::FS);
}

// ── SIB-less-base absolute-disp32 form under GS override ────────────────────
//
// The disp32-only / "no base" SIB form (ModR/M mod=00 rm=100, SIB base=101,
// disp32) is what `mov rax, gs:[0x20]` compiles to. It IS a SIB byte (a SIB
// byte is architecturally required whenever ModR/M rm=100 in 64-bit mode),
// but the SIB base and index fields are both encoded as "none" so no register
// participates in the effective address — a disp32 immediate is the sole
// address. PA-R14-001 (#926) audits this path for PerCpuOps at the encoder
// level; the W32 form (mov eax, gs:[0x1000]) is validated in
// mov/mov_mem_abs_disp32.rs Suite D.
//
// The `mov rax, gs:[0x20]` W64 form below is the p0 v0.21-005 (#1281)
// acceptance-criterion witness: PerCpuOps { read_u64 } lowering uses this
// idiom whenever the CB offset is a compile-time literal that fits disp32.

#[test]
fn mov_rax_gs_disp32_0x20_w64_read_pa_r14_001_witness() {
    // mov rax, gs:[0x20]
    // Segment prefix (65) + REX.W (48) + opcode (8B) + ModR/M (04) + SIB (25)
    //   + disp32 (20 00 00 00)
    // Expected: 65 48 8B 04 25 20 00 00 00 (9 bytes)
    //
    // This is the PA-R14-001 (#926) / v0.21-005 (#1281) acceptance-criterion
    // witness for `mov rax, gs:[0x20]` — the natural W64 per-CPU CB read at
    // offset 0x20, exercised through the plain `Mnemonic::Mov` path (which
    // defaults to W64 when the operand pair is `[Reg64, MemDisp]`).
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)), // rax
            Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemDisp { disp: 0x20 }),
            },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let bytes = encode_one(inst);
    assert_eq!(
        bytes,
        &[0x65, 0x48, 0x8B, 0x04, 0x25, 0x20, 0x00, 0x00, 0x00],
        "mov rax, gs:[0x20] W64 SIB-no-base form"
    );
}

#[test]
fn mov_gs_disp32_0x20_rax_w64_store_pa_r14_001_witness() {
    // mov gs:[0x20], rax — store direction of the SIB-no-base form.
    // Expected: 65 48 89 04 25 20 00 00 00 (9 bytes; 89 = MR store opcode)
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemDisp { disp: 0x20 }),
            },
            Operand::Reg(RegId(0)), // rax
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let bytes = encode_one(inst);
    assert_eq!(
        bytes,
        &[0x65, 0x48, 0x89, 0x04, 0x25, 0x20, 0x00, 0x00, 0x00],
        "mov gs:[0x20], rax W64 SIB-no-base store form"
    );
}

#[test]
fn mov_rax_gs_disp32_0x20_iced_round_trip_confirms_gs_and_disp() {
    // Round-trip the AC witness through iced-x86 to confirm not just byte
    // equality but that the decoded semantic (GS segment override, absolute
    // displacement 0x20, W64 mov) matches Intel's authoritative decoder.
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register as IcedReg};

    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)), // rax
            Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemDisp { disp: 0x20 }),
            },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let bytes = encode_one(inst);
    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
    assert_eq!(decoded.segment_prefix(), IcedReg::GS);
    assert_eq!(decoded.op0_register(), IcedReg::RAX);
    // op1 is a memory operand at absolute disp32 = 0x20; no base/index.
    assert_eq!(decoded.memory_displacement64(), 0x20);
    assert_eq!(decoded.memory_base(), IcedReg::None);
    assert_eq!(decoded.memory_index(), IcedReg::None);
}
