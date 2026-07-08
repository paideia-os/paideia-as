//! Integration tests for ENDBR64 and ENDBR32 (Intel CET) encoding (Phase R15 m4-005).
//!
//! Tests verify that ENDBR64 and ENDBR32 encode to the exact byte sequences:
//! - ENDBR64: F3 0F 1E FA (4 bytes) — End Branch 64-bit
//! - ENDBR32: F3 0F 1E FB (4 bytes) — End Branch 32-bit
//!
//! These are used as targets for indirect jumps/calls with Intel CET (Control Flow Enforcement).
//!
//! Test coverage:
//! 1. Byte-exact encoding: ENDBR64 → [0xF3, 0x0F, 0x1E, 0xFA]
//! 2. Byte-exact encoding: ENDBR32 → [0xF3, 0x0F, 0x1E, 0xFB]
//! 3. Mode-agnostic: both instructions identical in Mode32 and Mode64
//! 4. iced-x86 round-trip: decode should yield IcedMnem::Endbr64 or IcedMnem::Endbr32

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
fn endbr64_byte_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Endbr64,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    assert!(result.is_ok(), "ENDBR64 encoding should succeed");
    assert_eq!(
        buf.as_slice(),
        &[0xF3, 0x0F, 0x1E, 0xFA],
        "ENDBR64 should encode as F3 0F 1E FA"
    );
}

#[test]
fn endbr32_byte_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Endbr32,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    assert!(result.is_ok(), "ENDBR32 encoding should succeed");
    assert_eq!(
        buf.as_slice(),
        &[0xF3, 0x0F, 0x1E, 0xFB],
        "ENDBR32 should encode as F3 0F 1E FB"
    );
}

#[test]
fn endbr64_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Endbr64,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "ENDBR64 encoding differs between Mode32 and Mode64"
    );
    assert_eq!(
        mode32_bytes,
        &[0xF3, 0x0F, 0x1E, 0xFA],
        "ENDBR64 should encode as F3 0F 1E FA"
    );
}

#[test]
fn endbr32_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Endbr32,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "ENDBR32 encoding differs between Mode32 and Mode64"
    );
    assert_eq!(
        mode32_bytes,
        &[0xF3, 0x0F, 0x1E, 0xFB],
        "ENDBR32 should encode as F3 0F 1E FB"
    );
}

#[test]
fn endbr64_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Endbr64,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for endbr64");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Endbr64);
}

#[test]
fn endbr32_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Endbr32,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for endbr32");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Endbr32);
}
