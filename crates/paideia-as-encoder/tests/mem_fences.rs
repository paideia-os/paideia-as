//! Integration tests for memory fence instructions (MFENCE, SFENCE, LFENCE).
//!
//! Tests verify that memory fence instructions encode to exact byte sequences:
//! - MFENCE: 0F AE F0 (serializing memory barrier)
//! - SFENCE: 0F AE F8 (store fence barrier)
//! - LFENCE: 0F AE E8 (load fence barrier)
//!
//! Test coverage:
//! 1. MFENCE byte-exact encoding: assert output == [0x0F, 0xAE, 0xF0]
//! 2. SFENCE byte-exact encoding: assert output == [0x0F, 0xAE, 0xF8]
//! 3. LFENCE byte-exact encoding: assert output == [0x0F, 0xAE, 0xE8]
//! 4. Mode-agnostic: all three are identical in Mode32 and Mode64
//! 5. iced-x86 round-trip: decode should yield correct mnemonic for each fence

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

// MFENCE tests (PA-R13-005, issue #918)

#[test]
fn mfence_byte_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Mfence,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    assert!(result.is_ok(), "MFENCE encoding should succeed");
    assert_eq!(
        buf.as_slice(),
        &[0x0F, 0xAE, 0xF0],
        "MFENCE should encode as [0x0F, 0xAE, 0xF0]"
    );
}

#[test]
fn mfence_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Mfence,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "MFENCE encoding differs between Mode32 and Mode64"
    );
    assert_eq!(mode32_bytes, &[0x0F, 0xAE, 0xF0], "MFENCE should encode as [0x0F, 0xAE, 0xF0]");
}

#[test]
fn mfence_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Mfence,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mfence");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Mfence);
}

// SFENCE tests (PA-R14-004, issue #947)

#[test]
fn sfence_byte_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Sfence,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    assert!(result.is_ok(), "SFENCE encoding should succeed");
    assert_eq!(
        buf.as_slice(),
        &[0x0F, 0xAE, 0xF8],
        "SFENCE should encode as [0x0F, 0xAE, 0xF8]"
    );
}

#[test]
fn sfence_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Sfence,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "SFENCE encoding differs between Mode32 and Mode64"
    );
    assert_eq!(mode32_bytes, &[0x0F, 0xAE, 0xF8], "SFENCE should encode as [0x0F, 0xAE, 0xF8]");
}

#[test]
fn sfence_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Sfence,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for sfence");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Sfence);
}

// LFENCE tests (PA-R14-004, issue #947)

#[test]
fn lfence_byte_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Lfence,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    assert!(result.is_ok(), "LFENCE encoding should succeed");
    assert_eq!(
        buf.as_slice(),
        &[0x0F, 0xAE, 0xE8],
        "LFENCE should encode as [0x0F, 0xAE, 0xE8]"
    );
}

#[test]
fn lfence_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Lfence,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "LFENCE encoding differs between Mode32 and Mode64"
    );
    assert_eq!(mode64_bytes, &[0x0F, 0xAE, 0xE8], "LFENCE should encode as [0x0F, 0xAE, 0xE8]");
}

#[test]
fn lfence_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Lfence,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lfence");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Lfence);
}
