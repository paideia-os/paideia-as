//! Integration tests for lock xadd encoding (PA-R15-002 #957).
//!
//! Tests verify that LOCK XADD (lock-prefixed fetch-and-add) instructions are correctly encoded:
//! - lock xadd_d [mem], r32 (32-bit); lock xadd_q [mem], r64 (64-bit)
//! - Base+disp form only (no index*scale).
//! - Encoding: F0 0F C1 /r (no REX.W for W32) or F0 REX.W 0F C1 /r (W64)
//!
//! Suite A: Byte-exact encoding validation (9 test vectors).
//! Suite B: Width rejection tests (W8/W16 unsupported).
//! Suite C: iced-x86 round-trip validation (2 representative cases).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (SDM test vectors) =====

// W64 (64-bit exchange, with REX.W and LOCK prefix)

#[test]
fn lock_xadd_q_mem_rax_rcx_byte_exact() {
    // lock xadd [rax], rcx
    // rcx=1 (src), rax=0 (base)
    // LOCK (F0) + REX.W (48) + opcode + ModR/M: F0 48 0F C1 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(1)),  // rcx (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xadd [rax], rcx");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xC1, 0x08], "lock xadd [rax], rcx");
}

// W32 (32-bit exchange, no REX.W, but LOCK prefix present)

#[test]
fn lock_xadd_d_mem_rax_eax_byte_exact() {
    // lock xadd [rax], eax
    // eax=0 (src), rax=0 (base)
    // REX elided (both R and B bits 0). LOCK (F0) only.
    // Expected: F0 0F C1 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // eax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xadd [rax], eax");

    assert_eq!(buf.as_slice(), &[0xF0, 0x0F, 0xC1, 0x00], "lock xadd [rax], eax");
}

#[test]
fn lock_xadd_q_mem_r13_8_r8_byte_exact() {
    // lock xadd [r13 + 8], r8
    // r8=8 (src, 8 >> 3 = 1), r13=13 (base, 13 >> 3 = 1)
    // LOCK (F0) + REX.W | R | B (4D) + 0F + C1 + ModR/M with disp8
    // Expected: F0 4D 0F C1 45 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(13), index: None, scale: Scale::X1, disp: 8 },
            Operand::Reg(RegId(8)),  // r8 (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xadd [r13+8], r8");

    assert_eq!(buf.as_slice(), &[0xF0, 0x4D, 0x0F, 0xC1, 0x45, 0x08], "lock xadd [r13+8], r8");
}

#[test]
fn lock_xadd_q_mem_rbp_rax_byte_exact() {
    // lock xadd [rbp], rax
    // rax=0 (src), rbp=5 (base, 5 >> 3 = 0)
    // RBP as base forces mod=01 (disp8=0 escape).
    // LOCK (F0) + REX.W (48) + 0F + C1 + ModR/M (45) + disp8 (00)
    // Expected: F0 48 0F C1 45 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // rax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xadd [rbp], rax");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xC1, 0x45, 0x00], "lock xadd [rbp], rax");
}

#[test]
fn lock_xadd_q_mem_rsp_rax_byte_exact() {
    // lock xadd [rsp], rax
    // rax=0 (src), rsp=4 (base)
    // RSP as base requires SIB escape: ModR/M=04 (rm=100), SIB=24 (scale=00, index=4, base=4)
    // LOCK (F0) + REX.W (48) + 0F + C1 + ModR/M (04) + SIB (24)
    // Expected: F0 48 0F C1 04 24
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // rax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xadd [rsp], rax");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x0F, 0xC1, 0x04, 0x24], "lock xadd [rsp], rax");
}

#[test]
fn lock_xadd_q_mem_rdi_0x100_r15_byte_exact() {
    // lock xadd [rdi + 0x100], r15
    // r15=15 (src, 15 >> 3 = 1), rdi=7 (base, 7 >> 3 = 0)
    // disp32 (0x100 requires 4-byte displacement)
    // LOCK (F0) + REX.W | R (4C) + 0F + C1 + ModR/M (BF) + disp32 (00 01 00 00)
    // Expected: F0 4C 0F C1 BF 00 01 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0x100 },
            Operand::Reg(RegId(15)),  // r15 (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xadd [rdi+0x100], r15");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x4C, 0x0F, 0xC1, 0xBF, 0x00, 0x01, 0x00, 0x00],
        "lock xadd [rdi+0x100], r15"
    );
}

#[test]
fn lock_xadd_d_mem_r15_4_r15d_byte_exact() {
    // lock xadd [r15 + 4], r15d
    // r15d=15 (src, no W). r15=15 (base, 15 >> 3 = 1)
    // REX: R=1 (r15 >> 3), B=1 (r15 >> 3), W=0 (32-bit) → 0x45
    // ModR/M: mod=01 (disp8), reg=7 (r15d&7), rm=7 (r15&7) → 0x7F
    // Expected: F0 45 0F C1 7F 04
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(15), index: None, scale: Scale::X1, disp: 4 },
            Operand::Reg(RegId(15)),  // r15d (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xadd [r15+4], r15d");

    assert_eq!(buf.as_slice(), &[0xF0, 0x45, 0x0F, 0xC1, 0x7F, 0x04], "lock xadd [r15+4], r15d");
}

#[test]
fn lock_xadd_d_mem_rsp_eax_byte_exact() {
    // lock xadd [rsp], eax
    // eax=0 (src), rsp=4 (base)
    // No REX (both R and B bits 0, no W). SIB escape: ModR/M=04, SIB=24
    // LOCK (F0) + 0F + C1 + ModR/M (04) + SIB (24)
    // Expected: F0 0F C1 04 24
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // eax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xadd [rsp], eax");

    assert_eq!(buf.as_slice(), &[0xF0, 0x0F, 0xC1, 0x04, 0x24], "lock xadd [rsp], eax");
}

// W64 16-register src sweep loop: REX.R + ModR/M reg field verification

#[test]
fn lock_xadd_q_sweep_all_src_regs() {
    // Verify all 16 registers encode correctly as src in lock xadd
    for src_id in 0..16 {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::LockXadd { width: IntWidth::W64 },
            operands: smallvec![
                Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
                Operand::Reg(RegId(src_id)),
            ],
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            encoding_hint: None,
            emission_order: 0,
};

        let mut stats = EncodeStats::new();
        let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
        assert!(
            result.is_ok(),
            "failed to encode lock xadd [rax], r{:?}",
            src_id
        );

        // Verify LOCK prefix, REX byte, opcodes, and ModR/M reg field
        assert_eq!(buf.as_slice()[0], 0xF0, "missing LOCK prefix for r{:?}", src_id);
        assert_eq!(buf.as_slice()[1], 0x48 | if (src_id >> 3) != 0 { 0x04 } else { 0 },
                   "incorrect REX byte for r{:?}", src_id);
        assert_eq!(buf.as_slice()[2], 0x0F, "incorrect opcode byte 1 for r{:?}", src_id);
        assert_eq!(buf.as_slice()[3], 0xC1, "incorrect opcode byte 2 for r{:?}", src_id);
        // ModR/M: mod=00, reg=(src_id&7), rm=000 → 0x00 | ((src_id&7) << 3)
        assert_eq!(
            buf.as_slice()[4],
            ((src_id & 7) as u8) << 3,
            "incorrect ModR/M reg field for r{:?}",
            src_id
        );
    }
}

// ===== Suite B: Width Rejection Tests =====

#[test]
fn lock_xadd_w8_unsupported() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W8 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    let err = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(
        err.is_err(),
        "lock xadd W8 should be rejected"
    );
}

#[test]
fn lock_xadd_w16_unsupported() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W16 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    let err = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(
        err.is_err(),
        "lock xadd W16 should be rejected"
    );
}

// ===== Suite C: iced-x86 Round-trip Validation =====

#[test]
fn lock_xadd_q_mem_rdi_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(1)),  // rcx (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xadd [rdi], rcx");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Xadd);
    assert!(decoded.has_lock_prefix(), "lock xadd should have lock prefix");
    // Verify REX.W is present for W64
    assert!(buf.as_slice()[1] & 0x08 != 0, "REX.W should be set for W64");
}

#[test]
fn lock_xadd_d_mem_rdi_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockXadd { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(1)),  // ecx (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock xadd [rdi], ecx");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Xadd);
    assert!(decoded.has_lock_prefix(), "lock xadd should have lock prefix");
    // Verify REX.W is not present for W32 (for ecx/rdi, no REX at all)
    // Expected: F0 0F C1 0C (4 bytes)
    assert_eq!(buf.as_slice()[1], 0x0F, "second byte should be 0F (opcode), not a REX byte");
}
