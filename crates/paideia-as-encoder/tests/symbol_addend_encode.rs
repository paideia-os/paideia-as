//! Integration tests for symbol memory operands with addends (PA15-m6-001d).
//!
//! Tests verify that instructions with [symbol + N] memory operands are correctly
//! encoded with the appropriate relocation information and byte-exact output.
//!
//! Suite A: mov [symbol + addend], imm32 with various addend values.
//! Suite B: Relocation metadata verification (kind, byte_offset, addend).

use paideia_as_encoder::{CodeBuffer, EncodeStats, RelocKind};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding with Symbol Addends =====

/// Test 1: mov [pml4 + 4], imm32(0) → C7 05 ... 00 00 00 00 + reloc with addend 4.
#[test]
fn mov_mem_symbol_plus_4_imm32_zero() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "pml4".to_string(),
                addend: 4,
            },
            Operand::Imm64(0),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [pml4 + 4], 0");

    // Expected: C7 05 00 00 00 00 00 00 00 00 (10 bytes)
    let expected = &[0xC7, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(buf.as_slice(), expected);

    // Verify relocation with addend
    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.symbol, "pml4");
    assert_eq!(reloc.kind, RelocKind::Abs32);
    assert_eq!(reloc.addend, 4);
    assert_eq!(reloc.byte_offset, 2);
}

/// Test 2: mov [pdpt + 16], imm32(0x1003) → C7 05 ... 03 10 00 00 + reloc with addend 16.
#[test]
fn mov_mem_symbol_plus_16_imm32() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "pdpt".to_string(),
                addend: 16,
            },
            Operand::Imm64(0x1003),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [pdpt + 16], 0x1003");

    // Expected: C7 05 00 00 00 00 03 10 00 00
    let expected = &[0xC7, 0x05, 0x00, 0x00, 0x00, 0x00, 0x03, 0x10, 0x00, 0x00];
    assert_eq!(buf.as_slice(), expected);

    // Verify relocation with addend
    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.symbol, "pdpt");
    assert_eq!(reloc.kind, RelocKind::Abs32);
    assert_eq!(reloc.addend, 16);
}

// ===== Suite B: Symbol Addend Relocation Verification =====

/// Test 3: Verify symbol addend propagates correctly through relocation metadata.
#[test]
fn symbol_addend_reloc_metadata() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "kernel_data".to_string(),
                addend: 8,
            },
            Operand::Imm64(0xFFFFFFFF as i64),
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

    assert_eq!(reloc.symbol, "kernel_data");
    assert_eq!(reloc.kind, RelocKind::Abs32);
    assert_eq!(reloc.addend, 8, "addend should be preserved in relocation");
    assert_eq!(reloc.byte_offset, 2);
}

/// Test 4: mov [symbol + large_addend], imm32 → relocation with large addend.
#[test]
fn symbol_addend_large_value() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "page_table".to_string(),
                addend: 4096, // Large addend
            },
            Operand::Imm64(0x42),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for large addend");

    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.addend, 4096);
}

/// Test 5: mov [symbol - offset], imm32 → relocation with negative addend.
#[test]
fn symbol_minus_offset_encoding() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "data_section".to_string(),
                addend: -8, // Negative addend from [sym - 8]
            },
            Operand::Imm64(0x55),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for negative addend");

    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.symbol, "data_section");
    assert_eq!(reloc.addend, -8);
}
