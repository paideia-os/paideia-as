//! Regression tests for PA10-006o: Double-counted relocation offset bug fix.
//!
//! Verifies that 4 encoders (mov r64,[sym], lea r64,[sym], lgdt [sym], lidt [sym])
//! emit RelocSite with byte_offset = 3 (instruction-local), not the absolute buffer offset.
//!
//! Bug: These encoders were computing byte_offset = buf.bytes.len() (absolute) before
//! extending the buffer, but text_emitter adds offset_before again → double-counted.
//! Fix: Use byte_offset = 3 (the instruction-local position where disp32 starts).

use paideia_as_encoder::{CodeBuffer, EncodeStats, RelocKind};
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use smallvec::smallvec;

/// Helper to encode an instruction and check its relocation offset.
fn assert_reloc_byte_offset(inst: &Instruction, expected_offset: u32, desc: &str) {
    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();

    let output = paideia_as_encoder::encode_instruction(inst, &mut buf, &mut stats)
        .expect(&format!("encoding failed for {}", desc));

    let relocs = &output.reloc_sites;
    assert_eq!(relocs.len(), 1, "{}: expected exactly 1 relocation", desc);

    let reloc = &relocs[0];
    assert_eq!(
        reloc.byte_offset, expected_offset,
        "{}: byte_offset should be {} (instruction-local), got {}",
        desc, expected_offset, reloc.byte_offset
    );
}

// ===== mov r64, [symbol] =====
#[test]
fn mov_r64_symbol_reloc_offset_is_3() {
    // mov rax, [symbol] → REX + opcode (0x8B) + ModR/M + disp32
    // 3 bytes before disp32 (REX + 0x8B + ModR/M)
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)), // rax (dest)
            Operand::SymbolRef {
                name: "test_symbol".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        encoding_hint: None,
    };
    assert_reloc_byte_offset(&inst, 3, "mov rax, [test_symbol]");
}

#[test]
fn mov_r64_symbol_reloc_kind_is_pcrel32() {
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::SymbolRef {
                name: "test_symbol".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov with symbol");

    let relocs = &output.reloc_sites;
    assert_eq!(relocs.len(), 1);
    assert_eq!(relocs[0].kind, RelocKind::PcRel32);
    assert_eq!(relocs[0].symbol, "test_symbol");
}

// ===== lea r64, [symbol] =====
#[test]
fn lea_r64_symbol_reloc_offset_is_3() {
    // lea rax, [symbol] → REX + opcode (0x8D) + ModR/M + disp32
    // 3 bytes before disp32
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(0)), // rax (dest)
            Operand::SymbolRef {
                name: "test_symbol".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        encoding_hint: None,
    };
    assert_reloc_byte_offset(&inst, 3, "lea rax, [test_symbol]");
}

#[test]
fn lea_r64_symbol_reloc_kind_is_pcrel32() {
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::SymbolRef {
                name: "test_symbol".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lea with symbol");

    let relocs = &output.reloc_sites;
    assert_eq!(relocs.len(), 1);
    assert_eq!(relocs[0].kind, RelocKind::PcRel32);
    assert_eq!(relocs[0].symbol, "test_symbol");
}

// ===== lgdt [symbol] =====
#[test]
fn lgdt_symbol_reloc_offset_is_3() {
    // lgdt [symbol] → escape (0x0F) + opcode (0x01) + ModR/M + disp32
    // 3 bytes before disp32 (0x0F + 0x01 + ModR/M)
    let inst = Instruction {
        mnemonic: Mnemonic::Lgdt,
        operands: smallvec![Operand::SymbolRef {
            name: "gdt_table".to_string(),
            addend: 0,
        }],
        byte_offset_in_text: None,
        encoding_hint: None,
    };
    assert_reloc_byte_offset(&inst, 3, "lgdt [gdt_table]");
}

#[test]
fn lgdt_symbol_reloc_kind_is_pcrel32() {
    let inst = Instruction {
        mnemonic: Mnemonic::Lgdt,
        operands: smallvec![Operand::SymbolRef {
            name: "gdt_table".to_string(),
            addend: 0,
        }],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lgdt with symbol");

    let relocs = &output.reloc_sites;
    assert_eq!(relocs.len(), 1);
    assert_eq!(relocs[0].kind, RelocKind::PcRel32);
    assert_eq!(relocs[0].symbol, "gdt_table");
}

// ===== lidt [symbol] =====
#[test]
fn lidt_symbol_reloc_offset_is_3() {
    // lidt [symbol] → escape (0x0F) + opcode (0x01) + ModR/M + disp32
    // 3 bytes before disp32 (0x0F + 0x01 + ModR/M)
    let inst = Instruction {
        mnemonic: Mnemonic::Lidt,
        operands: smallvec![Operand::SymbolRef {
            name: "idt_table".to_string(),
            addend: 0,
        }],
        byte_offset_in_text: None,
        encoding_hint: None,
    };
    assert_reloc_byte_offset(&inst, 3, "lidt [idt_table]");
}

#[test]
fn lidt_symbol_reloc_kind_is_pcrel32() {
    let inst = Instruction {
        mnemonic: Mnemonic::Lidt,
        operands: smallvec![Operand::SymbolRef {
            name: "idt_table".to_string(),
            addend: 0,
        }],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    let output = paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lidt with symbol");

    let relocs = &output.reloc_sites;
    assert_eq!(relocs.len(), 1);
    assert_eq!(relocs[0].kind, RelocKind::PcRel32);
    assert_eq!(relocs[0].symbol, "idt_table");
}

// ===== Encoding size checks (sanity) =====
#[test]
fn mov_r64_symbol_encodes_7_bytes() {
    // REX + opcode + ModR/M + disp32 = 1 + 1 + 1 + 4 = 7
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::SymbolRef {
                name: "sym".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).unwrap();
    assert_eq!(buf.as_slice().len(), 7, "mov r64, [sym] should be 7 bytes");
}

#[test]
fn lea_r64_symbol_encodes_7_bytes() {
    // REX + opcode + ModR/M + disp32 = 1 + 1 + 1 + 4 = 7
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(0)),
            Operand::SymbolRef {
                name: "sym".to_string(),
                addend: 0,
            }
        ],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).unwrap();
    assert_eq!(buf.as_slice().len(), 7, "lea r64, [sym] should be 7 bytes");
}

#[test]
fn lgdt_symbol_encodes_7_bytes() {
    // escape + opcode + ModR/M + disp32 = 1 + 1 + 1 + 4 = 7
    let inst = Instruction {
        mnemonic: Mnemonic::Lgdt,
        operands: smallvec![Operand::SymbolRef {
            name: "gdt".to_string(),
            addend: 0,
        }],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).unwrap();
    assert_eq!(buf.as_slice().len(), 7, "lgdt [gdt] should be 7 bytes");
}

#[test]
fn lidt_symbol_encodes_7_bytes() {
    // escape + opcode + ModR/M + disp32 = 1 + 1 + 1 + 4 = 7
    let inst = Instruction {
        mnemonic: Mnemonic::Lidt,
        operands: smallvec![Operand::SymbolRef {
            name: "idt".to_string(),
            addend: 0,
        }],
        byte_offset_in_text: None,
        encoding_hint: None,
    };

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).unwrap();
    assert_eq!(buf.as_slice().len(), 7, "lidt [idt] should be 7 bytes");
}
