//! Integration tests for supervisor mnemonics mode-agnostic encoding (Phase 15 m4-002).
//!
//! These tests verify that supervisor mnemonics encode identically regardless of
//! instruction mode (Mode32 vs Mode64). The supervisor instruction set is
//! mode-agnostic: no REX.W prefix is used, and the encodings are byte-identical.
//!
//! Supervisor mnemonics tested:
//! - CLI (0xFA)
//! - STI (0xFB)
//! - HLT (0xF4)
//! - NOP (0x90)
//! - RDMSR (0x0F 0x32)
//! - WRMSR (0x0F 0x30)
//! - MOV CR3, RAX (0x0F 0x22 0xD8) — mode-agnostic but called with eax in Mode32
//! - MOV RAX, CR0 (0x0F 0x20 0xC0) — mode-agnostic but called with eax in Mode32
//! - MOV DR0, RAX (0x0F 0x23 0xC0)
//! - MOV RAX, DR6 (0x0F 0x21 0xF0)
//!
//! Each test encodes the instruction twice (once with Mode32, once with Mode64)
//! and asserts the resulting byte streams are byte-identical.

use paideia_as_encoder::{CodeBuffer, EncodeStats};
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
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
fn cli_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Cli,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "CLI encoding differs between Mode32 and Mode64"
    );
    assert_eq!(mode32_bytes, &[0xFA], "CLI should encode as 0xFA");
}

#[test]
fn sti_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Sti,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "STI encoding differs between Mode32 and Mode64"
    );
    assert_eq!(mode32_bytes, &[0xFB], "STI should encode as 0xFB");
}

#[test]
fn hlt_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Hlt,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "HLT encoding differs between Mode32 and Mode64"
    );
    assert_eq!(mode32_bytes, &[0xF4], "HLT should encode as 0xF4");
}

#[test]
fn nop_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Nop,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "NOP encoding differs between Mode32 and Mode64"
    );
    assert_eq!(mode32_bytes, &[0x90], "NOP should encode as 0x90");
}

#[test]
fn rdmsr_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Rdmsr,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "RDMSR encoding differs between Mode32 and Mode64"
    );
    assert_eq!(
        mode32_bytes,
        &[0x0F, 0x32],
        "RDMSR should encode as 0x0F 0x32"
    );
}

#[test]
fn wrmsr_mode32_equals_mode64() {
    let inst = Instruction {
        mnemonic: Mnemonic::Wrmsr,
        operands: smallvec![],
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "WRMSR encoding differs between Mode32 and Mode64"
    );
    assert_eq!(
        mode32_bytes,
        &[0x0F, 0x30],
        "WRMSR should encode as 0x0F 0x30"
    );
}

#[test]
fn mov_cr3_rax_mode32_equals_mode64() {
    // mov cr3, rax (or eax in Mode32) — encoding 0x0F 0x22 0xD8 is mode-agnostic
    // CR3 is RegId(19) (16 + 3); compact encoding via Mov dispatcher
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(19)), Operand::Reg(RegId(0))], // cr3, rax
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "MOV CR3,RAX encoding differs between Mode32 and Mode64"
    );
    assert_eq!(
        mode32_bytes,
        &[0x0F, 0x22, 0xD8],
        "MOV CR3,RAX should encode as 0x0F 0x22 0xD8"
    );
}

#[test]
fn mov_rax_cr0_mode32_equals_mode64() {
    // mov rax, cr0 (or eax in Mode32) — encoding 0x0F 0x20 0xC0 is mode-agnostic
    // CR0 is RegId(16) (16 + 0); compact encoding via Mov dispatcher
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(16))], // rax, cr0
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "MOV RAX,CR0 encoding differs between Mode32 and Mode64"
    );
    assert_eq!(
        mode32_bytes,
        &[0x0F, 0x20, 0xC0],
        "MOV RAX,CR0 should encode as 0x0F 0x20 0xC0"
    );
}

#[test]
fn mov_dr0_rax_mode32_equals_mode64() {
    // mov dr0, rax (or eax in Mode32) — encoding 0x0F 0x23 0xC0 is mode-agnostic
    // DR0 is RegId(25) (25 + 0); compact encoding via Mov dispatcher
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(25)), Operand::Reg(RegId(0))], // dr0, rax
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "MOV DR0,RAX encoding differs between Mode32 and Mode64"
    );
    assert_eq!(
        mode32_bytes,
        &[0x0F, 0x23, 0xC0],
        "MOV DR0,RAX should encode as 0x0F 0x23 0xC0"
    );
}

#[test]
fn mov_rax_dr6_mode32_equals_mode64() {
    // mov rax, dr6 (or eax in Mode32) — encoding 0x0F 0x21 0xF0 is mode-agnostic
    // DR6 is RegId(31) (25 + 6); compact encoding via Mov dispatcher
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(31))], // rax, dr6
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        encoding_hint: None,
    };

    let mode32_bytes = encode_in_mode(&inst, InstrMode::Mode32);
    let mode64_bytes = encode_in_mode(&inst, InstrMode::Mode64);

    assert_eq!(
        mode32_bytes, mode64_bytes,
        "MOV RAX,DR6 encoding differs between Mode32 and Mode64"
    );
    assert_eq!(
        mode32_bytes,
        &[0x0F, 0x21, 0xF0],
        "MOV RAX,DR6 should encode as 0x0F 0x21 0xF0"
    );
}
