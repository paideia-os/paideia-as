//! Encoder tests for ljmp two-operand form (selector : offset).
//! Issue #896 (Phase 6 m6-001b): Parser surface for `ljmp selector : offset` syntax.
//!
//! Tests encoding of:
//! - `ljmp 0x08, symbol` (imm16, SymbolRef) with relocation
//! - `ljmp 0x08, 0x100000` (imm16, imm32) without relocation

use paideia_as_encoder::{CodeBuffer, EncodeStats, RelocKind};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use smallvec::smallvec;

/// PA10-006h Test: `ljmp 0x08, symbol` produces correct encoding with relocation.
///
/// ljmp imm16:imm32 encodes as:
/// - Opcode: 0xEA (1 byte)
/// - Imm32 (offset): 4 bytes (contains symbol relocation)
/// - Imm16 (selector): 2 bytes
/// Total: 7 bytes, relocation at byte offset 1 (immediately after opcode)
#[test]
fn ljmp_imm16_symbol_encodes_7_bytes() {
    let inst = Instruction {
        mnemonic: Mnemonic::FarJmp,
        operands: smallvec![
            Operand::Imm64(0x08), // selector (imm16)
            Operand::SymbolRef {
                name: "long_mode_entry".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed");

    assert_eq!(
        buf.as_slice().len(),
        7,
        "ljmp imm16:symbol should encode to 7 bytes"
    );
}

/// PA10-006h Test: ljmp opcode byte.
///
/// Verifies that the first byte is 0xEA (ljmp opcode).
#[test]
fn ljmp_imm16_symbol_opcode_is_ea() {
    let inst = Instruction {
        mnemonic: Mnemonic::FarJmp,
        operands: smallvec![
            Operand::Imm64(0x08),
            Operand::SymbolRef {
                name: "entry".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed");

    let bytes = buf.as_slice();
    assert!(!bytes.is_empty(), "buffer should have at least 1 byte");
    assert_eq!(bytes[0], 0xEA, "first byte should be 0xEA (ljmp opcode)");
}

/// PA10-006h Test: ljmp relocation offset.
///
/// The 4-byte imm32 offset field starts at byte 1 (right after 0xEA opcode).
/// Relocation should be at byte_offset = 1.
#[test]
fn ljmp_imm16_symbol_reloc_offset_is_1() {
    let inst = Instruction {
        mnemonic: Mnemonic::FarJmp,
        operands: smallvec![
            Operand::Imm64(0x08),
            Operand::SymbolRef {
                name: "target_symbol".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed");

    let relocs = &output.reloc_sites;
    assert_eq!(relocs.len(), 1, "should have exactly 1 relocation");

    let reloc = &relocs[0];
    assert_eq!(
        reloc.byte_offset, 1,
        "relocation should be at byte offset 1 (after 0xEA opcode)"
    );
}

/// PA10-006h Test: ljmp relocation kind and symbol.
///
/// ljmp uses R_X86_64_32 (Abs32) relocation, with symbol name and addend.
#[test]
fn ljmp_imm16_symbol_reloc_kind_is_abs32() {
    let inst = Instruction {
        mnemonic: Mnemonic::FarJmp,
        operands: smallvec![
            Operand::Imm64(0x08),
            Operand::SymbolRef {
                name: "long_mode_entry".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed");

    let relocs = &output.reloc_sites;
    assert_eq!(relocs.len(), 1);

    let reloc = &relocs[0];
    assert_eq!(
        reloc.kind,
        RelocKind::Abs32,
        "ljmp should use Abs32 relocation (R_X86_64_32)"
    );
    assert_eq!(
        reloc.symbol, "long_mode_entry",
        "relocation symbol should be 'long_mode_entry'"
    );
}

/// PA10-006h Test: `ljmp 0x08, 0x100000` with two immediates (no relocation).
///
/// When both operands are immediates, there should be no relocation.
/// Encoding remains 7 bytes (0xEA + 4-byte imm32 + 2-byte imm16).
#[test]
fn ljmp_imm16_imm32_encodes_7_bytes_no_reloc() {
    let inst = Instruction {
        mnemonic: Mnemonic::FarJmp,
        operands: smallvec![
            Operand::Imm64(0x08),     // selector (imm16)
            Operand::Imm64(0x100000), // offset (imm32)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed");

    assert_eq!(
        buf.as_slice().len(),
        7,
        "ljmp imm16:imm32 should encode to 7 bytes"
    );

    let relocs = &output.reloc_sites;
    assert_eq!(
        relocs.len(),
        0,
        "no relocation should be generated for two immediates"
    );
}

/// PA10-006h Test: ljmp selector value encoding.
///
/// The last 2 bytes should contain the selector value in little-endian.
/// For selector 0x08, bytes [5..7] should be [0x08, 0x00].
#[test]
fn ljmp_imm16_imm32_selector_encoding() {
    let inst = Instruction {
        mnemonic: Mnemonic::FarJmp,
        operands: smallvec![Operand::Imm64(0x08), Operand::Imm64(0),],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed");

    let bytes = buf.as_slice();
    assert_eq!(bytes.len(), 7, "should be 7 bytes");

    // Selector is at bytes [5..7] in little-endian
    assert_eq!(bytes[5], 0x08, "selector low byte should be 0x08");
    assert_eq!(bytes[6], 0x00, "selector high byte should be 0x00");
}

/// PA10-006h Test: ljmp relocation addend for Abs32.
///
/// Unlike PC-relative relocations (which apply -4 bias), Abs32 uses addend as-is.
/// IR addend 0 → reloc addend 0.
#[test]
fn ljmp_imm16_symbol_reloc_addend_is_unchanged() {
    let inst = Instruction {
        mnemonic: Mnemonic::FarJmp,
        operands: smallvec![
            Operand::Imm64(0x08),
            Operand::SymbolRef {
                name: "target".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed");

    let relocs = &output.reloc_sites;
    assert_eq!(relocs.len(), 1);
    assert_eq!(
        relocs[0].addend, 0,
        "ljmp: IR addend 0 should yield reloc addend 0"
    );
}

/// PA10-006h Test: ljmp relocation addend with non-zero IR addend.
///
/// Abs32 relocations preserve the addend as-is.
/// IR addend 8 → reloc addend 8.
#[test]
fn ljmp_imm16_symbol_reloc_addend_with_offset() {
    let inst = Instruction {
        mnemonic: Mnemonic::FarJmp,
        operands: smallvec![
            Operand::Imm64(0x08),
            Operand::SymbolRef {
                name: "target".to_string(),
                addend: 8,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding should succeed");

    let relocs = &output.reloc_sites;
    assert_eq!(relocs.len(), 1);
    assert_eq!(
        relocs[0].addend, 8,
        "ljmp: IR addend 8 should yield reloc addend 8"
    );
}
