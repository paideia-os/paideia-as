//! Tests for PA-R13-003: indirect call instructions (call reg / call [mem]).
//!
//! This test suite verifies that call forms emit the correct x86_64 bytecode:
//! - call reg64: FF /2 (register form)
//! - call [base + disp]: FF 14 [base + disp] (memory form)
//! - call [base + index*scale + disp]: FF 14 SIB [disp] (indexed memory form)
//! - call [rip + disp32]: FF 15 <disp32> (RIP-relative form)
//! - call [rip + sym + addend]: FF 15 <disp32_placeholder> + PcRel32 relocation (PA-R17-015)

use paideia_as_encoder::{CodeBuffer, EncodeStats, Reg64, RelocKind};
use paideia_as_encoder::encode::{call_reg64, call_mem_base_disp, call_mem_sib_disp, call_mem_rip_rel};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use smallvec::smallvec;

// ── 7 byte-exact test cases ────────────────────────────────────────────

#[test]
fn pa_r13_003_call_rax() {
    // call rax → FF D0
    // Expected: 0xFF (opcode), 0xD0 (ModR/M: mod=11, reg=010, rm=000)
    let mut buf = CodeBuffer::new();
    call_reg64(&mut buf, Reg64::Rax);
    assert_eq!(buf.as_slice(), &[0xFF, 0xD0]);
}

#[test]
fn pa_r13_003_call_r8() {
    // call r8 → 41 FF D0
    // Expected: 0x41 (REX.B), 0xFF (opcode), 0xD0 (ModR/M: mod=11, reg=010, rm=000 for r8)
    let mut buf = CodeBuffer::new();
    call_reg64(&mut buf, Reg64::R8);
    assert_eq!(buf.as_slice(), &[0x41, 0xFF, 0xD0]);
}

#[test]
fn pa_r13_003_call_mem_rax() {
    // call [rax] → FF 10
    // Expected: 0xFF (opcode), 0x10 (ModR/M: mod=00, reg=010, rm=000)
    let mut buf = CodeBuffer::new();
    call_mem_base_disp(&mut buf, Reg64::Rax, 0);
    assert_eq!(buf.as_slice(), &[0xFF, 0x10]);
}

#[test]
fn pa_r13_003_call_mem_rdi_8() {
    // call [rdi + 8] → FF 57 08
    // Expected: 0xFF (opcode), 0x57 (ModR/M: mod=01, reg=010, rm=111), 0x08 (disp8)
    let mut buf = CodeBuffer::new();
    call_mem_base_disp(&mut buf, Reg64::Rdi, 8);
    assert_eq!(buf.as_slice(), &[0xFF, 0x57, 0x08]);
}

#[test]
fn pa_r13_003_call_mem_r12_rsi_8() {
    // call [r12 + rsi*8] → 41 FF 14 F4
    // Expected: 0x41 (REX.B for r12), 0xFF (opcode), 0x14 (ModR/M: mod=00, reg=010, rm=100 for SIB),
    // 0xF4 (SIB: scale=11 (×8), index=110 (rsi), base=100 (r12))
    let mut buf = CodeBuffer::new();
    call_mem_sib_disp(&mut buf, Reg64::R12, Reg64::Rsi, 3, 0);
    assert_eq!(buf.as_slice(), &[0x41, 0xFF, 0x14, 0xF4]);
}

#[test]
fn pa_r13_003_call_mem_r13_rsi_8() {
    // call [r13 + rsi*8] → 41 FF 54 F5 00
    // R13 is RBP with B=1 (r13 = rbp + 8, so bit 3 set = B escape for SIB base)
    // Expected: 0x41 (REX.B for r13), 0xFF (opcode), 0x54 (ModR/M: mod=01, reg=010, rm=100 for SIB),
    // 0xF5 (SIB: scale=11 (×8), index=110 (rsi), base=101 (r13)), 0x00 (disp8 = 0, but required due to RBP escape)
    let mut buf = CodeBuffer::new();
    call_mem_sib_disp(&mut buf, Reg64::R13, Reg64::Rsi, 3, 0);
    assert_eq!(buf.as_slice(), &[0x41, 0xFF, 0x54, 0xF5, 0x00]);
}

#[test]
fn pa_r13_003_call_mem_rip_rel() {
    // call [rip + sym] → FF 15 00 00 00 00 (+ relocation)
    // Expected: 0xFF (opcode), 0x15 (ModR/M: mod=00, reg=010, rm=101 for RIP-relative),
    // 0x00 0x00 0x00 0x00 (disp32 placeholder)
    let mut buf = CodeBuffer::new();
    call_mem_rip_rel(&mut buf, 0);
    assert_eq!(buf.as_slice(), &[0xFF, 0x15, 0x00, 0x00, 0x00, 0x00]);
}

// ── 3 iced-x86 round-trip verification tests ────────────────────────────

#[test]
fn pa_r13_003_roundtrip_call_rax() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    call_reg64(&mut buf, Reg64::Rax);
    let bytes = buf.as_slice();

    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode call rax");

    assert_eq!(instr.mnemonic(), IcedMnem::Call);
    assert_eq!(instr.op_count(), 1);
}

#[test]
fn pa_r13_003_roundtrip_call_mem_rdi_8() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    call_mem_base_disp(&mut buf, Reg64::Rdi, 8);
    let bytes = buf.as_slice();

    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode call [rdi + 8]");

    assert_eq!(instr.mnemonic(), IcedMnem::Call);
    assert_eq!(instr.op_count(), 1);
}

#[test]
fn pa_r13_003_roundtrip_call_mem_r12_rsi_8() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    call_mem_sib_disp(&mut buf, Reg64::R12, Reg64::Rsi, 3, 0);
    let bytes = buf.as_slice();

    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();
    assert!(instr.len() > 0, "Failed to decode call [r12 + rsi*8]");

    assert_eq!(instr.mnemonic(), IcedMnem::Call);
    assert_eq!(instr.op_count(), 1);
}

// ── PA-R17-015: call [rip + sym + addend] tests ────────────────────────

// ── 6 byte-exact test cases ────────────────────────────────────────────

#[test]
fn pa_r17_015_call_rip_sym_addend_0() {
    // call [rip + sym] with addend=0
    // Expected: FF 15 00 00 00 00 (6 bytes)
    // Relocation: byte_offset=2, kind=PcRel32, addend=-4 (PC32_FIELD_BIAS)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Call,
        operands: smallvec![
            Operand::MemRipRelSym {
                name: "test_sym".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for call [rip + sym]");

    assert_eq!(buf.as_slice(), &[0xFF, 0x15, 0x00, 0x00, 0x00, 0x00],
               "call [rip + sym] bytecode mismatch");

    // Verify relocation
    assert_eq!(output.reloc_sites.len(), 1, "Expected 1 relocation");
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.byte_offset, 2, "Relocation byte_offset should be 2");
    assert_eq!(reloc.symbol, "test_sym", "Relocation symbol mismatch");
    assert_eq!(reloc.kind, RelocKind::PcRel32, "Relocation kind should be PcRel32");
    assert_eq!(reloc.addend, -4, "Relocation addend should be -4 (PC32_FIELD_BIAS)");
}

#[test]
fn pa_r17_015_call_rip_sym_addend_8() {
    // call [rip + sym + 8] (record field at offset 8)
    // Expected: FF 15 00 00 00 00 (6 bytes, same as addend=0)
    // Relocation: byte_offset=2, kind=PcRel32, addend=8+(-4)=4
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Call,
        operands: smallvec![
            Operand::MemRipRelSym {
                name: "vtable".to_string(),
                addend: 8,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for call [rip + sym + 8]");

    assert_eq!(buf.as_slice(), &[0xFF, 0x15, 0x00, 0x00, 0x00, 0x00],
               "call [rip + sym + 8] bytecode mismatch");

    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.byte_offset, 2);
    assert_eq!(reloc.addend, 4, "Relocation addend should be 8 + (-4) = 4");
}

#[test]
fn pa_r17_015_call_rip_sym_addend_24() {
    // call [rip + sym + 24]
    // Expected: FF 15 00 00 00 00 (6 bytes)
    // Relocation: addend=24+(-4)=20
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Call,
        operands: smallvec![
            Operand::MemRipRelSym {
                name: "vops".to_string(),
                addend: 24,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for call [rip + sym + 24]");

    assert_eq!(buf.as_slice(), &[0xFF, 0x15, 0x00, 0x00, 0x00, 0x00]);

    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.addend, 20, "Relocation addend should be 24 + (-4) = 20");
}

#[test]
fn pa_r17_015_call_rip_sym_negative_addend() {
    // call [rip + sym + (-16)]
    // Expected: FF 15 00 00 00 00 (6 bytes)
    // Relocation: addend=-16+(-4)=-20
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Call,
        operands: smallvec![
            Operand::MemRipRelSym {
                name: "data".to_string(),
                addend: -16,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for call [rip + sym - 16]");

    assert_eq!(buf.as_slice(), &[0xFF, 0x15, 0x00, 0x00, 0x00, 0x00]);

    assert_eq!(output.reloc_sites.len(), 1);
    let reloc = &output.reloc_sites[0];
    assert_eq!(reloc.addend, -20, "Relocation addend should be -16 + (-4) = -20");
}

#[test]
fn pa_r17_015_two_consecutive_calls_different_symbols() {
    // Two consecutive call [rip + sym1]; call [rip + sym2]
    // Each should have its own RelocSite with byte_offset=2 (instruction-local)
    let mut buf = CodeBuffer::new();

    let inst1 = Instruction {
        mnemonic: Mnemonic::Call,
        operands: smallvec![
            Operand::MemRipRelSym {
                name: "sym1".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let inst2 = Instruction {
        mnemonic: Mnemonic::Call,
        operands: smallvec![
            Operand::MemRipRelSym {
                name: "sym2".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    let output1 = paideia_as_encoder::encode_instruction(&inst1, &mut buf, &mut stats)
        .expect("encoding failed for first call");
    let output2 = paideia_as_encoder::encode_instruction(&inst2, &mut buf, &mut stats)
        .expect("encoding failed for second call");

    // Verify bytecode: 12 bytes total (6 + 6)
    assert_eq!(buf.as_slice().len(), 12);
    assert_eq!(&buf.as_slice()[0..6], &[0xFF, 0x15, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(&buf.as_slice()[6..12], &[0xFF, 0x15, 0x00, 0x00, 0x00, 0x00]);

    // Verify both relocations exist with byte_offset=2 (instruction-local)
    assert_eq!(output1.reloc_sites.len(), 1);
    assert_eq!(output1.reloc_sites[0].byte_offset, 2, "First reloc byte_offset should be 2");
    assert_eq!(output1.reloc_sites[0].symbol, "sym1");

    assert_eq!(output2.reloc_sites.len(), 1);
    assert_eq!(output2.reloc_sites[0].byte_offset, 2, "Second reloc byte_offset should be 2 (instruction-local)");
    assert_eq!(output2.reloc_sites[0].symbol, "sym2");
}

#[test]
fn pa_r17_015_call_after_padding() {
    // 10 padding bytes + call [rip + sym]
    // Verify that byte_offset in the relocation is still 2 (instruction-local, not 12)
    let mut buf = CodeBuffer::new();

    // Add 10 padding bytes
    for _ in 0..10 {
        buf.bytes.push(0x90); // nop
    }

    let inst = Instruction {
        mnemonic: Mnemonic::Call,
        operands: smallvec![
            Operand::MemRipRelSym {
                name: "padding_test".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for call after padding");

    // Verify bytecode: 16 bytes total (10 padding + 6 call)
    assert_eq!(buf.as_slice().len(), 16);
    assert_eq!(&buf.as_slice()[10..16], &[0xFF, 0x15, 0x00, 0x00, 0x00, 0x00]);

    // Relocation byte_offset should be 2 (instruction-local), not 12 (global)
    assert_eq!(output.reloc_sites.len(), 1);
    assert_eq!(output.reloc_sites[0].byte_offset, 2, "Relocation should be instruction-local (byte_offset=2)");
}

// ── 2 iced-x86 round-trip verification tests ────────────────────────────

#[test]
fn pa_r17_015_roundtrip_call_rip_sym() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Call,
        operands: smallvec![
            Operand::MemRipRelSym {
                name: "roundtrip1".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for roundtrip test");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert!(instr.len() > 0, "Failed to decode call [rip + sym]");
    assert_eq!(instr.mnemonic(), IcedMnem::Call, "Decoded mnemonic should be Call");
    assert_eq!(instr.op_count(), 1, "Call should have 1 operand");
    assert!(instr.is_ip_rel_memory_operand(), "Call operand should be RIP-relative memory");
}

#[test]
fn pa_r17_015_roundtrip_call_rip_sym_with_addend() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Call,
        operands: smallvec![
            Operand::MemRipRelSym {
                name: "roundtrip2".to_string(),
                addend: 100,
            }
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for roundtrip with addend");

    let bytes = buf.as_slice();
    let mut decoder = Decoder::new(64, bytes, DecoderOptions::NONE);
    let instr = decoder.decode();

    assert!(instr.len() > 0, "Failed to decode call [rip + sym + 100]");
    assert_eq!(instr.mnemonic(), IcedMnem::Call);
    assert!(instr.is_ip_rel_memory_operand(), "Operand should be RIP-relative memory");
}
