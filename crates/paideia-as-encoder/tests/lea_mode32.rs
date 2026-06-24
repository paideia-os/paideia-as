//! Integration tests for 32-bit LEA with absolute addressing (PA15-m6-001c).
//!
//! Tests verify that 32-bit lea instructions with absolute 32-bit addressing are
//! correctly encoded with byte-exact validation against test vectors and iced-x86 round-trip.
//!
//! Suite A: Byte-exact encoding validation (2 test vectors from softarch).
//! Suite B: Relocation metadata verification (1 test for reloc kind, addend, byte_offset).
//! Suite C: iced-x86 round-trip validation.

use paideia_as_encoder::{CodeBuffer, EncodeStats, RelocKind};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (softarch test vectors) =====

/// Test 1: lea eax, [pml4] → 8D 05 00 00 00 00 (6 bytes, no REX.B for eax)
#[test]
fn lea_eax_pml4() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(0)), // eax (destination)
            Operand::SymbolRef {
                name: "pml4".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lea eax, [pml4]");

    // Expected: 8D 05 00 00 00 00 (6 bytes, no REX.B for eax)
    assert_eq!(buf.as_slice(), &[0x8D, 0x05, 0, 0, 0, 0]);

    // Expect 1 relocation site
    assert_eq!(output.reloc_sites.len(), 1);
}

/// Test 2: lea r8d, [pml4] → 41 8D 05 00 00 00 00 (7 bytes with REX.B for r8d)
#[test]
fn lea_r8d_pml4() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(8)), // r8d (destination, requires REX.B)
            Operand::SymbolRef {
                name: "pml4".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lea r8d, [pml4]");

    // Expected: 41 8D 05 00 00 00 00 (7 bytes with REX.B)
    assert_eq!(buf.as_slice(), &[0x41, 0x8D, 0x05, 0, 0, 0, 0]);

    // Expect 1 relocation site
    assert_eq!(output.reloc_sites.len(), 1);
}

// ===== Suite B: Relocation Metadata Verification =====

/// Test 3: Verify relocation metadata (kind, addend, byte_offset, symbol).
#[test]
fn lea_reloc_metadata_abs32() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(0)), // eax
            Operand::SymbolRef {
                name: "pml4".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed");

    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];

    // Verify relocation kind is Abs32 (not PcRel32)
    assert_eq!(reloc.kind, RelocKind::Abs32);

    // Verify addend is 0
    assert_eq!(reloc.addend, 0);

    // Verify byte_offset is 2 (disp32 starts at byte 2 for eax without REX)
    assert_eq!(reloc.byte_offset, 2);

    // Verify symbol name
    assert_eq!(reloc.symbol, "pml4");
}

// ===== Suite C: iced-x86 Round-trip Validation =====

/// Test 4: iced-x86 32-bit decoder round-trip for lea r32, [abs32].
/// This verifies the encoding is structurally correct and decodable.
#[test]
fn iced_x86_32bit_lea_decoder_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(0)), // eax
            Operand::SymbolRef {
                name: "pml4".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Decode the bytes with iced-x86 (32-bit mode for consistency)
    let mut decoder = Decoder::new(32, buf.as_slice(), DecoderOptions::NONE);
    let decoded_instr = decoder.decode();

    // Verify it decoded as a LEA instruction
    assert_eq!(decoded_instr.mnemonic(), IcedMnem::Lea);
}
