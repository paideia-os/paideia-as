//! Tests for `vmovdqu ymm dst/[mem], ymm/[mem] src` encoding — Phase R18 PA-R18-011 (issue #1004).
//! Encoding: VEX F3 0F 6F (load) / 7F (store) /r

use paideia_as_encoder::{CodeBuffer, encode_instruction};
use paideia_as_ir::{Instruction, Mnemonic, Operand, RegId, InstrMode, Scale};
use smallvec::smallvec;

/// Test `vmovdqu ymm0, [rax]` → `C5 FE 6F 00` (load form)
#[test]
fn vmovdqu_ymm0_mem_rax_emits_c5_fe_6f_00() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vmovdqu { is_store: false },
        operands: smallvec![
            Operand::Reg(RegId(37)),
            Operand::MemSib {
                base: RegId(0),
                index: None,
                scale: Scale::X1,
                disp: 0,
            }
        ],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.bytes, vec![0xC5, 0xFE, 0x6F, 0x00]);
}

/// Test `vmovdqu ymm0, [rax + 0x40]` (load with disp8)
#[test]
fn vmovdqu_ymm0_mem_rax_disp8_load() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vmovdqu { is_store: false },
        operands: smallvec![
            Operand::Reg(RegId(37)),
            Operand::MemSib {
                base: RegId(0),
                index: None,
                scale: Scale::X1,
                disp: 0x40,
            }
        ],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // C5 FE 6F 40 40 (ModR/M with disp8)
    assert_eq!(buf.bytes[0], 0xC5);
    assert_eq!(buf.bytes[1], 0xFE);
    assert_eq!(buf.bytes[2], 0x6F);
    assert_eq!(buf.bytes[3], 0x40); // ModR/M with mod=01
    assert_eq!(buf.bytes[4], 0x40); // disp8
}

/// Test `vmovdqu ymm0, [rsp]` (load, SIB escape for rsp)
#[test]
fn vmovdqu_ymm0_mem_rsp_load() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vmovdqu { is_store: false },
        operands: smallvec![
            Operand::Reg(RegId(37)),
            Operand::MemSib {
                base: RegId(4), // rsp
                index: None,
                scale: Scale::X1,
                disp: 0,
            }
        ],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // rsp (RegId 4) requires SIB byte; verify we emit the instruction
    assert!(buf.bytes.len() > 0);
    assert_eq!(buf.bytes[0], 0xC5);
}

/// Test `vmovdqu ymm0, [r8]` (load, VEX.B for base-high)
#[test]
fn vmovdqu_ymm0_mem_r8_load() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vmovdqu { is_store: false },
        operands: smallvec![
            Operand::Reg(RegId(37)),
            Operand::MemSib {
                base: RegId(8), // r8
                index: None,
                scale: Scale::X1,
                disp: 0,
            }
        ],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    // Should use 3-byte VEX due to high base register
    assert_eq!(buf.bytes[0], 0xC4);
}

/// Test `vmovdqu [rax], ymm0` → `C5 FE 7F 00` (store form)
#[test]
fn vmovdqu_mem_rax_ymm0_emits_c5_fe_7f_00() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vmovdqu { is_store: true },
        operands: smallvec![
            Operand::MemSib {
                base: RegId(0),
                index: None,
                scale: Scale::X1,
                disp: 0,
            },
            Operand::Reg(RegId(37)),
        ],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    assert_eq!(buf.bytes, vec![0xC5, 0xFE, 0x7F, 0x00]);
}

/// Test `vmovdqu [rax + rcx*4], ymm0` (store with SIB index)
#[test]
fn vmovdqu_mem_rax_index_ymm0_store() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vmovdqu { is_store: true },
        operands: smallvec![
            Operand::MemSib {
                base: RegId(0),
                index: Some(RegId(1)), // rcx
                scale: Scale::X4,
                disp: 0,
            },
            Operand::Reg(RegId(37)),
        ],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    // This tests that the encoder rejects non-simple MemSib forms
    // For now, we only support base + disp (no index)
    let result = encode_instruction(&inst, &mut buf, &mut stats);
    // Should return an error because we don't support index yet
    assert!(result.is_err());
}

/// Test iced round-trip: `vmovdqu ymm3, [rbx + 0x20]` (load form)
#[test]
fn vmovdqu_ymm3_mem_rbx_disp_round_trips_iced() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Vmovdqu { is_store: false },
        operands: smallvec![
            Operand::Reg(RegId(40)),
            Operand::MemSib {
                base: RegId(3), // rbx
                index: None,
                scale: Scale::X1,
                disp: 0x20,
            }
        ],
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        emission_order: 1,
    };
    let mut stats = paideia_as_encoder::EncodeStats::new();
    encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    let mut decoder = Decoder::new(64, &buf.bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert_eq!(instr.mnemonic(), IcedMnem::Vmovdqu);
    assert_eq!(instr.op_count(), 2);
}
