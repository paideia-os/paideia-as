//! Integration tests for 32-bit MOV with absolute addressing (PA15-m3-002).
//!
//! Tests verify that 32-bit mov instructions with absolute 32-bit addressing are
//! correctly encoded with byte-exact validation against test vectors and iced-x86 round-trip.
//!
//! Suite A: Byte-exact encoding validation (2 test vectors from softarch).
//! Suite B: Relocation metadata verification (1 test for reloc kind, addend, byte_offset).
//! Suite C: Boot stub witness bytes and round-trip via iced-x86.

use paideia_as_encoder::{CodeBuffer, EncodeStats, RelocKind};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

// ===== Suite A: Byte-Exact Encoding (softarch test vectors) =====

/// Test 1: movl pml4, %eax (load) → bytes [0x8B, 0x05, 0, 0, 0, 0] + 1 reloc.
#[test]
fn movl_pml4_eax_load() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
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
        .expect("encoding failed for mov eax, [pml4]");

    // Expected: 8B 05 00 00 00 00 (6 bytes, no REX.B for eax)
    assert_eq!(buf.as_slice(), &[0x8B, 0x05, 0, 0, 0, 0]);

    // Expect 1 relocation site
    assert_eq!(output.reloc_sites.len(), 1);
}

/// Test 2: movl %eax, pml4 (store) → bytes [0x89, 0x05, 0, 0, 0, 0] + 1 reloc.
#[test]
fn movl_eax_pml4_store() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "pml4".to_string(),
                addend: 0,
            },
            Operand::Reg(RegId(0)), // eax (source)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [pml4], eax");

    // Expected: 89 05 00 00 00 00 (6 bytes, no REX.B for eax)
    assert_eq!(buf.as_slice(), &[0x89, 0x05, 0, 0, 0, 0]);

    // Expect 1 relocation site
    assert_eq!(output.reloc_sites.len(), 1);
}

// ===== Suite B: Relocation Metadata Verification =====

/// Test 3: Verify relocation metadata (kind, addend, byte_offset, symbol).
#[test]
fn reloc_metadata_abs32() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
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

// ===== Suite C: Boot Stub Witness Bytes and iced-x86 Round-trip =====

/// Test 4: Boot stub witness pattern: mov eax, 0x00000000 (B8) + mov [pml4], eax (89 05).
/// This demonstrates the sequence used in real boot stub code before relocation.
#[test]
fn boot_stub_witness_bytes() {
    let mut buf = CodeBuffer::new();

    // First: mov eax, 0x00000000 → B8 00 00 00 00
    let inst1 = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)), // eax
            Operand::Imm64(0),      // imm32 = 0
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst1, &mut buf, &mut stats)
        .expect("encoding failed for mov eax, 0");

    // Second: mov [pml4], eax → 89 05 00 00 00 00
    let inst2 = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "pml4".to_string(),
                addend: 0,
            },
            Operand::Reg(RegId(0)), // eax
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    paideia_as_encoder::encode_instruction(&inst2, &mut buf, &mut stats)
        .expect("encoding failed for mov [pml4], eax");

    // Expected concatenated bytes: B8 00 00 00 00 89 05 00 00 00 00
    let expected = &[
        0xB8, 0x00, 0x00, 0x00, 0x00, 0x89, 0x05, 0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(buf.as_slice(), expected);
}

/// Test 5: iced-x86 32-bit decoder round-trip for mov r32, [abs32].
/// This verifies the encoding is structurally correct and decodable.
#[test]
fn iced_x86_32bit_decoder_round_trip() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
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

    // Verify it decoded as a MOV instruction
    assert_eq!(decoded_instr.mnemonic(), IcedMnem::Mov);
}

// ===== Suite D: Phase 15 m3-003 Tests (mov [abs32], imm32) =====

/// Test 1: mov [pdpt], imm32(0) → C7 05 00 00 00 00 00 00 00 00 + reloc with addend 0.
#[test]
fn mov_mem_abs32_zero_imm() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "pdpt".to_string(),
                addend: 0,
            },
            Operand::Imm64(0),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [pdpt], 0");

    // Expected: C7 05 00 00 00 00 00 00 00 00 (10 bytes: opcode, ModR/M, disp32 placeholder, imm32)
    let expected = &[0xC7, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(buf.as_slice(), expected);

    // Verify relocation
    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.symbol, "pdpt");
    assert_eq!(reloc.kind, RelocKind::Abs32);
    assert_eq!(reloc.addend, 0);
    assert_eq!(reloc.byte_offset, 2); // disp32 starts at byte offset 2
}

/// Test 2: mov [abs32], imm32(0xFFFF_FFFF) → C7 05 ... FF FF FF FF + reloc with addend 0.
#[test]
fn mov_mem_abs32_max_u32_imm() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "kernel_data".to_string(),
                addend: 0,
            },
            Operand::Imm64(0xFFFF_FFFF as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [kernel_data], 0xFFFF_FFFF");

    // Expected: C7 05 00 00 00 00 FF FF FF FF
    let expected = &[0xC7, 0x05, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF];
    assert_eq!(buf.as_slice(), expected);

    // Verify relocation
    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.symbol, "kernel_data");
    assert_eq!(reloc.kind, RelocKind::Abs32);
    assert_eq!(reloc.addend, 0);
}

/// Test 3: mov [pdpt + addend=0], imm32 → reloc with addend 0.
#[test]
fn mov_mem_abs32_pdpt_plus_zero_addend() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "pdpt".to_string(),
                addend: 0,
            },
            Operand::Imm64(0x42),
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
    assert_eq!(reloc.symbol, "pdpt");
    assert_eq!(reloc.addend, 0);
}

/// Test 4: mov [pdpt + addend=8], imm32(0x1003) → C7 05 ... 03 10 00 00 + reloc addend 8.
#[test]
fn mov_mem_abs32_pdpt_plus_8_addend() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "pdpt".to_string(),
                addend: 8,
            },
            Operand::Imm64(0x1003),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [pdpt + 8], 0x1003");

    // Expected: C7 05 00 00 00 00 03 10 00 00
    let expected = &[0xC7, 0x05, 0x00, 0x00, 0x00, 0x00, 0x03, 0x10, 0x00, 0x00];
    assert_eq!(buf.as_slice(), expected);

    // Verify relocation with addend
    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.symbol, "pdpt");
    assert_eq!(reloc.kind, RelocKind::Abs32);
    assert_eq!(reloc.addend, 8);
    assert_eq!(reloc.byte_offset, 2);
}

/// Test 5: Mode64 + mov [abs], imm → Err with "not encodable in 64-bit mode" substring.
#[test]
fn mov_mem_abs_imm_mode64_unsupported() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::SymbolRef {
                name: "kernel_data".to_string(),
                addend: 0,
            },
            Operand::Imm64(0x1234),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let result = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats);

    // Should return an error
    assert!(result.is_err(), "expected error for Mode64 mov [abs], imm");

    // Check that the error message contains the diagnostic text
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("not encodable in 64-bit mode"),
        "error message should contain 'not encodable in 64-bit mode', got: {}",
        err_msg
    );
}
