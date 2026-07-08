//! Integration tests for lock add/sub encoding (PA-R15-003 #958).
//!
//! Tests verify that LOCK ADD and LOCK SUB (lock-prefixed add/subtract) instructions
//! are correctly encoded with immediate or register operands:
//! - lock add/sub [mem], imm8/imm32/r32/r64
//! - Base+disp form only (no index*scale).
//! - Encoding: F0 [REX.W] 83/81/01/29 /r [imm8/imm32]
//!
//! Suite A: Byte-exact encoding validation (9 test vectors per mnemonic).
//! Suite B: imm8/imm32 boundary tests (4 test vectors).
//! Suite C: iced-x86 round-trip validation (4 test vectors).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (lock add) =====

#[test]
fn lock_add_q_mem_rax_imm8_byte_exact() {
    // lock add [rax], 1
    // LOCK (F0) + REX.W (48) + 83 + ModR/M (00) + imm8 (01)
    // Expected: F0 48 83 00 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
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
        .expect("encoding failed for lock add [rax], 1");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x83, 0x00, 0x01], "lock add [rax], 1");
}

#[test]
fn lock_add_q_mem_rax_imm8_neg_byte_exact() {
    // lock add [rax], -1
    // LOCK (F0) + REX.W (48) + 83 + ModR/M (00) + imm8 (FF sign-extended)
    // Expected: F0 48 83 00 FF
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(-1i64 as u64 as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add [rax], -1");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x83, 0x00, 0xFF], "lock add [rax], -1");
}

#[test]
fn lock_add_q_mem_rax_imm32_byte_exact() {
    // lock add [rax], 0x100
    // LOCK (F0) + REX.W (48) + 81 + ModR/M (00) + imm32 (00 01 00 00)
    // Expected: F0 48 81 00 00 01 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(0x100),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add [rax], 0x100");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x81, 0x00, 0x00, 0x01, 0x00, 0x00],
        "lock add [rax], 0x100"
    );
}

#[test]
fn lock_add_q_mem_rax_rcx_byte_exact() {
    // lock add [rax], rcx
    // LOCK (F0) + REX.W (48) + 01 + ModR/M (08)
    // Expected: F0 48 01 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
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
        .expect("encoding failed for lock add [rax], rcx");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x01, 0x08], "lock add [rax], rcx");
}

#[test]
fn lock_add_q_mem_r13_8_r8_byte_exact() {
    // lock add [r13+8], r8
    // r8=8, r13=13 (both have high bit set)
    // LOCK (F0) + REX.W|R|B (4D) + 01 + ModR/M with disp8
    // Expected: F0 4D 01 45 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(13), index: None, scale: Scale::X1, disp: 8 },
            Operand::Reg(RegId(8)),  // r8
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add [r13+8], r8");

    assert_eq!(buf.as_slice(), &[0xF0, 0x4D, 0x01, 0x45, 0x08], "lock add [r13+8], r8");
}

#[test]
fn lock_add_q_mem_rsp_imm8_byte_exact() {
    // lock add [rsp], 1
    // RSP requires SIB: ModR/M=04, SIB=24
    // LOCK (F0) + REX.W (48) + 83 + ModR/M (04) + SIB (24) + imm8 (01)
    // Expected: F0 48 83 04 24 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(1),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add [rsp], 1");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x83, 0x04, 0x24, 0x01],
        "lock add [rsp], 1"
    );
}

#[test]
fn lock_add_q_mem_rbp_imm8_byte_exact() {
    // lock add [rbp], 1
    // RBP forces mod=01 (disp8=0 escape)
    // LOCK (F0) + REX.W (48) + 83 + ModR/M (45) + disp8 (00) + imm8 (01)
    // Expected: F0 48 83 45 00 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(1),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add [rbp], 1");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x83, 0x45, 0x00, 0x01],
        "lock add [rbp], 1"
    );
}

#[test]
fn lock_add_d_mem_rax_eax_byte_exact() {
    // lock add [rax], eax (32-bit)
    // LOCK (F0) + 01 + ModR/M (00)
    // Expected: F0 01 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // eax
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add [rax], eax");

    assert_eq!(buf.as_slice(), &[0xF0, 0x01, 0x00], "lock add [rax], eax");
}

#[test]
fn lock_add_d_mem_rax_imm8_byte_exact() {
    // lock add [rax], 1 (32-bit)
    // LOCK (F0) + 83 + ModR/M (00) + imm8 (01)
    // Expected: F0 83 00 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W32 },
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
        .expect("encoding failed for lock add [rax], 1");

    assert_eq!(buf.as_slice(), &[0xF0, 0x83, 0x00, 0x01], "lock add [rax], 1 (W32)");
}

// ===== Suite A: Byte-Exact Encoding (lock sub) =====

#[test]
fn lock_sub_q_mem_rax_imm8_byte_exact() {
    // lock sub [rax], 1
    // LOCK (F0) + REX.W (48) + 83 /5 + ModR/M (28) + imm8 (01)
    // Expected: F0 48 83 28 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
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
        .expect("encoding failed for lock sub [rax], 1");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x83, 0x28, 0x01], "lock sub [rax], 1");
}

#[test]
fn lock_sub_q_mem_rax_imm8_neg_byte_exact() {
    // lock sub [rax], -1
    // LOCK (F0) + REX.W (48) + 83 /5 + ModR/M (28) + imm8 (FF)
    // Expected: F0 48 83 28 FF
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(-1i64 as u64 as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock sub [rax], -1");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x83, 0x28, 0xFF], "lock sub [rax], -1");
}

#[test]
fn lock_sub_q_mem_rax_imm32_byte_exact() {
    // lock sub [rax], 0x100
    // LOCK (F0) + REX.W (48) + 81 /5 + ModR/M (28) + imm32 (00 01 00 00)
    // Expected: F0 48 81 28 00 01 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(0x100),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock sub [rax], 0x100");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x81, 0x28, 0x00, 0x01, 0x00, 0x00],
        "lock sub [rax], 0x100"
    );
}

#[test]
fn lock_sub_q_mem_rax_rcx_byte_exact() {
    // lock sub [rax], rcx
    // LOCK (F0) + REX.W (48) + 29 + ModR/M (08)
    // Expected: F0 48 29 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
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
        .expect("encoding failed for lock sub [rax], rcx");

    assert_eq!(buf.as_slice(), &[0xF0, 0x48, 0x29, 0x08], "lock sub [rax], rcx");
}

#[test]
fn lock_sub_q_mem_r13_8_r8_byte_exact() {
    // lock sub [r13+8], r8
    // r8=8, r13=13 (both have high bit set)
    // LOCK (F0) + REX.W|R|B (4D) + 29 + ModR/M with disp8
    // Expected: F0 4D 29 45 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(13), index: None, scale: Scale::X1, disp: 8 },
            Operand::Reg(RegId(8)),  // r8
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock sub [r13+8], r8");

    assert_eq!(buf.as_slice(), &[0xF0, 0x4D, 0x29, 0x45, 0x08], "lock sub [r13+8], r8");
}

#[test]
fn lock_sub_q_mem_rsp_imm8_byte_exact() {
    // lock sub [rsp], 1
    // RSP requires SIB
    // LOCK (F0) + REX.W (48) + 83 /5 + ModR/M (04) + SIB (24) + imm8 (01)
    // Expected: F0 48 83 2C 24 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(1),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock sub [rsp], 1");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x83, 0x2C, 0x24, 0x01],
        "lock sub [rsp], 1"
    );
}

#[test]
fn lock_sub_q_mem_rbp_imm8_byte_exact() {
    // lock sub [rbp], 1
    // RBP forces mod=01 (disp8=0 escape)
    // LOCK (F0) + REX.W (48) + 83 /5 + ModR/M (6D) + disp8 (00) + imm8 (01)
    // Expected: F0 48 83 6D 00 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(1),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock sub [rbp], 1");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x83, 0x6D, 0x00, 0x01],
        "lock sub [rbp], 1"
    );
}

#[test]
fn lock_sub_d_mem_rax_eax_byte_exact() {
    // lock sub [rax], eax (32-bit)
    // LOCK (F0) + 29 + ModR/M (00)
    // Expected: F0 29 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Reg(RegId(0)),  // eax
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock sub [rax], eax");

    assert_eq!(buf.as_slice(), &[0xF0, 0x29, 0x00], "lock sub [rax], eax");
}

#[test]
fn lock_sub_d_mem_rax_imm8_byte_exact() {
    // lock sub [rax], 1 (32-bit)
    // LOCK (F0) + 83 /5 + ModR/M (28) + imm8 (01)
    // Expected: F0 83 28 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W32 },
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
        .expect("encoding failed for lock sub [rax], 1");

    assert_eq!(buf.as_slice(), &[0xF0, 0x83, 0x28, 0x01], "lock sub [rax], 1 (W32)");
}

// ===== Suite B: imm8/imm32 Boundary Tests =====

#[test]
fn lock_add_q_imm127_byte_exact() {
    // lock add [rax], 127 (fits in imm8)
    // Expected 5 bytes: F0 48 83 00 7F
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(127),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add [rax], 127");

    assert_eq!(buf.as_slice().len(), 5, "lock add [rax], 127 should be 5 bytes (imm8)");
}

#[test]
fn lock_add_q_imm128_byte_exact() {
    // lock add [rax], 128 (does not fit in imm8, needs imm32)
    // Expected 8 bytes: F0 48 81 00 80 00 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(128),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add [rax], 128");

    assert_eq!(buf.as_slice().len(), 8, "lock add [rax], 128 should be 8 bytes (imm32)");
}

#[test]
fn lock_sub_q_imm127_byte_exact() {
    // lock sub [rax], 127 (fits in imm8)
    // Expected 5 bytes: F0 48 83 28 7F
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(127),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock sub [rax], 127");

    assert_eq!(buf.as_slice().len(), 5, "lock sub [rax], 127 should be 5 bytes (imm8)");
}

#[test]
fn lock_sub_q_imm128_byte_exact() {
    // lock sub [rax], 128 (does not fit in imm8, needs imm32)
    // Expected 8 bytes: F0 48 81 28 80 00 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(128),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock sub [rax], 128");

    assert_eq!(buf.as_slice().len(), 8, "lock sub [rax], 128 should be 8 bytes (imm32)");
}

// ===== Suite C: iced-x86 Round-trip Validation =====

#[test]
fn lock_add_q_mem_rdi_rax_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
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
        .expect("encoding failed for lock add [rdi], rax");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Add);
    assert!(decoded.has_lock_prefix(), "lock add should have lock prefix");
}

#[test]
fn lock_add_q_mem_rdi_imm1_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(1),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add [rdi], 1");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Add);
    assert!(decoded.has_lock_prefix(), "lock add should have lock prefix");
}

#[test]
fn lock_sub_q_mem_rdi_rax_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
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
        .expect("encoding failed for lock sub [rdi], rax");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Sub);
    assert!(decoded.has_lock_prefix(), "lock sub should have lock prefix");
}

#[test]
fn lock_sub_q_mem_rdi_imm1_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockSub { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(1),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock sub [rdi], 1");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Sub);
    assert!(decoded.has_lock_prefix(), "lock sub should have lock prefix");
}
