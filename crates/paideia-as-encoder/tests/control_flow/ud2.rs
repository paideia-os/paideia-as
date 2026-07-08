//! Integration tests for UD2 (undefined instruction) encoding (Phase R13 m4-004).
//!
//! Tests verify that UD2 encodes to the exact byte sequence 0x0F 0x0B,
//! which triggers the undefined opcode exception (#UD).
//!
//! Test coverage:
//! 1. Byte-exact encoding: assert output == [0x0F, 0x0B]
//! 2. Mode-agnostic: UD2 is identical in Mode32 and Mode64
//! 3. iced-x86 round-trip: decode should yield IcedMnem::Ud2

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic};
use smallvec::smallvec;

/// Helper to encode an instruction in a given mode and return the bytes.
fn encode_in_mode(inst: &Instruction, mode: InstrMode) -> Vec<u8> {
    let mut inst = inst.clone();
    inst.mode = mode;
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
    buf.as_slice().to_vec()
}

#[test]
fn ud2_byte_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Ud2,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    assert!(result.is_ok(), "UD2 encoding should succeed");
    assert_eq!(
        buf.as_slice(),
        &[0x0F, 0x0B],
        "UD2 should encode as 0x0F 0x0B"
    );
}

#[test]
fn ud2_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Ud2,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "UD2 encoding differs between Mode32 and Mode64"
    );
    assert_eq!(mode32_bytes, &[0x0F, 0x0B], "UD2 should encode as 0x0F 0x0B");
}

#[test]
fn ud2_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Ud2,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for ud2");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Ud2);
}
