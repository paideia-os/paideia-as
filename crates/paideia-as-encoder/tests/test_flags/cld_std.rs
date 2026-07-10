//! Integration tests for CLD/STD (direction flag) encoding (Phase R13 m4-004).
//!
//! Tests verify that CLD and STD encode to exact byte sequences (0xFC and 0xFD),
//! which clear and set the direction flag respectively.
//!
//! Test coverage:
//! 1. CLD byte-exact encoding: assert output == [0xFC]
//! 2. STD byte-exact encoding: assert output == [0xFD]
//! 3. Mode-agnostic: both are identical in Mode32 and Mode64
//! 4. iced-x86 round-trip: decode should yield IcedMnem::Cld / IcedMnem::Std

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
fn cld_byte_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Cld,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    assert!(result.is_ok(), "CLD encoding should succeed");
    assert_eq!(
        buf.as_slice(),
        &[0xFC],
        "CLD should encode as 0xFC"
    );
}

#[test]
fn std_byte_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Std,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    assert!(result.is_ok(), "STD encoding should succeed");
    assert_eq!(
        buf.as_slice(),
        &[0xFD],
        "STD should encode as 0xFD"
    );
}

#[test]
fn cld_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Cld,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "CLD encoding differs between Mode32 and Mode64"
    );
    assert_eq!(mode32_bytes, &[0xFC], "CLD should encode as 0xFC");
}

#[test]
fn std_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Std,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "STD encoding differs between Mode32 and Mode64"
    );
    assert_eq!(mode64_bytes, &[0xFD], "STD should encode as 0xFD");
}

#[test]
fn cld_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Cld,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for cld");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Cld);
}

#[test]
fn std_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Std,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
        emission_order: 0,
};

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for std");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Std);
}
