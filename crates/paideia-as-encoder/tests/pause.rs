//! Integration tests for pause instruction (spinloop hint).
//!
//! Tests verify that pause instruction encodes to exact byte sequence:
//! - PAUSE: F3 90 (2 bytes, spinloop hint per Intel SDM Vol 2A)
//!
//! Test coverage:
//! 1. PAUSE byte-exact encoding: assert output == [0xF3, 0x90]
//! 2. PAUSE iced-x86 round-trip: decode should yield correct mnemonic
//! 3. PAUSE rejects operands: assert error on non-zero operand count

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

#[test]
fn pause_byte_exact_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::Pause,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    assert!(result.is_ok(), "PAUSE encoding should succeed");
    assert_eq!(
        buf.as_slice(),
        &[0xF3, 0x90],
        "PAUSE should encode as [0xF3, 0x90]"
    );
}

#[test]
fn pause_iced_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let inst = Instruction {
        mnemonic: Mnemonic::Pause,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for pause");

    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    assert_eq!(decoded.mnemonic(), IcedMnem::Pause);
    assert_eq!(decoded.op_count(), 0);
}

#[test]
fn pause_rejects_operand() {
    use paideia_as_encoder::EncodeError;

    let inst = Instruction {
        mnemonic: Mnemonic::Pause,
        operands: smallvec![Operand::Reg(RegId(0))],  // rax
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    assert!(result.is_err(), "PAUSE with operand should fail");
    match result {
        Err(EncodeError::OperandCount { mnemonic, expected, got }) => {
            assert_eq!(mnemonic, Mnemonic::Pause);
            assert_eq!(expected, 0);
            assert_eq!(got, 1);
        }
        _ => panic!("Expected OperandCount error"),
    }
}
