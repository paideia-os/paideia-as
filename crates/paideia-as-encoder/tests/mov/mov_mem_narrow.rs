//! Integration tests for narrow-form MOV encoding with memory operands (PA13-001).
//!
//! Tests verify that narrow MOV instructions with memory sources are correctly encoded:
//! - mov r8, [mem]; mov r16, [mem]; mov r32, [mem]
//! - Both base+disp and SIB forms (base+index*scale+disp)
//!
//! Suite A: Byte-exact encoding validation (24 test vectors from softarch).
//! Suite B: iced-x86 round-trip validation (4 representative cases).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (softarch test vectors) =====

// W8 base+disp tests

#[test]
fn mov_al_mem_rdi_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov al, [rdi]");

    // Expected: 8A 07 (2 bytes)
    assert_eq!(buf.as_slice(), &[0x8A, 0x07]);
}

#[test]
fn mov_r8b_mem_rdi_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::Reg(RegId(8)),
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r8b, [rdi]");

    // Expected: 44 8A 07 (3 bytes, REX.R for r8b destination)
    // r8b is RegId(8), so REX.R = (8 >> 3) = 1
    // rdi is RegId(7), so REX.B = (7 >> 3) = 0
    // REX byte = 0x40 | (1 << 2) = 0x44
    assert_eq!(buf.as_slice(), &[0x44, 0x8A, 0x07]);
}

#[test]
fn mov_bl_mem_rbx_8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::Reg(RegId(3)),
            Operand::MemSib { base: RegId(3), index: None, scale: Scale::X1, disp: 8 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov bl, [rbx + 8]");

    // Expected: 8A 5B 08 (3 bytes, bl=RegId(3), so reg=011 in ModR/M)
    assert_eq!(buf.as_slice(), &[0x8A, 0x5B, 0x08]);
}

#[test]
fn mov_dl_mem_rsi_256_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::Reg(RegId(2)),
            Operand::MemSib { base: RegId(6), index: None, scale: Scale::X1, disp: 256 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov dl, [rsi + 256]");

    // Expected: 8A 96 00 01 00 00 (6 bytes, dl=RegId(2), so reg=010 in ModR/M)
    assert_eq!(buf.as_slice(), &[0x8A, 0x96, 0x00, 0x01, 0x00, 0x00]);
}

// W16 base+disp tests

#[test]
fn mov_ax_mem_rdi_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov ax, [rdi]");

    // Expected: 66 8B 07 (3 bytes, 0x66 prefix)
    assert_eq!(buf.as_slice(), &[0x66, 0x8B, 0x07]);
}

#[test]
fn mov_r8w_mem_rdi_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::Reg(RegId(8)),
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r8w, [rdi]");

    // Expected: 66 44 8B 07 (4 bytes, 0x66 prefix + REX.R)
    // r8w is RegId(8), so REX.R = (8 >> 3) = 1
    assert_eq!(buf.as_slice(), &[0x66, 0x44, 0x8B, 0x07]);
}

#[test]
fn mov_bx_mem_rbx_8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::Reg(RegId(3)),
            Operand::MemSib { base: RegId(3), index: None, scale: Scale::X1, disp: 8 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov bx, [rbx + 8]");

    // Expected: 66 8B 5B 08 (4 bytes, 0x66 prefix, bx=RegId(3), so reg=011 in ModR/M)
    assert_eq!(buf.as_slice(), &[0x66, 0x8B, 0x5B, 0x08]);
}

#[test]
fn mov_dx_mem_rsi_256_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::Reg(RegId(2)),
            Operand::MemSib { base: RegId(6), index: None, scale: Scale::X1, disp: 256 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov dx, [rsi + 256]");

    // Expected: 66 8B 96 00 01 00 00 (7 bytes, 0x66 prefix, dx=RegId(2), so reg=010 in ModR/M)
    assert_eq!(buf.as_slice(), &[0x66, 0x8B, 0x96, 0x00, 0x01, 0x00, 0x00]);
}

// W32 base+disp tests

#[test]
fn mov_eax_mem_rdi_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov eax, [rdi]");

    // Expected: 8B 07 (2 bytes, no REX.W)
    assert_eq!(buf.as_slice(), &[0x8B, 0x07]);
}

#[test]
fn mov_r8d_mem_rdi_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::Reg(RegId(8)),
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r8d, [rdi]");

    // Expected: 44 8B 07 (3 bytes, REX.R for r8d, no REX.W)
    // r8d is RegId(8), so REX.R = (8 >> 3) = 1
    assert_eq!(buf.as_slice(), &[0x44, 0x8B, 0x07]);
}

#[test]
fn mov_ebx_mem_rbx_8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::Reg(RegId(3)),
            Operand::MemSib { base: RegId(3), index: None, scale: Scale::X1, disp: 8 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov ebx, [rbx + 8]");

    // Expected: 8B 5B 08 (3 bytes, no REX.W, ebx=RegId(3), so reg=011 in ModR/M)
    assert_eq!(buf.as_slice(), &[0x8B, 0x5B, 0x08]);
}

#[test]
fn mov_edx_mem_rsi_256_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::Reg(RegId(2)),
            Operand::MemSib { base: RegId(6), index: None, scale: Scale::X1, disp: 256 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov edx, [rsi + 256]");

    // Expected: 8B 96 00 01 00 00 (6 bytes, no REX.W, edx=RegId(2), so reg=010 in ModR/M)
    assert_eq!(buf.as_slice(), &[0x8B, 0x96, 0x00, 0x01, 0x00, 0x00]);
}

// SIB tests (base + index*scale + disp)

#[test]
fn mov_al_mem_rdi_rcx_1_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(7), index: Some(RegId(1)), scale: Scale::X1, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov al, [rdi + rcx]");

    // Expected: 8A 04 0F (3 bytes, SIB with no scale)
    assert_eq!(buf.as_slice(), &[0x8A, 0x04, 0x0F]);
}

#[test]
fn mov_ax_mem_rsi_rdx_4_disp8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(6), index: Some(RegId(2)), scale: Scale::X4, disp: 8 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov ax, [rsi + rdx*4 + 8]");

    // Expected: 66 8B 44 96 08 (5 bytes, 0x66 prefix + SIB)
    assert_eq!(buf.as_slice(), &[0x66, 0x8B, 0x44, 0x96, 0x08]);
}

#[test]
fn mov_eax_mem_rbx_r9_8_disp32_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(3), index: Some(RegId(9)), scale: Scale::X8, disp: 256 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov eax, [rbx + r9*8 + 256]");

    // Expected: 42 8B 84 CB 00 01 00 00 (8 bytes, REX.X for r9, SIB with scale=8)
    assert_eq!(buf.as_slice(), &[0x42, 0x8B, 0x84, 0xCB, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn mov_r8b_mem_r10_r11_2_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::Reg(RegId(8)),
            Operand::MemSib { base: RegId(10), index: Some(RegId(11)), scale: Scale::X2, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r8b, [r10 + r11*2]");

    // Expected: 47 8A 04 5A (4 bytes, REX.R + REX.X + REX.B for extended regs)
    // r8b is RegId(8), so REX.R = 1
    // r11 is RegId(11), so REX.X = 1
    // r10 is RegId(10), so REX.B = 1
    assert_eq!(buf.as_slice(), &[0x47, 0x8A, 0x04, 0x5A]);
}

// W8 negative disp8

#[test]
fn mov_al_mem_rdi_neg8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: -8 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov al, [rdi - 8]");

    // Expected: 8A 47 F8 (8A = mov r8, r/m8; 47 = mod=01 disp8 rm=111; F8 = -8 as i8)
    assert_eq!(buf.as_slice(), &[0x8A, 0x47, 0xF8]);
}

// W16 RSP escape via SIB

#[test]
fn mov_ax_mem_rsp_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov ax, [rsp]");

    // Expected: 66 8B 04 24 (66 = operand-size override; 8B = mov r16, r/m16; 04 = mod=00 reg=000 rm=100; 24 = SIB scale=00 index=100 base=100)
    assert_eq!(buf.as_slice(), &[0x66, 0x8B, 0x04, 0x24]);
}

// W32 RBP escape (mod=01 with disp8)

#[test]
fn mov_eax_mem_rbp_8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 8 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov eax, [rbp + 8]");

    // Expected: 8B 45 08 (8B = mov r32, r/m32; 45 = mod=01 reg=000 rm=101; 08 = disp8)
    assert_eq!(buf.as_slice(), &[0x8B, 0x45, 0x08]);
}

// W64 via sized path (regression witness)

#[test]
fn mov_rax_mem_rdi_8_sized_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, [rdi + 8]");

    // Expected: 48 8B 47 08 (48 = REX.W; 8B = mov r64, r/m64; 47 = mod=01 reg=000 rm=111; 08 = disp8)
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x47, 0x08]);
}

// W8 RBP escape (mod=01 with disp8)

#[test]
fn mov_al_mem_rbp_8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: 8 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov al, [rbp + 8]");

    // Expected: 8A 45 08 (8A = mov r8, r/m8; 45 = mod=01 reg=000 rm=101; 08 = disp8)
    assert_eq!(buf.as_slice(), &[0x8A, 0x45, 0x08]);
}

// W16 RSP-as-base SIB with index*scale

#[test]
fn mov_ax_mem_rsp_rsi_2_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(4), index: Some(RegId(6)), scale: Scale::X2, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov ax, [rsp + rsi*2]");

    // Expected: 66 8B 04 74 (66 = operand-size override; 8B = mov r16, r/m16; 04 = mod=00 reg=000 rm=100; 74 = SIB scale=01 index=110 base=100)
    assert_eq!(buf.as_slice(), &[0x66, 0x8B, 0x04, 0x74]);
}

// W32 R13 escape (R13 is RegId(13) = 1101, requires REX.B)

#[test]
fn mov_eax_mem_r13_8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(13), index: None, scale: Scale::X1, disp: 8 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov eax, [r13 + 8]");

    // Expected: 41 8B 45 08 (41 = REX.B for r13; 8B = mov r32, r/m32; 45 = mod=01 reg=000 rm=101; 08 = disp8)
    assert_eq!(buf.as_slice(), &[0x41, 0x8B, 0x45, 0x08]);
}

// W64 via sized path with SIB index*scale (regression witness)

#[test]
fn mov_rax_mem_rdi_rsi_8_sized_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(7), index: Some(RegId(6)), scale: Scale::X8, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, [rdi + rsi*8]");

    // Expected: 48 8B 04 F7 (48 = REX.W; 8B = mov r64, r/m64; 04 = mod=00 reg=000 rm=100; F7 = SIB scale=11 index=110 base=111)
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x04, 0xF7]);
}

// ===== Suite B: iced-x86 Round-trip Validation =====

#[test]
fn mov_al_mem_rdi_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov al, [rdi]");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_ax_mem_rsi_rdx_4_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(6), index: Some(RegId(2)), scale: Scale::X4, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov ax, [rsi + rdx*4]");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_eax_mem_rbx_r9_8_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::MemSib { base: RegId(3), index: Some(RegId(9)), scale: Scale::X8, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov eax, [rbx + r9*8]");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_r8b_mem_r10_r11_2_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::Reg(RegId(8)),
            Operand::MemSib { base: RegId(10), index: Some(RegId(11)), scale: Scale::X2, disp: 0 }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r8b, [r10 + r11*2]");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}
