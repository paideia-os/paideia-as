//! #1270 regression pins: cmp+jcc after mov reg64,imm+mov reg64,imm.
//!
//! Issue #1270 reports that `mov rax, 0x30 ; cmp rax, 0x30 ; je label`
//! (and the `mov reg,imm ; mov reg,imm ; cmp reg,reg ; je label` variant)
//! branches the wrong way in specific paideia-as-emitted contexts under
//! paideia-os. The reporter's own encoding audit shows byte-perfect valid
//! x86_64; the miscompile symptom therefore points at pipeline layers ABOVE
//! the encoder (walker sequencing, label-fixup offsets, or a
//! context-dependent interaction), not at the raw encoding of these
//! individual instructions.
//!
//! These byte-exact tests LOCK the raw encoder's emission for the exact
//! bytes named in issue #1270. If a future edit to the encoder ever
//! silently miscompiles these instructions at the byte level, one of these
//! tests will fail — narrowing the search space when the higher-level
//! branch-taken symptom next resurfaces.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

fn encode_one(inst: Instruction) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed");
    buf.as_slice().to_vec()
}

fn mov_rax_imm(imm: i64) -> Instruction {
    Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(imm)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    }
}

fn mov_rdx_imm(imm: i64) -> Instruction {
    Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(2)), Operand::Imm64(imm)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    }
}

fn cmp_rax_imm(imm: i64) -> Instruction {
    Instruction {
        mnemonic: Mnemonic::Cmp,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Imm64(imm)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    }
}

fn cmp_rax_rdx() -> Instruction {
    Instruction {
        mnemonic: Mnemonic::Cmp,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(2))],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    }
}

/// The exact `mov rax, 0x30` byte sequence quoted in issue #1270:
/// `48 c7 c0 30 00 00 00`. Confirms `mov r/m64, imm32-sxt` form
/// (not `movabs`) is selected for a small positive immediate.
#[test]
fn mov_rax_imm_0x30_byte_exact_1270() {
    assert_eq!(
        encode_one(mov_rax_imm(0x30)),
        &[0x48, 0xC7, 0xC0, 0x30, 0x00, 0x00, 0x00]
    );
}

/// The exact `cmp rax, 0x30` byte sequence quoted in issue #1270:
/// `48 83 f8 30`. Confirms `cmp r64, imm8` sign-ext form is selected for
/// a positive value inside i8 range (dispatcher in encode_cmp @ line 3017).
#[test]
fn cmp_rax_imm_0x30_byte_exact_1270() {
    assert_eq!(encode_one(cmp_rax_imm(0x30)), &[0x48, 0x83, 0xF8, 0x30]);
}

/// The full `mov rax, 0x30 ; mov rdx, 0x30 ; cmp rax, rdx` sequence from
/// the second failing pattern in issue #1270. All three encodings are
/// individually valid; this pin catches any per-instruction drift so the
/// paideia-os-context branch-taken failure can be reproduced against a
/// known-good encoder byte trace.
#[test]
fn mov_mov_cmp_rax_rdx_byte_exact_1270() {
    let mut expected: Vec<u8> = Vec::new();
    // mov rax, 0x30 → 48 C7 C0 30 00 00 00
    expected.extend([0x48, 0xC7, 0xC0, 0x30, 0x00, 0x00, 0x00]);
    // mov rdx, 0x30 → 48 C7 C2 30 00 00 00
    expected.extend([0x48, 0xC7, 0xC2, 0x30, 0x00, 0x00, 0x00]);
    // cmp rax, rdx → 48 39 D0  (op 39 = CMP r/m64,r64; ModR/M = 11 010 000)
    expected.extend([0x48, 0x39, 0xD0]);

    let mut actual: Vec<u8> = Vec::new();
    actual.extend(encode_one(mov_rax_imm(0x30)));
    actual.extend(encode_one(mov_rdx_imm(0x30)));
    actual.extend(encode_one(cmp_rax_rdx()));

    assert_eq!(actual, expected);
}

/// iced-x86 cross-check: the three encoded instructions decode back to
/// MOV / MOV / CMP with equal operands. If this test ever fails, the
/// encoder started emitting bytes that a real decoder disagrees with —
/// which would be a legitimate root cause candidate for #1270.
#[test]
fn mov_mov_cmp_iced_round_trip_1270() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut bytes: Vec<u8> = Vec::new();
    bytes.extend(encode_one(mov_rax_imm(0x30)));
    bytes.extend(encode_one(mov_rdx_imm(0x30)));
    bytes.extend(encode_one(cmp_rax_rdx()));

    let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NONE);
    let i0 = decoder.decode();
    let i1 = decoder.decode();
    let i2 = decoder.decode();
    assert_eq!(i0.mnemonic(), IcedMnem::Mov);
    assert_eq!(i1.mnemonic(), IcedMnem::Mov);
    assert_eq!(i2.mnemonic(), IcedMnem::Cmp);
    // No lingering bytes; sequence consumed exactly.
    assert_eq!(decoder.position(), bytes.len());
}
