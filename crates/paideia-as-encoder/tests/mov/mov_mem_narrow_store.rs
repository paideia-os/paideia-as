//! Integration tests for narrow-form MOV [mem], imm encoding (PA-R14-001 #944).
//!
//! Tests verify that narrow MOV instructions with immediate sources are correctly encoded:
//! - mov_b [mem], imm8; mov_w [mem], imm16; mov_d [mem], imm32; mov_q [mem], imm32_sxt
//! - Both base+disp and SIB forms (base+index*scale+disp)
//!
//! Suite A: Byte-exact encoding validation (16 test vectors from softarch).
//! Suite B: iced-x86 round-trip validation (4 representative cases).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (softarch test vectors) =====

// W8 base+disp tests

#[test]
fn mov_b_mem_rdi_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(0x42)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_b [rdi], 0x42");

    // Expected: C6 07 42 (C6 = mov r/m8, imm8; 07 = mod=00 reg=000 rm=111; 42 = imm8)
    assert_eq!(buf.as_slice(), &[0xC6, 0x07, 0x42]);
}

#[test]
fn mov_b_mem_r10_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::MemSib { base: RegId(10), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(0x42)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_b [r10], 0x42");

    // Expected: 41 C6 02 42 (41 = REX.B for r10; C6 = mov r/m8, imm8; 02 = mod=00 reg=000 rm=010; 42 = imm8)
    assert_eq!(buf.as_slice(), &[0x41, 0xC6, 0x02, 0x42]);
}

// W16 base+disp tests

#[test]
fn mov_w_mem_rdi_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(0x1234)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_w [rdi], 0x1234");

    // Expected: 66 C7 07 34 12 (66 = operand-size override; C7 = mov r/m16, imm16; 07 = mod=00 reg=000 rm=111; 34 12 = imm16_le)
    assert_eq!(buf.as_slice(), &[0x66, 0xC7, 0x07, 0x34, 0x12]);
}

#[test]
fn mov_w_mem_rdi_8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 8 },
            Operand::Imm64(0x1234)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_w [rdi + 8], 0x1234");

    // Expected: 66 C7 47 08 34 12 (66 = operand-size override; C7 = mov r/m16, imm16; 47 = mod=01 reg=000 rm=111; 08 = disp8; 34 12 = imm16_le)
    assert_eq!(buf.as_slice(), &[0x66, 0xC7, 0x47, 0x08, 0x34, 0x12]);
}

// W32 base+disp tests

#[test]
fn mov_d_mem_rdi_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(0xDEADBEEF)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_d [rdi], 0xDEADBEEF");

    // Expected: C7 07 EF BE AD DE (C7 = mov r/m32, imm32; 07 = mod=00 reg=000 rm=111; EF BE AD DE = imm32_le)
    assert_eq!(buf.as_slice(), &[0xC7, 0x07, 0xEF, 0xBE, 0xAD, 0xDE]);
}

#[test]
fn mov_d_mem_rbp_minus_8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(5), index: None, scale: Scale::X1, disp: -8 },
            Operand::Imm64(0x00000001)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_d [rbp - 8], 1");

    // Expected: C7 45 F8 01 00 00 00 (C7 = mov r/m32, imm32; 45 = mod=01 reg=000 rm=101; F8 = disp8=-8; 01 00 00 00 = imm32_le)
    assert_eq!(buf.as_slice(), &[0xC7, 0x45, 0xF8, 0x01, 0x00, 0x00, 0x00]);
}

// W64 base+disp tests (sign-extended)

#[test]
fn mov_q_mem_rdi_0_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(0)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_q [rdi], 0");

    // Expected: 48 C7 07 00 00 00 00 (48 = REX.W; C7 = mov r/m64, imm32_sxt; 07 = mod=00 reg=000 rm=111; 00 00 00 00 = imm32_le)
    assert_eq!(buf.as_slice(), &[0x48, 0xC7, 0x07, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn mov_q_mem_rdi_neg1_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(-1_i64)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_q [rdi], -1");

    // Expected: 48 C7 07 FF FF FF FF (48 = REX.W; C7 = mov r/m64, imm32_sxt; 07 = mod=00 reg=000 rm=111; FF FF FF FF = -1 as i32_le)
    assert_eq!(buf.as_slice(), &[0x48, 0xC7, 0x07, 0xFF, 0xFF, 0xFF, 0xFF]);
}

// W8 SIB tests

#[test]
fn mov_b_mem_rdi_rcx_1_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: Some(RegId(1)), scale: Scale::X1, disp: 0 },
            Operand::Imm64(0x42)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_b [rdi + rcx], 0x42");

    // Expected: C6 04 0F 42 (C6 = mov r/m8, imm8; 04 = mod=00 reg=000 rm=100; 0F = SIB scale=00 index=001 base=111; 42 = imm8)
    assert_eq!(buf.as_slice(), &[0xC6, 0x04, 0x0F, 0x42]);
}

// W16 SIB tests

#[test]
fn mov_w_mem_rdi_rcx_2_disp8_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: Some(RegId(1)), scale: Scale::X2, disp: 8 },
            Operand::Imm64(0x1234)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_w [rdi + rcx*2 + 8], 0x1234");

    // Expected: 66 C7 44 4F 08 34 12 (66 = operand-size override; C7 = mov r/m16, imm16; 44 = mod=01 reg=000 rm=100; 4F = SIB scale=01 index=001 base=111; 08 = disp8; 34 12 = imm16_le)
    assert_eq!(buf.as_slice(), &[0x66, 0xC7, 0x44, 0x4F, 0x08, 0x34, 0x12]);
}

// W32 SIB tests

#[test]
fn mov_d_mem_rbx_r9_8_disp32_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(3), index: Some(RegId(9)), scale: Scale::X8, disp: 256 },
            Operand::Imm64(0xCAFEBABE)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_d [rbx + r9*8 + 256], 0xCAFEBABE");

    // Expected: 42 C7 84 CB 00 01 00 00 BE BA FE CA (42 = REX.X for r9; C7 = mov r/m32, imm32; 84 = mod=10 reg=000 rm=100; CB = SIB scale=11 index=001 base=011; 00 01 00 00 = disp32_le; BE BA FE CA = imm32_le)
    assert_eq!(buf.as_slice(), &[0x42, 0xC7, 0x84, 0xCB, 0x00, 0x01, 0x00, 0x00, 0xBE, 0xBA, 0xFE, 0xCA]);
}

// W64 SIB tests (sign-extended)

#[test]
fn mov_q_mem_rsp_rax_1_byte_exact() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: Some(RegId(0)), scale: Scale::X1, disp: 0 },
            Operand::Imm64(0x7FFFFFFF as i64)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_q [rsp + rax], 0x7FFFFFFF");

    // Expected: 48 C7 04 04 FF FF FF 7F (48 = REX.W; C7 = mov r/m64, imm32_sxt; 04 = mod=00 reg=000 rm=100; 04 = SIB scale=00 index=000 base=100; FF FF FF 7F = 0x7FFFFFFF as i32_le)
    assert_eq!(buf.as_slice(), &[0x48, 0xC7, 0x04, 0x04, 0xFF, 0xFF, 0xFF, 0x7F]);
}

// ===== Suite B: iced-x86 Round-trip Validation =====

#[test]
fn mov_b_mem_rdi_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: None, scale: Scale::X1, disp: 0 },
            Operand::Imm64(0x42)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_b [rdi], 0x42");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_w_mem_rdi_rcx_2_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: Some(RegId(1)), scale: Scale::X2, disp: 0 },
            Operand::Imm64(0x1234)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_w [rdi + rcx*2], 0x1234");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

#[test]
fn mov_d_mem_rbx_r9_8_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(3), index: Some(RegId(9)), scale: Scale::X8, disp: 0 },
            Operand::Imm64(0xCAFEBABE)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_d [rbx + r9*8], 0xCAFEBABE");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}

// ===== Suite C: #1269 — SIB-indexed register-source narrow stores =====
//
// Regression tests for the encoder gap where `mov [base + index*scale + disp], reg`
// with a NARROW (8/16/32-bit) register source was missing from encode_mov_sized —
// it fell through to the generic OperandShape handler and either rejected or
// silently emitted a 64-bit REX.W form.

#[test]
fn mov_d_mem_rax_rcx_4_edi_byte_exact_1269() {
    // `mov_d [rax + rcx*4], edi` — the exact reproducer from issue #1269.
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: Some(RegId(1)), scale: Scale::X4, disp: 0 },
            Operand::Reg(RegId(7))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_d [rax + rcx*4], edi");
    // Expected: 89 3C 88 — 32-bit store, no REX.W. Absence of 0x48 REX proves the fix.
    assert_eq!(buf.as_slice(), &[0x89, 0x3C, 0x88]);
}

#[test]
fn mov_d_mem_rax_rcx_4_r8d_byte_exact_1269() {
    // Extended-reg source needs REX.R.
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: Some(RegId(1)), scale: Scale::X4, disp: 0 },
            Operand::Reg(RegId(8))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_d [rax + rcx*4], r8d");
    // Expected: 44 89 04 88 — REX.R for r8, no REX.W.
    assert_eq!(buf.as_slice(), &[0x44, 0x89, 0x04, 0x88]);
}

#[test]
fn mov_w_mem_rax_rcx_4_di_byte_exact_1269() {
    // 16-bit narrow store: 66 prefix + no REX.
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W16 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: Some(RegId(1)), scale: Scale::X4, disp: 0 },
            Operand::Reg(RegId(7))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_w [rax + rcx*4], di");
    // Expected: 66 89 3C 88
    assert_eq!(buf.as_slice(), &[0x66, 0x89, 0x3C, 0x88]);
}

#[test]
fn mov_b_mem_rax_rcx_4_dil_byte_exact_1269() {
    // 8-bit narrow store: MANDATORY REX (dil requires REX.0 to disambiguate from ah/bh/ch/dh).
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W8 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: Some(RegId(1)), scale: Scale::X4, disp: 0 },
            Operand::Reg(RegId(7))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_b [rax + rcx*4], dil");
    // Expected: 40 88 3C 88 — REX.0 for dil (register id 7 in 4..=7 range).
    assert_eq!(buf.as_slice(), &[0x40, 0x88, 0x3C, 0x88]);
}

#[test]
fn mov_d_mem_rax_rcx_4_disp8_byte_exact_1269() {
    // With a small displacement: mod=01, disp8.
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: Some(RegId(1)), scale: Scale::X4, disp: 8 },
            Operand::Reg(RegId(7))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_d [rax + rcx*4 + 8], edi");
    // Expected: 89 7C 88 08
    assert_eq!(buf.as_slice(), &[0x89, 0x7C, 0x88, 0x08]);
}

#[test]
fn mov_d_mem_rax_rcx_4_edi_iced_round_trip_1269() {
    // iced-x86 cross-check: verify the encoded bytes decode back to MOV r/m32, r32.
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem, OpKind, Register};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: Some(RegId(1)), scale: Scale::X4, disp: 0 },
            Operand::Reg(RegId(7))
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
    };
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).unwrap();

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
    // Op0 must be memory (destination); op1 must be 32-bit register (source).
    assert_eq!(decoded.op0_kind(), OpKind::Memory);
    assert_eq!(decoded.op1_kind(), OpKind::Register);
    assert_eq!(decoded.op1_register(), Register::EDI);
    assert_eq!(decoded.memory_base(), Register::RAX);
    assert_eq!(decoded.memory_index(), Register::RCX);
    assert_eq!(decoded.memory_index_scale(), 4);
}

#[test]
fn mov_q_mem_rsp_rax_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: Some(RegId(0)), scale: Scale::X1, disp: 0 },
            Operand::Imm64(0)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov_q [rsp + rax], 0");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mov);
}
