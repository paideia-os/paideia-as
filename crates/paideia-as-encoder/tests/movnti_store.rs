//! Integration tests for non-temporal store MOVNTI encoding (PA-R14-003 #946).
//!
//! Tests verify that MOVNTI (non-temporal store, bypasses cache) instructions are correctly encoded:
//! - movnti_d [mem], r32 (32-bit); movnti_q [mem], r64 (64-bit)
//! - Both base+disp and SIB forms (base+index*scale+disp)
//! - Encoding: 0F C3 /r (no REX.W for W32) or REX.W 0F C3 /r (W64)
//!
//! Suite A: Byte-exact encoding validation (6 test vectors from SDM).
//! Suite B: iced-x86 round-trip validation (2 representative cases).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (SDM test vectors) =====

// W32 (32-bit store, no REX.W)

#[test]
fn movnti_d_mem_rdi_0_byte_exact() {
    // movnti [rdi], eax
    // eax=0, rdi=7
    // Encoding: 0F C3 /r with no REX byte (both R and B bits 0)
    // Expected: 0F C3 07
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Movnti { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // eax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for movnti_d [rdi], eax");

    assert_eq!(buf.as_slice(), &[0x0F, 0xC3, 0x07], "movnti_d [rdi], eax");
}

#[test]
fn movnti_d_mem_rdi_8_r10d_byte_exact() {
    // movnti [rdi + 8], r10d
    // r10d=10 (src, high bit set: 10 >> 3 = 1), rdi=7 (base, 7 >> 3 = 0)
    // REX: R=1 (r10 >> 3), B=0 (rdi >> 3) → 0x44
    // ModR/M: /r form (src in reg field) → 57 (mod=01 reg=010 rm=111)
    // Expected: 44 0F C3 57 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Movnti { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
            Operand::Reg(RegId(10)),  // r10d (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for movnti_d [rdi + 8], r10d");

    assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0xC3, 0x57, 0x08], "movnti_d [rdi+8], r10d");
}

// W64 (64-bit store, with REX.W)

#[test]
fn movnti_q_mem_rdi_0_byte_exact() {
    // movnti [rdi], rax
    // rax=0, rdi=7
    // REX: W=1, R=0, B=0 → 0x48
    // Encoding: 0F C3 /r with ModR/M 07
    // Expected: 48 0F C3 07
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Movnti { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // rax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for movnti_q [rdi], rax");

    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xC3, 0x07], "movnti_q [rdi], rax");
}

// W64 SIB form (base+index*scale+disp)

#[test]
fn movnti_q_mem_r12_rsi_4_byte_exact() {
    // movnti [r12 + rsi*4], rax
    // rax=0 (src, 0 >> 3 = 0), r12=12 (base, 12 >> 3 = 1), rsi=6 (index, 6 >> 3 = 0)
    // REX: W=1, R=0, X=0, B=1 → 0x49
    // ModR/M: mod=00 reg=000 rm=100 (SIB follows) → 0x04
    // SIB: scale=2 (scale_bits=2 for X4), index=6, base=4 (r12&7) → (2<<6)|(6<<3)|4 = 0xB4
    // Expected: 49 0F C3 04 B4
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Movnti { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(12), index: Some(RegId(6)), scale: Scale::X4, disp: 0 },
            Operand::Reg(RegId(0)),  // rax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for movnti_q [r12 + rsi*4], rax");

    assert_eq!(buf.as_slice(), &[0x49, 0x0F, 0xC3, 0x04, 0xB4], "movnti_q [r12+rsi*4], rax");
}

#[test]
fn movnti_q_mem_rbp_r15_byte_exact() {
    // movnti [rbp], r15
    // r15=15 (src, 15 >> 3 = 1), rbp=5 (base, 5 >> 3 = 0)
    // REX: W=1, R=1, B=0 → 0x4C
    // For RBP as base with disp=0, emit_mem_base_disp forces mod=01 (due to RBP escape):
    // ModR/M: mod=01 reg=111 rm=101 → 0x7D
    // disp8: 0x00
    // Expected: 4C 0F C3 7D 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Movnti { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(15)),  // r15 (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for movnti_q [rbp], r15");

    assert_eq!(buf.as_slice(), &[0x4C, 0x0F, 0xC3, 0x7D, 0x00], "movnti_q [rbp], r15");
}

// ===== Suite B: iced-x86 Round-trip Validation =====

#[test]
fn movnti_d_mem_rdi_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Movnti { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // eax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for movnti_d [rdi], eax");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Movnti);
}

#[test]
fn movnti_q_mem_r12_rsi_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Movnti { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(12), index: Some(RegId(6)), scale: Scale::X4, disp: 0 },
            Operand::Reg(RegId(0)),  // rax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for movnti_q [r12 + rsi*4], rax");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Movnti);
}
