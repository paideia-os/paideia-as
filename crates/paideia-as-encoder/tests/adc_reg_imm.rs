//! Tests for ADC register + immediate encoders (PA-R16-007).
//! Mirrors the pattern from issue #1060 (add_reg_imm tests).

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand, RegId};
use smallvec::smallvec;

/// Helper to encode ADC instruction and return bytes.
fn encode_adc_inst(
    mnemonic: Mnemonic,
    operands: smallvec::SmallVec<[Operand; 3]>,
) -> Vec<u8> {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic,
        operands,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for adc");

    buf.as_slice().to_vec()
}

#[test]
fn test_adc_rax_imm8() {
    // `adc rax, 5` (W64, imm8) → REX.W 83 D0 05
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W64,
        },
        smallvec![
            Operand::Reg(RegId(0)), // rax
            Operand::Imm64(5),
        ],
    );
    assert_eq!(bytes, vec![0x48, 0x83, 0xD0, 0x05]);
}

#[test]
fn test_adc_rax_imm8_negative() {
    // `adc rax, -5` (W64, imm8 negative) → REX.W 83 D0 FB
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W64,
        },
        smallvec![
            Operand::Reg(RegId(0)), // rax
            Operand::Imm64(-5),
        ],
    );
    assert_eq!(bytes, vec![0x48, 0x83, 0xD0, 0xFB]);
}

#[test]
fn test_adc_rax_imm32() {
    // `adc rax, 0x1000` (W64, imm32) → REX.W 81 D0 00 10 00 00 (little-endian)
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W64,
        },
        smallvec![
            Operand::Reg(RegId(0)), // rax
            Operand::Imm64(0x1000),
        ],
    );
    assert_eq!(bytes, vec![0x48, 0x81, 0xD0, 0x00, 0x10, 0x00, 0x00]);
}

#[test]
fn test_adc_r10_imm8() {
    // `adc r10, 5` (W64, extended reg + imm8) → REX.W,B 83 D2 05
    // R10 is register id 10 (0xA), so REX.B = 0x49
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W64,
        },
        smallvec![
            Operand::Reg(RegId(10)), // r10
            Operand::Imm64(5),
        ],
    );
    assert_eq!(bytes, vec![0x49, 0x83, 0xD2, 0x05]);
}

#[test]
fn test_adc_eax_imm8() {
    // `adc eax, 5` (W32, imm8) → 83 D0 05 (no REX.W, no REX.B)
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W32,
        },
        smallvec![
            Operand::Reg(RegId(0)), // eax
            Operand::Imm64(5),
        ],
    );
    assert_eq!(bytes, vec![0x83, 0xD0, 0x05]);
}

#[test]
fn test_adc_eax_imm32() {
    // `adc eax, 0x1000` (W32, imm32) → 81 D0 00 10 00 00
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W32,
        },
        smallvec![
            Operand::Reg(RegId(0)), // eax
            Operand::Imm64(0x1000),
        ],
    );
    assert_eq!(bytes, vec![0x81, 0xD0, 0x00, 0x10, 0x00, 0x00]);
}

#[test]
fn test_adc_r15d_imm8() {
    // `adc r15d, 5` (W32, extended reg + imm8) → REX.B 83 D7 05
    // R15 is register id 15 (0xF), so REX.B = 0x41
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W32,
        },
        smallvec![
            Operand::Reg(RegId(15)), // r15
            Operand::Imm64(5),
        ],
    );
    assert_eq!(bytes, vec![0x41, 0x83, 0xD7, 0x05]);
}

#[test]
fn test_adc_rcx_imm8() {
    // `adc rcx, 10` (W64, imm8) → 48 83 D1 0A
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W64,
        },
        smallvec![
            Operand::Reg(RegId(1)), // rcx
            Operand::Imm64(10),
        ],
    );
    assert_eq!(bytes, vec![0x48, 0x83, 0xD1, 0x0A]);
}

#[test]
fn test_adc_rdx_imm32() {
    // `adc rdx, -1000` (W64, imm32 negative) → 48 81 D2 18 FC FF FF
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W64,
        },
        smallvec![
            Operand::Reg(RegId(2)), // rdx
            Operand::Imm64(-1000),
        ],
    );
    // -1000 in i32 little-endian: 0xFFFFFC18
    assert_eq!(bytes, vec![0x48, 0x81, 0xD2, 0x18, 0xFC, 0xFF, 0xFF]);
}

#[test]
fn test_adc_r8_imm32_w64() {
    // `adc r8, 0x100000` (W64, extended reg + imm32)
    // R8 is register id 8, so REX.W,B = 0x49
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W64,
        },
        smallvec![
            Operand::Reg(RegId(8)), // r8
            Operand::Imm64(0x100000),
        ],
    );
    // 0x100000 in little-endian: 0x00 0x00 0x10 0x00
    assert_eq!(bytes, vec![0x49, 0x81, 0xD0, 0x00, 0x00, 0x10, 0x00]);
}

#[test]
fn test_adc_r8d_imm32_w32() {
    // `adc r8d, 0x100000` (W32, extended reg + imm32)
    // R8 is register id 8, so REX.B = 0x41
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W32,
        },
        smallvec![
            Operand::Reg(RegId(8)), // r8
            Operand::Imm64(0x100000),
        ],
    );
    assert_eq!(bytes, vec![0x41, 0x81, 0xD0, 0x00, 0x00, 0x10, 0x00]);
}

#[test]
fn test_adc_rbx_imm8() {
    // `adc rbx, 1` (W64, imm8) → 48 83 D3 01
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W64,
        },
        smallvec![
            Operand::Reg(RegId(3)), // rbx
            Operand::Imm64(1),
        ],
    );
    assert_eq!(bytes, vec![0x48, 0x83, 0xD3, 0x01]);
}

#[test]
fn test_adc_rsi_imm32() {
    // `adc rsi, 0x7FFFFFFF` (W64, max i32) → 48 81 D6 FF FF FF 7F
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W64,
        },
        smallvec![
            Operand::Reg(RegId(6)), // rsi
            Operand::Imm64(i32::MAX as i64),
        ],
    );
    assert_eq!(bytes, vec![0x48, 0x81, 0xD6, 0xFF, 0xFF, 0xFF, 0x7F]);
}

#[test]
fn test_adc_ebx_imm8() {
    // `adc ebx, 127` (W32, max i8) → 83 D3 7F
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W32,
        },
        smallvec![
            Operand::Reg(RegId(3)), // ebx
            Operand::Imm64(127),
        ],
    );
    assert_eq!(bytes, vec![0x83, 0xD3, 0x7F]);
}

#[test]
fn test_adc_esi_imm32() {
    // `adc esi, 0x7FFFFFFF` (W32, max i32)
    let bytes = encode_adc_inst(
        Mnemonic::Adc {
            width: IntWidth::W32,
        },
        smallvec![
            Operand::Reg(RegId(6)), // esi
            Operand::Imm64(i32::MAX as i64),
        ],
    );
    assert_eq!(bytes, vec![0x81, 0xD6, 0xFF, 0xFF, 0xFF, 0x7F]);
}

