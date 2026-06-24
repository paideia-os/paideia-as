//! Integration tests for IMUL encoding (PA10-003).
//!
//! Tests verify that IMUL instructions are correctly encoded with byte-exact validation
//! against NASM-verified test vectors. Tests cover:
//! - 2-operand form: imul r64, r64
//! - 3-operand form: imul r64, r64, imm (with sign-extension trap guards for imm8/imm32)
//! - mem,mem case: verifies proper error ("imul mem,mem")

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::InstrMode;
use smallvec::smallvec;

// ===== 2-operand IMUL Tests =====

#[test]
fn imul_rax_rbx_reg_reg_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Imul,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))], // rax, rbx
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for imul rax, rbx");

    // Expected: 48 0F AF C3 (4 bytes)
    // IMUL inverted: dst (rax) in reg, src (rbx) in r/m
    assert_eq!(buf.as_slice(), &[0x48, 0x0f, 0xaf, 0xc3]);
}

// ===== 3-operand IMUL Tests =====

#[test]
fn imul_rax_rbx_5_imm8_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Imul,
        operands: smallvec![
            Operand::Reg(RegId(0)), // rax (dst)
            Operand::Reg(RegId(3)), // rbx (src)
            Operand::Imm64(5),      // 5
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for imul rax, rbx, 5");

    // Expected: 48 6B C3 05 (4 bytes, imm8 form)
    assert_eq!(buf.as_slice(), &[0x48, 0x6b, 0xc3, 0x05]);
}

#[test]
fn imul_rax_rbx_0x1000_imm32_encodes_correctly() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Imul,
        operands: smallvec![
            Operand::Reg(RegId(0)), // rax (dst)
            Operand::Reg(RegId(3)), // rbx (src)
            Operand::Imm64(0x1000), // 0x1000
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for imul rax, rbx, 0x1000");

    // Expected: 48 69 C3 00 10 00 00 (7 bytes, imm32 form)
    assert_eq!(buf.as_slice(), &[0x48, 0x69, 0xc3, 0x00, 0x10, 0x00, 0x00]);
}

#[test]
fn imul_sign_ext_trap_rejects_out_of_range() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Imul,
        operands: smallvec![
            Operand::Reg(RegId(0)),      // rax
            Operand::Reg(RegId(3)),      // rbx
            Operand::Imm64(0x100000001), // out of i32 range
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err());
}

// ===== Error case: mem,mem =====

#[test]
fn imul_mem_mem_rejects_with_correct_error() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Imul,
        operands: smallvec![
            Operand::MemSib {
                base: RegId(5), // rbp
                index: None,
                scale: paideia_as_ir::Scale::X1,
                disp: 0,
            },
            Operand::MemSib {
                base: RegId(3), // rbx
                index: None,
                scale: paideia_as_ir::Scale::X1,
                disp: 0,
            },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);
    assert!(result.is_err());
    match result {
        Err(paideia_as_encoder::EncodeError::Unsupported(msg)) => {
            assert_eq!(msg, "imul mem,mem");
        }
        _ => panic!("Expected Unsupported error with message 'imul mem,mem'"),
    }
}

// ===== Round-trip tests with iced-x86 =====

#[test]
fn imul_rax_rbx_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Imul,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Imul);
}

#[test]
fn imul_rax_rbx_5_round_trips_through_iced_x86() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Imul,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::Reg(RegId(3)),
            Operand::Imm64(5),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Imul);
}
