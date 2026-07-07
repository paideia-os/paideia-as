//! Integration tests for MOV with absolute-disp32 memory forms (PA-R16-007 #1061).
//!
//! Tests verify that MOV instructions with absolute-address displacement-only memory operands
//! are correctly encoded using SIB no-base form:
//! - mov eax, [0x1000]          → 8B 04 25 00 10 00 00 (W32 read)
//! - mov [0x1000], eax          → 89 04 25 00 10 00 00 (W32 write reg)
//! - mov dword [0x1000], 0x1234 → C7 04 25 00 10 00 00 34 12 00 00 (W32 write imm)
//! - mov rax, [0x1000]          → 48 8B 04 25 00 10 00 00 (W64 read)
//! - mov [0x1000], rax          → 48 89 04 25 00 10 00 00 (W64 write reg)
//! - mov ax, [0x1000]           → 66 8B 04 25 00 10 00 00 (W16 read)
//! - mov al, [0x1000]           → 8A 04 25 00 10 00 00 (W8 read)
//!
//! Suite A: Byte-exact encoding validation (mov reads and writes, all widths).
//! Suite B: Extended register tests (REX.R handling).
//! Suite C: Negative displacement tests (sign-extended disp32).
//! Suite D: MemSeg (GS-prefix) integration tests.
//! Suite E: iced-x86 round-trip validation.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, SegPrefix};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (MOV reads and writes) =====

#[test]
fn mov_eax_mem_abs_disp32_0x1000_w32_read_byte_exact() {
    // mov eax, [0x1000]
    // 8B + ModRM (04) + SIB (25) + disp32 (00 10 00 00)
    // Expected: 8B 04 25 00 10 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W32,
        },
        operands: smallvec![
            Operand::Reg(RegId(0)), // eax (reg id 0)
            Operand::MemDisp { disp: 0x1000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov eax, [0x1000]");

    assert_eq!(
        buf.as_slice(),
        &[0x8B, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
        "mov eax, [0x1000] (W32 read)"
    );
}

#[test]
fn mov_mem_abs_disp32_0x1000_eax_w32_write_reg_byte_exact() {
    // mov [0x1000], eax
    // 89 + ModRM (04) + SIB (25) + disp32 (00 10 00 00)
    // Expected: 89 04 25 00 10 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W32,
        },
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
            Operand::Reg(RegId(0)), // eax (reg id 0)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [0x1000], eax");

    assert_eq!(
        buf.as_slice(),
        &[0x89, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
        "mov [0x1000], eax (W32 write reg)"
    );
}

#[test]
fn mov_mem_abs_disp32_0x1000_imm32_w32_write_imm_byte_exact() {
    // mov dword [0x1000], 0x1234
    // C7 + ModRM (04) + SIB (25) + disp32 (00 10 00 00) + imm32 (34 12 00 00)
    // Expected: C7 04 25 00 10 00 00 34 12 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W32,
        },
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
            Operand::Imm64(0x1234),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov dword [0x1000], 0x1234");

    assert_eq!(
        buf.as_slice(),
        &[0xC7, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00, 0x34, 0x12, 0x00, 0x00],
        "mov dword [0x1000], 0x1234 (W32 write imm)"
    );
}

#[test]
fn mov_rax_mem_abs_disp32_0x1000_w64_read_byte_exact() {
    // mov rax, [0x1000]
    // REX.W (48) + 8B + ModRM (04) + SIB (25) + disp32 (00 10 00 00)
    // Expected: 48 8B 04 25 00 10 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)), // rax (reg id 0, W64)
            Operand::MemDisp { disp: 0x1000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, [0x1000]");

    assert_eq!(
        buf.as_slice(),
        &[0x48, 0x8B, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
        "mov rax, [0x1000] (W64 read)"
    );
}

#[test]
fn mov_mem_abs_disp32_0x1000_rax_w64_write_reg_byte_exact() {
    // mov [0x1000], rax
    // REX.W (48) + 89 + ModRM (04) + SIB (25) + disp32 (00 10 00 00)
    // Expected: 48 89 04 25 00 10 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
            Operand::Reg(RegId(0)), // rax (reg id 0, W64)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [0x1000], rax");

    assert_eq!(
        buf.as_slice(),
        &[0x48, 0x89, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
        "mov [0x1000], rax (W64 write reg)"
    );
}

#[test]
fn mov_ax_mem_abs_disp32_0x1000_w16_read_byte_exact() {
    // mov ax, [0x1000]
    // 66 (operand-size override) + 8B + ModRM (04) + SIB (25) + disp32 (00 10 00 00)
    // Expected: 66 8B 04 25 00 10 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W16,
        },
        operands: smallvec![
            Operand::Reg(RegId(0)), // ax (reg id 0, W16)
            Operand::MemDisp { disp: 0x1000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov ax, [0x1000]");

    assert_eq!(
        buf.as_slice(),
        &[0x66, 0x8B, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
        "mov ax, [0x1000] (W16 read)"
    );
}

#[test]
fn mov_al_mem_abs_disp32_0x1000_w8_read_byte_exact() {
    // mov al, [0x1000]
    // 8A + ModRM (04) + SIB (25) + disp32 (00 10 00 00)
    // Expected: 8A 04 25 00 10 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W8,
        },
        operands: smallvec![
            Operand::Reg(RegId(0)), // al (reg id 0, W8)
            Operand::MemDisp { disp: 0x1000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov al, [0x1000]");

    assert_eq!(
        buf.as_slice(),
        &[0x8A, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
        "mov al, [0x1000] (W8 read)"
    );
}

// ===== Suite B: Extended Register Tests (REX.R handling) =====

#[test]
fn mov_r10d_mem_abs_disp32_0x2000_w32_extended_reg_byte_exact() {
    // mov r10d, [0x2000]
    // 8B + ModRM (0x04 | (2 << 3)) = 0x14 + SIB (25) + disp32 (00 20 00 00)
    // No REX needed for r10d because REX.R is only for register id >= 8 in the ModRM.reg field,
    // but r10d register id is 10, so (10 >> 3) = 1 (extended), so we need REX
    // Wait, let me reconsider: REX.R is for the r/m field when it needs extension,
    // but in this case the register is the destination (ModRM.reg field), so if reg id >= 8, we need REX.R
    // r10d has reg id 10, so (10 >> 3) = 1, we need REX.R
    // Expected: 41 8B 14 25 00 20 00 00 (with REX.R=1)
    // Actually, let me reconsider the ModRM encoding:
    // For mov r10d, [0x2000], r10d is the destination, so it goes in ModRM.reg field
    // r10d has reg id 10 = 0b1010, so r10d & 7 = 2, and (r10d >> 3) = 1
    // ModRM = mod(2 bits) reg(3 bits) r/m(3 bits) = 00 010 100 = 0x14
    // REX = 0x41 = 0b0100 0001 = REX.R=1
    // Expected: 41 8B 14 25 00 20 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W32,
        },
        operands: smallvec![
            Operand::Reg(RegId(10)), // r10d (reg id 10)
            Operand::MemDisp { disp: 0x2000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r10d, [0x2000]");

    // REX.R (0x44) because destination register r10d (id=10) needs REX
    assert_eq!(
        buf.as_slice(),
        &[0x44, 0x8B, 0x14, 0x25, 0x00, 0x20, 0x00, 0x00],
        "mov r10d, [0x2000] (extended register, W32 read with REX.R)"
    );
}

#[test]
fn mov_mem_abs_disp32_0x3000_r10_w64_extended_reg_byte_exact() {
    // mov [0x3000], r10
    // REX.W (48) + REX.R (04) = 0x4C + 89 + ModRM (0x14) + SIB (25) + disp32 (00 30 00 00)
    // r10 has reg id 10, so (10 >> 3) = 1 (REX.R), (10 & 7) = 2
    // ModRM = 00 010 100 = 0x14 (reg=2, r/m=4 for SIB)
    // REX = 0x4C = 0b0100 1100 = REX.W=1, REX.R=1
    // Expected: 4C 89 14 25 00 30 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemDisp { disp: 0x3000 },
            Operand::Reg(RegId(10)), // r10 (reg id 10, W64)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [0x3000], r10");

    assert_eq!(
        buf.as_slice(),
        &[0x4C, 0x89, 0x14, 0x25, 0x00, 0x30, 0x00, 0x00],
        "mov [0x3000], r10 (extended register, W64 write reg)"
    );
}

// ===== Suite B continuation: W16 Extended Register Tests (REX.R with 0x66 prefix) =====

#[test]
fn mov_r10w_mem_abs_disp32_0x2000_w16_extended_reg_correct_prefix_order() {
    // mov r10w, [0x2000]
    // Per Intel SDM Vol 2A §2.1.1: legacy prefixes (0x66) MUST precede REX
    // 66 (operand-size override) + 44 (REX.R) + 8B (opcode) + ModRM (0x14) + SIB (25) + disp32 (00 20 00 00)
    // r10w has reg id 10, so (10 >> 3) = 1 (REX.R), (10 & 7) = 2
    // ModRM = 00 010 100 = 0x14 (reg=2, r/m=4 for SIB)
    // Expected: 66 44 8B 14 25 00 20 00 00
    // This regression test verifies the fix for #1061: 0x66 must come BEFORE REX
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W16,
        },
        operands: smallvec![
            Operand::Reg(RegId(10)), // r10w (reg id 10)
            Operand::MemDisp { disp: 0x2000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r10w, [0x2000]");

    // Assert correct byte-exact encoding with 0x66 BEFORE REX
    assert_eq!(
        buf.as_slice(),
        &[0x66, 0x44, 0x8B, 0x14, 0x25, 0x00, 0x20, 0x00, 0x00],
        "mov r10w, [0x2000] (W16 extended register with correct prefix order: 0x66 then REX)"
    );
}

#[test]
fn mov_r10w_mem_abs_disp32_0x2000_w16_extended_reg_iced_round_trip() {
    // mov r10w, [0x2000]
    // Verify iced-x86 correctly decodes the W16 extended-register form to r10w (not dx)
    // This ensures REX.R bit is not dropped by incorrect prefix order
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W16,
        },
        operands: smallvec![
            Operand::Reg(RegId(10)), // r10w (reg id 10)
            Operand::MemDisp { disp: 0x2000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r10w, [0x2000]");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode mov r10w, [0x2000]");

    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    assert_eq!(instr.op_count(), 2);
    // Verify the destination register is r10w (not dx which would be 0x44 REX.R dropped)
    assert_eq!(
        instr.op0_register(),
        Register::R10W,
        "Decoded register should be r10w, not dx (verifies REX.R not dropped)"
    );
}

#[test]
fn mov_mem_abs_disp32_0x2000_r10w_w16_extended_reg_correct_prefix_order() {
    // mov [0x2000], r10w
    // Per Intel SDM Vol 2A §2.1.1: legacy prefixes (0x66) MUST precede REX
    // 66 (operand-size override) + 44 (REX.R) + 89 (opcode) + ModRM (0x14) + SIB (25) + disp32 (00 20 00 00)
    // r10w has reg id 10, so (10 >> 3) = 1 (REX.R), (10 & 7) = 2
    // ModRM = 00 010 100 = 0x14 (reg=2, r/m=4 for SIB)
    // Expected: 66 44 89 14 25 00 20 00 00
    // This regression test verifies the fix for #1061: 0x66 must come BEFORE REX
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W16,
        },
        operands: smallvec![
            Operand::MemDisp { disp: 0x2000 },
            Operand::Reg(RegId(10)), // r10w (reg id 10)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [0x2000], r10w");

    // Assert correct byte-exact encoding with 0x66 BEFORE REX
    assert_eq!(
        buf.as_slice(),
        &[0x66, 0x44, 0x89, 0x14, 0x25, 0x00, 0x20, 0x00, 0x00],
        "mov [0x2000], r10w (W16 extended register with correct prefix order: 0x66 then REX)"
    );
}

#[test]
fn mov_mem_abs_disp32_0x2000_r10w_w16_extended_reg_iced_round_trip() {
    // mov [0x2000], r10w
    // Verify iced-x86 correctly decodes the W16 extended-register form to r10w (not dx)
    // This ensures REX.R bit is not dropped by incorrect prefix order
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, Register};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W16,
        },
        operands: smallvec![
            Operand::MemDisp { disp: 0x2000 },
            Operand::Reg(RegId(10)), // r10w (reg id 10)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [0x2000], r10w");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode mov [0x2000], r10w");

    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    assert_eq!(instr.op_count(), 2);
    // Verify the source register is r10w (not dx which would be 0x44 REX.R dropped)
    assert_eq!(
        instr.op1_register(),
        Register::R10W,
        "Decoded register should be r10w, not dx (verifies REX.R not dropped)"
    );
}

// ===== Suite C: Negative Displacement Tests =====

#[test]
fn mov_rax_mem_abs_disp32_negative_0xffff_byte_exact() {
    // mov rax, [0xFFFFFFFF] (negative displacement: -1 sign-extended)
    // REX.W (48) + 8B + ModRM (04) + SIB (25) + disp32 (FF FF FF FF)
    // Expected: 48 8B 04 25 FF FF FF FF
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)), // rax
            Operand::MemDisp { disp: -1 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, [0xFFFFFFFF]");

    assert_eq!(
        buf.as_slice(),
        &[0x48, 0x8B, 0x04, 0x25, 0xFF, 0xFF, 0xFF, 0xFF],
        "mov rax, [0xFFFFFFFF] (negative disp32)"
    );
}

// ===== Suite D: MemSeg (GS-prefix) Integration =====

#[test]
fn mov_eax_gs_mem_abs_disp32_0x1000_w32_read_with_gs_prefix() {
    // mov eax, gs:[0x1000]
    // GS prefix (65) + 8B + ModRM (04) + SIB (25) + disp32 (00 10 00 00)
    // Expected: 65 8B 04 25 00 10 00 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W32,
        },
        operands: smallvec![
            Operand::Reg(RegId(0)), // eax
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
        .expect("encoding failed for mov eax, gs:[0x1000]");

    assert_eq!(
        buf.as_slice(),
        &[0x65, 0x8B, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
        "mov eax, gs:[0x1000] (W32 read with GS prefix)"
    );
}

// ===== Suite E: iced-x86 Round-Trip Validation =====

#[test]
fn mov_rax_mem_abs_disp32_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)), // rax
            Operand::MemDisp { disp: 0x1000 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, [0x1000]");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode mov rax, [0x1000]");

    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    assert_eq!(instr.op_count(), 2);
}

#[test]
fn mov_mem_abs_disp32_rax_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
            Operand::Reg(RegId(0)), // rax
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [0x1000], rax");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode mov [0x1000], rax");

    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    assert_eq!(instr.op_count(), 2);
}

#[test]
fn mov_mem_abs_disp32_imm32_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized {
            width: IntWidth::W32,
        },
        operands: smallvec![
            Operand::MemDisp { disp: 0x1000 },
            Operand::Imm64(0x1234),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov dword [0x1000], 0x1234");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode mov dword [0x1000], 0x1234");

    assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    assert_eq!(instr.op_count(), 2);
}
