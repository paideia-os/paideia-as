//! Integration tests for lock bts/btr/btc encoding (PA-R16-002 #968).
//!
//! Tests verify that LOCK BTS, LOCK BTR, and LOCK BTC (lock-prefixed bit test)
//! instructions are correctly encoded with immediate or register operands:
//! - lock bts/btr/btc [mem], imm8/r64
//! - Base+disp form only (no index*scale).
//! - Encoding: F0 REX.W 0F BA/AB/B3/BB [ModR/M] [disp] [imm8]
//!
//! Suite A: Byte-exact encoding validation (12 test vectors).
//! Suite B: iced-x86 round-trip validation (3 test vectors).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (lock bts) =====

#[test]
fn lock_bts_q_mem_rax_imm8_zero_byte_exact() {
    // lock bts [rax], 0
    // LOCK (F0) + REX.W (48) + 0F BA + ModR/M (28) + imm8 (00)
    // Expected: F0 48 0F BA 28 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBts { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(0),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock bts [rax], 0");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xBA, 0x28, 0x00], "lock bts [rax], 0");
}

#[test]
fn lock_bts_q_mem_r15_disp8_imm8_byte_exact() {
    // lock bts [r15+8], 63
    // r15 is register id 15 (high bit set, requires REX.B)
    // LOCK (F0) + REX.W|B (49) + 0F BA + ModR/M with disp8 (6F = 01 101 111, /5 in bits 3-5) + disp8 (08) + imm8 (3F)
    // Expected: F0 49 0F BA 6F 08 3F
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBts { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 8 },
            Operand::Imm64(63),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock bts [r15+8], 63");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x49, 0x0F, 0xBA, 0x6F, 0x08, 0x3F],
        "lock bts [r15+8], 63"
    );
}

// ===== Suite A: Byte-Exact Encoding (lock btr) =====

#[test]
fn lock_btr_q_mem_rax_imm8_byte_exact() {
    // lock btr [rax], 1
    // LOCK (F0) + REX.W (48) + 0F BA + ModR/M (/6 = 30) + imm8 (01)
    // Expected: F0 48 0F BA 30 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBtr { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(1),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock btr [rax], 1");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xBA, 0x30, 0x01], "lock btr [rax], 1");
}

#[test]
fn lock_btr_q_mem_rsp_disp32_imm8_byte_exact() {
    // lock btr [rsp+0x100], 5
    // rsp is register id 4 (SIB escape: base_low == 4)
    // LOCK (F0) + REX.W (48) + 0F BA + ModR/M with SIB (B4 = 10 110 100) + SIB (24) + disp32 LE + imm8
    // Expected: F0 48 0F BA B4 24 00 01 00 00 05
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBtr { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0x100 },
            Operand::Imm64(5),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock btr [rsp+0x100], 5");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x0F, 0xBA, 0xB4, 0x24, 0x00, 0x01, 0x00, 0x00, 0x05],
        "lock btr [rsp+0x100], 5"
    );
}

// ===== Suite A: Byte-Exact Encoding (lock btc) =====

#[test]
fn lock_btc_q_mem_rax_imm8_byte_exact() {
    // lock btc [rax], 2
    // LOCK (F0) + REX.W (48) + 0F BA + ModR/M (/7 = 38) + imm8 (02)
    // Expected: F0 48 0F BA 38 02
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBtc { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(2),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock btc [rax], 2");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xBA, 0x38, 0x02], "lock btc [rax], 2");
}

#[test]
fn lock_btc_q_mem_r13_disp0_imm8_byte_exact() {
    // lock btc [r13+0], 7
    // r13 is register id 13 (low bits = 5, high bit set -> REX.B)
    // RBP/R13 force mod=01 with disp8=0
    // LOCK (F0) + REX.W|B (49) + 0F BA + ModR/M with disp8 (7D = 01 111 101) + disp8 (00) + imm8 (07)
    // Expected: F0 49 0F BA 7D 00 07
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBtc { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(13), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(7),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock btc [r13+0], 7");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x49, 0x0F, 0xBA, 0x7D, 0x00, 0x07],
        "lock btc [r13+0], 7"
    );
}

// ===== Suite A: Byte-Exact Encoding (lock bts register form) =====

#[test]
fn lock_bts_q_mem_rax_rcx_byte_exact() {
    // lock bts [rax], rcx
    // LOCK (F0) + REX.W (48) + 0F AB + ModR/M (08 = 00 001 000, rcx in reg field)
    // Expected: F0 48 0F AB 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBts { width: IntWidth::W64 },
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
        .expect("encoding failed for lock bts [rax], rcx");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xAB, 0x08], "lock bts [rax], rcx");
}

#[test]
fn lock_bts_q_mem_r8_r15_byte_exact() {
    // lock bts [r8], r15
    // r8 is register id 8 (high bit set -> REX.B), r15 is register id 15 (high bit set -> REX.R)
    // LOCK (F0) + REX.W|R|B (4D) + 0F AB + ModR/M (38 = 00 111 000)
    // Expected: F0 4D 0F AB 38
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBts { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(8), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(15)),  // r15
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock bts [r8], r15");

    assert_eq!(buf.as_slice(), &[0xF0, 0x4D, 0x0F, 0xAB, 0x38], "lock bts [r8], r15");
}

// ===== Suite A: Byte-Exact Encoding (lock btr register form) =====

#[test]
fn lock_btr_q_mem_rax_rcx_byte_exact() {
    // lock btr [rax], rcx
    // LOCK (F0) + REX.W (48) + 0F B3 + ModR/M (08)
    // Expected: F0 48 0F B3 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBtr { width: IntWidth::W64 },
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
        .expect("encoding failed for lock btr [rax], rcx");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xB3, 0x08], "lock btr [rax], rcx");
}

#[test]
fn lock_btr_q_mem_rdi_disp8_r10_byte_exact() {
    // lock btr [rdi+8], r10
    // rdi is register id 7, r10 is register id 10 (high bit set -> REX.R)
    // LOCK (F0) + REX.W|R (4C) + 0F B3 + ModR/M with disp8 (57 = 01 010 111) + disp8 (08)
    // Expected: F0 4C 0F B3 57 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBtr { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
            Operand::Reg(RegId(10)),  // r10
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock btr [rdi+8], r10");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x4C, 0x0F, 0xB3, 0x57, 0x08],
        "lock btr [rdi+8], r10"
    );
}

// ===== Suite A: Byte-Exact Encoding (lock btc register form) =====

#[test]
fn lock_btc_q_mem_rax_rcx_byte_exact() {
    // lock btc [rax], rcx
    // LOCK (F0) + REX.W (48) + 0F BB + ModR/M (08)
    // Expected: F0 48 0F BB 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBtc { width: IntWidth::W64 },
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
        .expect("encoding failed for lock btc [rax], rcx");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xBB, 0x08], "lock btc [rax], rcx");
}

#[test]
fn lock_btc_q_mem_rsp_rax_byte_exact() {
    // lock btc [rsp], rax
    // rsp is register id 4 (SIB escape)
    // LOCK (F0) + REX.W (48) + 0F BB + ModR/M with SIB (04 = 00 000 100) + SIB (24)
    // Expected: F0 48 0F BB 04 24
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBtc { width: IntWidth::W64 },
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
        .expect("encoding failed for lock btc [rsp], rax");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xBB, 0x04, 0x24], "lock btc [rsp], rax");
}

// ===== Suite B: iced-x86 Round-trip Validation =====

#[test]
fn lock_bts_q_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBts { width: IntWidth::W64 },
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
        .expect("encoding failed for lock bts [rax], rcx");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Bts);
    assert!(decoded.has_lock_prefix(), "lock bts should have lock prefix");
}

#[test]
fn lock_btr_q_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBtr { width: IntWidth::W64 },
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
        .expect("encoding failed for lock btr [rax], rcx");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Btr);
    assert!(decoded.has_lock_prefix(), "lock btr should have lock prefix");
}

#[test]
fn lock_btc_q_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockBtc { width: IntWidth::W64 },
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
        .expect("encoding failed for lock btc [rax], rcx");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Btc);
    assert!(decoded.has_lock_prefix(), "lock btc should have lock prefix");
}
