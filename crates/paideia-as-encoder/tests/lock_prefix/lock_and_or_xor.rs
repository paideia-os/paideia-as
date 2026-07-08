//! Integration tests for lock and/or/xor encoding (PA-R16-006 #972).
//!
//! Tests verify that LOCK AND, LOCK OR, and LOCK XOR (lock-prefixed bitwise)
//! instructions are correctly encoded with register operands:
//! - lock and/or/xor [mem], r64
//! - Base+disp form only (no index*scale).
//! - Encoding: F0 REX.W 21/09/31 /r [ModR/M] [disp]
//!
//! Suite A: Byte-exact encoding validation (9 test vectors).
//! Suite B: iced-x86 round-trip validation (3 test vectors).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (lock and) =====

#[test]
fn lock_and_q_mem_rax_rcx_baseline() {
    // lock and [rax], rcx
    // LOCK (F0) + REX.W (48) + 21 + ModR/M (08 = 00 001 000, rcx in reg field)
    // Expected: F0 48 21 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAnd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(1)),  // rcx
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock and [rax], rcx");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x21, 0x08], "lock and [rax], rcx");
}

#[test]
fn lock_and_q_mem_r15_disp8_r10() {
    // lock and [r15+8], r10
    // r15 is register id 15 (high bit set, requires REX.B)
    // r10 is register id 10 (high bit set, requires REX.R)
    // LOCK (F0) + REX.W|R|B (4D) + 21 + ModR/M with disp8 (57 = 01 010 111, /2 in bits 3-5) + disp8 (08)
    // Expected: F0 4D 21 57 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAnd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 8 },
            Operand::Reg(RegId(10)),  // r10
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock and [r15+8], r10");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x4D, 0x21, 0x57, 0x08],
        "lock and [r15+8], r10"
    );
}

#[test]
fn lock_and_q_mem_rsp_rax_sib() {
    // lock and [rsp], rax
    // rsp is register id 4 (SIB escape: base_low == 4)
    // LOCK (F0) + REX.W (48) + 21 + ModR/M with SIB (04 = 00 000 100) + SIB (24 = 00 100 100)
    // Expected: F0 48 21 04 24
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAnd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // rax
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock and [rsp], rax");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x21, 0x04, 0x24],
        "lock and [rsp], rax"
    );
}

// ===== Suite A: Byte-Exact Encoding (lock or) =====

#[test]
fn lock_or_q_mem_rax_rcx_baseline() {
    // lock or [rax], rcx
    // LOCK (F0) + REX.W (48) + 09 + ModR/M (08 = 00 001 000, rcx in reg field)
    // Expected: F0 48 09 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockOr { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(1)),  // rcx
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock or [rax], rcx");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x09, 0x08], "lock or [rax], rcx");
}

#[test]
fn lock_or_q_mem_r15_disp8_r10() {
    // lock or [r15+8], r10
    // r15 is register id 15 (high bit set, requires REX.B)
    // r10 is register id 10 (high bit set, requires REX.R)
    // LOCK (F0) + REX.W|R|B (4D) + 09 + ModR/M with disp8 (57 = 01 010 111, /2 in bits 3-5) + disp8 (08)
    // Expected: F0 4D 09 57 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockOr { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 8 },
            Operand::Reg(RegId(10)),  // r10
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock or [r15+8], r10");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x4D, 0x09, 0x57, 0x08],
        "lock or [r15+8], r10"
    );
}

#[test]
fn lock_or_q_mem_rsp_rax_sib() {
    // lock or [rsp], rax
    // rsp is register id 4 (SIB escape: base_low == 4)
    // LOCK (F0) + REX.W (48) + 09 + ModR/M with SIB (04 = 00 000 100) + SIB (24 = 00 100 100)
    // Expected: F0 48 09 04 24
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockOr { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // rax
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock or [rsp], rax");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x09, 0x04, 0x24],
        "lock or [rsp], rax"
    );
}

// ===== Suite A: Byte-Exact Encoding (lock xor) =====

#[test]
fn lock_xor_q_mem_rax_rcx_baseline() {
    // lock xor [rax], rcx
    // LOCK (F0) + REX.W (48) + 31 + ModR/M (08 = 00 001 000, rcx in reg field)
    // Expected: F0 48 31 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXor { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(1)),  // rcx
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xor [rax], rcx");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x31, 0x08], "lock xor [rax], rcx");
}

#[test]
fn lock_xor_q_mem_r15_disp8_r10() {
    // lock xor [r15+8], r10
    // r15 is register id 15 (high bit set, requires REX.B)
    // r10 is register id 10 (high bit set, requires REX.R)
    // LOCK (F0) + REX.W|R|B (4D) + 31 + ModR/M with disp8 (57 = 01 010 111, /2 in bits 3-5) + disp8 (08)
    // Expected: F0 4D 31 57 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXor { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 8 },
            Operand::Reg(RegId(10)),  // r10
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xor [r15+8], r10");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x4D, 0x31, 0x57, 0x08],
        "lock xor [r15+8], r10"
    );
}

#[test]
fn lock_xor_q_mem_rsp_rax_sib() {
    // lock xor [rsp], rax
    // rsp is register id 4 (SIB escape: base_low == 4)
    // LOCK (F0) + REX.W (48) + 31 + ModR/M with SIB (04 = 00 000 100) + SIB (24 = 00 100 100)
    // Expected: F0 48 31 04 24
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXor { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // rax
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xor [rsp], rax");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x31, 0x04, 0x24],
        "lock xor [rsp], rax"
    );
}

// ===== Suite B: iced-x86 Round-Trip Validation =====

#[test]
fn lock_and_iced_roundtrip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAnd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(1)),  // rcx
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock and [rax], rcx");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::And);
    assert!(instr.has_lock_prefix(), "lock and should have LOCK prefix");
}

#[test]
fn lock_or_iced_roundtrip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockOr { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(1)),  // rcx
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock or [rax], rcx");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Or);
    assert!(instr.has_lock_prefix(), "lock or should have LOCK prefix");
}

#[test]
fn lock_xor_iced_roundtrip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXor { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(1)),  // rcx
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xor [rax], rcx");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let instr = decoder.decode();
    assert_eq!(instr.mnemonic(), IcedMnem::Xor);
    assert!(instr.has_lock_prefix(), "lock xor should have LOCK prefix");
}
