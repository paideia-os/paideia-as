//! Integration tests for lock inc/add with absolute-disp32 memory forms (PA-R16-007 #1060).
//!
//! Tests verify that LOCK INC and LOCK ADD (lock-prefixed increment/add at absolute addresses)
//! are correctly encoded using SIB no-base form:
//! - lock inc [disp32]          → F0 48 FF 04 25 <disp32>
//! - lock add [disp32], imm8    → F0 48 83 04 25 <disp32> <imm8>
//! - lock add [disp32], imm32   → F0 48 81 04 25 <disp32> <imm32>
//!
//! Suite A: Byte-exact encoding validation (lock inc + lock add).
//! Suite B: Negative displacement tests (sign-extended disp32).
//! Suite C: MemSeg (GS-prefix) integration tests.
//! Suite D: iced-x86 round-trip validation.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, SegPrefix};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (lock inc) =====

#[test]
fn lock_inc_q_mem_abs_disp32_0x1000_byte_exact() {
    // lock inc qword [0x1000]
    // LOCK (F0) + REX.W (48) + FF + ModRM (04) + SIB (25) + disp32 (00 10 00 00)
    // Expected: F0 48 FF 04 25 00 10 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockInc { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock inc qword [0x1000]");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0xFF, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
        "lock inc qword [0x1000]"
    );
}

#[test]
fn lock_inc_q_mem_abs_disp32_0x0_byte_exact() {
    // lock inc qword [0x0]
    // LOCK (F0) + REX.W (48) + FF + ModRM (04) + SIB (25) + disp32 (00 00 00 00)
    // Expected: F0 48 FF 04 25 00 00 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockInc { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock inc qword [0x0]");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0xFF, 0x04, 0x25, 0x00, 0x00, 0x00, 0x00],
        "lock inc qword [0x0]"
    );
}

#[test]
fn lock_inc_d_mem_abs_disp32_0x2000_byte_exact() {
    // lock inc dword [0x2000]  (W32 form)
    // LOCK (F0) + FF + ModRM (04) + SIB (25) + disp32 (00 20 00 00)
    // Expected: F0 FF 04 25 00 20 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockInc { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x2000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock inc dword [0x2000]");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0xFF, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00],
        "lock inc dword [0x2000]"
    );
}

// ===== Suite A: Byte-Exact Encoding (lock add imm8) =====

#[test]
fn lock_add_q_mem_abs_disp32_imm8_0x1000_1_byte_exact() {
    // lock add qword [0x1000], 1
    // LOCK (F0) + REX.W (48) + 83 + ModRM (04) + SIB (25) + disp32 (00 10 00 00) + imm8 (01)
    // Expected: F0 48 83 04 25 00 10 00 00 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
            Operand::Imm64(1),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add qword [0x1000], 1");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x83, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00, 0x01],
        "lock add qword [0x1000], 1"
    );
}

#[test]
fn lock_add_q_mem_abs_disp32_imm8_neg_byte_exact() {
    // lock add qword [0x2000], -5
    // LOCK (F0) + REX.W (48) + 83 + ModRM (04) + SIB (25) + disp32 (00 20 00 00) + imm8 (FB)
    // Expected: F0 48 83 04 25 00 20 00 00 FB
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x2000 },
            Operand::Imm64(-5i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add qword [0x2000], -5");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x83, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, 0xFB],
        "lock add qword [0x2000], -5"
    );
}

// ===== Suite A: Byte-Exact Encoding (lock add imm32) =====

#[test]
fn lock_add_q_mem_abs_disp32_imm32_0x1000_0x100_byte_exact() {
    // lock add qword [0x1000], 0x100
    // LOCK (F0) + REX.W (48) + 81 + ModRM (04) + SIB (25) + disp32 (00 10 00 00) + imm32 (00 01 00 00)
    // Expected: F0 48 81 04 25 00 10 00 00 00 01 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
            Operand::Imm64(0x100),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add qword [0x1000], 0x100");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x81, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00],
        "lock add qword [0x1000], 0x100"
    );
}

#[test]
fn lock_add_q_mem_abs_disp32_imm32_large_byte_exact() {
    // lock add qword [0x3000], 0x12345678
    // LOCK (F0) + REX.W (48) + 81 + ModRM (04) + SIB (25) + disp32 (00 30 00 00) + imm32 (78 56 34 12)
    // Expected: F0 48 81 04 25 00 30 00 00 78 56 34 12
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x3000 },
            Operand::Imm64(0x12345678),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add qword [0x3000], 0x12345678");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x81, 0x04, 0x25, 0x00, 0x30, 0x00, 0x00, 0x78, 0x56, 0x34, 0x12],
        "lock add qword [0x3000], 0x12345678"
    );
}

#[test]
fn lock_add_d_mem_abs_disp32_imm32_w32_byte_exact() {
    // lock add dword [0x4000], 0x10000  (W32 form)
    // LOCK (F0) + 81 + ModRM (04) + SIB (25) + disp32 (00 40 00 00) + imm32 (00 00 01 00)
    // Expected: F0 81 04 25 00 40 00 00 00 00 01 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x4000 },
            Operand::Imm64(0x10000),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add dword [0x4000], 0x10000");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x81, 0x04, 0x25, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00],
        "lock add dword [0x4000], 0x10000"
    );
}

// ===== Suite B: Negative Displacement Tests =====

#[test]
fn lock_inc_q_mem_abs_disp32_neg_byte_exact() {
    // lock inc qword [-0x1000]  (signed disp32)
    // Disp32 = -0x1000 → 0xFFFFF000 (little-endian: 00 F0 FF FF)
    // Expected: F0 48 FF 04 25 00 F0 FF FF
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockInc { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: -0x1000i32 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock inc qword [-0x1000]");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0xFF, 0x04, 0x25, 0x00, 0xF0, 0xFF, 0xFF],
        "lock inc qword [-0x1000]"
    );
}

#[test]
fn lock_add_q_mem_abs_disp32_neg_imm8_byte_exact() {
    // lock add qword [-0x2000], -10
    // Disp32 = -0x2000 → 0xFFFFE000 (little-endian: 00 E0 FF FF)
    // Imm8 = -10 → 0xF6
    // Expected: F0 48 83 04 25 00 E0 FF FF F6
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: -0x2000i32 },
            Operand::Imm64(-10i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lock add qword [-0x2000], -10");

    assert_eq!(
        buf.as_slice(),
        &[0xF0, 0x48, 0x83, 0x04, 0x25, 0x00, 0xE0, 0xFF, 0xFF, 0xF6],
        "lock add qword [-0x2000], -10"
    );
}

// ===== Suite C: MemSeg (GS-prefix) Integration =====

#[test]
fn lock_inc_q_mem_gs_abs_disp32_0x1000_byte_exact() {
    // gs lock inc qword [0x1000]
    // Segment prefix (65) + LOCK (F0) + REX.W (48) + FF + ModRM (04) + SIB (25) + disp32 (00 10 00 00)
    // Expected: 65 F0 48 FF 04 25 00 10 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockInc { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemDisp { disp: 0x1000 }),
            },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for gs lock inc qword [0x1000]");

    assert_eq!(
        buf.as_slice(),
        &[0x65, 0xF0, 0x48, 0xFF, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
        "gs lock inc qword [0x1000]"
    );
}

#[test]
fn lock_add_q_mem_gs_abs_disp32_imm8_byte_exact() {
    // gs lock add qword [0x2000], 5
    // Segment prefix (65) + LOCK (F0) + REX.W (48) + 83 + ModRM (04) + SIB (25) + disp32 (00 20 00 00) + imm8 (05)
    // Expected: 65 F0 48 83 04 25 00 20 00 00 05
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemDisp { disp: 0x2000 }),
            },
            Operand::Imm64(5),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for gs lock add qword [0x2000], 5");

    assert_eq!(
        buf.as_slice(),
        &[0x65, 0xF0, 0x48, 0x83, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, 0x05],
        "gs lock add qword [0x2000], 5"
    );
}

#[test]
fn lock_add_q_mem_gs_abs_disp32_imm32_byte_exact() {
    // gs lock add qword [0x3000], 0x11223344
    // Segment prefix (65) + LOCK (F0) + REX.W (48) + 81 + ModRM (04) + SIB (25) + disp32 (00 30 00 00) + imm32 (44 33 22 11)
    // Expected: 65 F0 48 81 04 25 00 30 00 00 44 33 22 11
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemDisp { disp: 0x3000 }),
            },
            Operand::Imm64(0x11223344),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for gs lock add qword [0x3000], 0x11223344");

    assert_eq!(
        buf.as_slice(),
        &[0x65, 0xF0, 0x48, 0x81, 0x04, 0x25, 0x00, 0x30, 0x00, 0x00, 0x44, 0x33, 0x22, 0x11],
        "gs lock add qword [0x3000], 0x11223344"
    );
}

// ===== Suite D: iced-x86 Round-Trip Validation =====

#[test]
fn lock_inc_q_mem_abs_disp32_iced_round_trip() {
    // lock inc qword [0x1000] → verify iced can decode it back
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockInc { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    // Verify mnemonic is Inc (LOCK is separate)
    assert_eq!(decoded.mnemonic(), IcedMnem::Inc);

    // Verify length
    assert_eq!(decoded.len(), 9);
}

#[test]
fn lock_add_q_mem_abs_disp32_imm8_iced_round_trip() {
    // lock add qword [0x1000], 1 → verify iced can decode it back
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
            Operand::Imm64(1),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    // Verify mnemonic is Add (LOCK is separate)
    assert_eq!(decoded.mnemonic(), IcedMnem::Add);

    // Verify length
    assert_eq!(decoded.len(), 10);
}

#[test]
fn lock_add_q_mem_abs_disp32_imm32_iced_round_trip() {
    // lock add qword [0x1000], 0x12345678 → verify iced can decode it back
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockAdd { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
            Operand::Imm64(0x12345678),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    // Verify mnemonic is Add (LOCK is separate)
    assert_eq!(decoded.mnemonic(), IcedMnem::Add);

    // Verify length
    assert_eq!(decoded.len(), 13);
}

#[test]
fn lock_inc_d_mem_abs_disp32_iced_round_trip() {
    // lock inc dword [0x2000] (W32) → verify iced can decode it back
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::LockInc { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemDisp { disp: 0x2000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let decoded = decoder.decode();

    // Verify mnemonic is Inc (LOCK is separate)
    assert_eq!(decoded.mnemonic(), IcedMnem::Inc);

    // Verify length
    assert_eq!(decoded.len(), 8);
}
