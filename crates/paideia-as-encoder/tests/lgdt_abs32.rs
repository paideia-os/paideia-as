//! Integration tests for 32-bit LGDT with absolute addressing (PA15-m4-001).
//!
//! Tests verify that 32-bit lgdt instructions with absolute 32-bit addressing are
//! correctly encoded with byte-exact validation against test vectors and iced-x86 round-trip.
//!
//! Suite A: Byte-exact encoding validation (Mode32 SymbolRef).
//! Suite B: Mode64 regression guard (PcRel32 with addend=-4).
//! Suite C: Mode-agnostic [base+disp] form matching across modes.
//! Suite D: iced-x86 32-bit decoder round-trip.

use paideia_as_encoder::{CodeBuffer, EncodeStats, RelocKind};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== Suite A: Mode32 Absolute 32-bit Addressing =====

/// Test 1: lgdt [gdt32_ptr] in Mode32 → bytes [0x0F, 0x01, 0x15, 0, 0, 0, 0] + Abs32 reloc.
#[test]
fn encode_lgdt_symbol_ref_mode32_emits_abs32_reloc() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lgdt,
        operands: smallvec![Operand::SymbolRef {
            name: "gdt32_ptr".to_string(),
            addend: 0,
        }],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lgdt [gdt32_ptr]");

    // Expected bytes: 0F 01 15 00 00 00 00
    assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x15, 0, 0, 0, 0]);

    // Expect 1 relocation site with Abs32 kind
    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(
        reloc.kind,
        RelocKind::Abs32,
        "reloc kind should be Abs32 for Mode32"
    );
    assert_eq!(reloc.byte_offset, 3, "disp32 should start at byte offset 3");
    assert_eq!(reloc.symbol, "gdt32_ptr");
    assert_eq!(reloc.addend, 0, "addend should be raw (no PC32_FIELD_BIAS)");
}

// ===== Suite B: Mode64 Regression Guard (PA10-006v) =====

/// Test 2: lgdt [gdt_ptr] in Mode64 → PcRel32 reloc with addend=-4.
/// Regression guard for PA10-006v fix: ensures Mode64 SymbolRef uses PC-relative, not absolute.
#[test]
fn encode_lgdt_symbol_ref_mode64_keeps_pcrel32() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lgdt,
        operands: smallvec![Operand::SymbolRef {
            name: "gdt_ptr".to_string(),
            addend: 0,
        }],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lgdt [gdt_ptr] in Mode64");

    // Mode64 SymbolRef should still use RIP-relative (0x15 = rip-rel with /2)
    assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x15, 0, 0, 0, 0]);

    // Expect 1 relocation site with PcRel32 kind
    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(
        reloc.kind,
        RelocKind::PcRel32,
        "reloc kind should be PcRel32 for Mode64 (not Abs32)"
    );
    assert_eq!(
        reloc.addend, -4,
        "Mode64 PcRel32 addend should be -4 (PC32_FIELD_BIAS)"
    );
    assert_eq!(reloc.byte_offset, 3);
    assert_eq!(reloc.symbol, "gdt_ptr");
}

// ===== Suite C: Mode-Agnostic [base+disp] Form =====

/// Test 3: lgdt [rax+8] in Mode32 emits identical bytes to Mode64.
/// The [base+disp] form is mode-agnostic and should not be affected by the Mode32 short-circuit.
#[test]
fn encode_lgdt_rax_disp8_mode32_matches_mode64() {
    let mut buf_mode32 = CodeBuffer::new();
    let inst_mode32 = Instruction {
        mnemonic: Mnemonic::Lgdt,
        operands: smallvec![Operand::MemSib {
            base: RegId(0), // rax
            index: None,
            scale: Scale::X1,
            disp: 8,
        }],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let _ = paideia_as_encoder::encode_instruction(&inst_mode32, &mut buf_mode32, &mut stats)
        .expect("encoding failed for lgdt [rax+8] Mode32");

    let mut buf_mode64 = CodeBuffer::new();
    let inst_mode64 = Instruction {
        mnemonic: Mnemonic::Lgdt,
        operands: smallvec![Operand::MemSib {
            base: RegId(0), // rax
            index: None,
            scale: Scale::X1,
            disp: 8,
        }],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let _ = paideia_as_encoder::encode_instruction(&inst_mode64, &mut buf_mode64, &mut stats)
        .expect("encoding failed for lgdt [rax+8] Mode64");

    // Both should produce identical bytes
    assert_eq!(
        buf_mode32.as_slice(),
        buf_mode64.as_slice(),
        "[rax+8] form should be mode-agnostic"
    );
}

// ===== Suite D: iced-x86 32-bit Decoder Round-trip =====

/// Test 4: iced-x86 32-bit decoder round-trip for lgdt [abs32].
/// This verifies the encoding is structurally correct and decodable.
#[test]
fn iced_x86_32bit_decoder_round_trip_lgdt() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lgdt,
        operands: smallvec![Operand::SymbolRef {
            name: "gdt32_ptr".to_string(),
            addend: 0,
        }],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

    // Decode the bytes with iced-x86 (32-bit mode for consistency)
    let mut decoder = Decoder::new(32, buf.as_slice(), DecoderOptions::NONE);
    let decoded_instr = decoder.decode();

    // Verify it decoded as LGDT (LLDT in iced is the load instruction)
    // Note: iced-x86 may call it LGDT or LLDT depending on version; we check for load descriptor table instruction
    assert_eq!(decoded_instr.mnemonic(), IcedMnem::Lgdt);
}

// ===== Suite E: Addend Propagation =====

/// Test 5: lgdt [gdt + addend=8] in Mode32 → Abs32 reloc with addend 8.
#[test]
fn encode_lgdt_symbol_ref_mode32_with_nonzero_addend() {
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lgdt,
        operands: smallvec![Operand::SymbolRef {
            name: "gdt".to_string(),
            addend: 8,
        }],
        byte_offset_in_text: None,
        mode: InstrMode::Mode32,
        encoding_hint: None,
    };

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lgdt [gdt + 8]");

    // Expected bytes: 0F 01 15 00 00 00 00
    assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x15, 0, 0, 0, 0]);

    // Expect 1 relocation site with Abs32 kind and addend 8
    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.kind, RelocKind::Abs32);
    assert_eq!(reloc.addend, 8, "addend should be propagated raw (no bias)");
    assert_eq!(reloc.byte_offset, 3);
    assert_eq!(reloc.symbol, "gdt");
}
