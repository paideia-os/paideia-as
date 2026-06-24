//! Integration tests for or r32, imm32 with sign-bit-set immediates (PA15-m6-001e).
//!
//! Tests verify that or instructions with 32-bit immediates that have the sign bit set
//! are correctly encoded, accepting values in both signed i32 and unsigned u32 ranges.
//!
//! Suite A: Byte-exact encoding validation for sign-bit-set values.
//! Suite B: Round-trip validation via iced-x86.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding with Sign-Bit-Set Immediates =====

/// Test 1: or eax, 0x80000001 (sign bit set) → 81 C8 01 00 00 80 (LE bytes).
#[test]
fn or_eax_signbit_0x80000001() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![
            Operand::Reg(RegId(0)),               // eax
            Operand::Imm64(0x80000001u32 as i64), // Sign-bit-set value
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed for 0x80000001");

    // Expected: 81 C8 01 00 00 80 (opcode 81, modrm C8, imm32 in LE)
    // imm32 0x80000001 in LE: 01 00 00 80
    let expected = &[0x81, 0xC8, 0x01, 0x00, 0x00, 0x80];
    assert_eq!(buf.as_slice(), expected);
}

/// Test 2: or eax, 0xFFFFFFFF (all bits set) → 81 C8 FF FF FF FF.
#[test]
fn or_eax_signbit_0xFFFFFFFF() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![
            Operand::Reg(RegId(0)),               // eax
            Operand::Imm64(0xFFFFFFFFu32 as i64), // Max u32
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed for 0xFFFFFFFF");

    // Expected: 81 C8 FF FF FF FF
    let expected = &[0x81, 0xC8, 0xFF, 0xFF, 0xFF, 0xFF];
    assert_eq!(buf.as_slice(), expected);
}

/// Test 3: or ecx, 0x80000000 (sign bit boundary) → 81 C9 00 00 00 80.
#[test]
fn or_ecx_signbit_boundary() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![
            Operand::Reg(RegId(1)), // ecx
            Operand::Imm64(0x80000000u32 as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed for 0x80000000");

    // Expected: 81 C9 00 00 00 80 (modrm C9 for ecx)
    let expected = &[0x81, 0xC9, 0x00, 0x00, 0x00, 0x80];
    assert_eq!(buf.as_slice(), expected);
}

/// Test 4: or edx, 0x7FFFFFFF (signed max, no sign bit) → 81 CA FF FF FF 7F.
#[test]
fn or_edx_signed_max_no_signbit() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![
            Operand::Reg(RegId(2)), // edx
            Operand::Imm64(0x7FFFFFFFu32 as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed for 0x7FFFFFFF");

    // Expected: 81 CA FF FF FF 7F (modrm CA for edx)
    let expected = &[0x81, 0xCA, 0xFF, 0xFF, 0xFF, 0x7F];
    assert_eq!(buf.as_slice(), expected);
}

// ===== Suite B: iced-x86 Round-trip Validation =====

/// Test 5: iced-x86 32-bit decoder round-trip for or r32, imm32(0x80000001).
#[test]
fn iced_x86_or_r32_imm32_signbit_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![
            Operand::Reg(RegId(0)), // eax
            Operand::Imm64(0x80000001u32 as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Decode with iced-x86
    let mut decoder = Decoder::new(32, buf.as_slice(), DecoderOptions::NONE);
    let decoded_instr = decoder.decode();

    // Verify it decoded as an OR instruction
    assert_eq!(decoded_instr.mnemonic(), IcedMnem::Or);
}

/// Test 6: iced-x86 round-trip for or r32, imm32(0xFFFFFFFF).
#[test]
fn iced_x86_or_r32_imm32_max_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Or,
        operands: smallvec![
            Operand::Reg(RegId(0)), // eax
            Operand::Imm64(0xFFFFFFFFu32 as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(32, buf.as_slice(), DecoderOptions::NONE);
    let decoded_instr = decoder.decode();

    assert_eq!(decoded_instr.mnemonic(), IcedMnem::Or);
}
