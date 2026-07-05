//! PA-R13-002: REX.B audit for indexed load/store and LEA with SIB.
//!
//! This test suite validates that:
//! 1. emit_indexed_load correctly derives REX.R from dst and REX.B from base
//! 2. emit_indexed_store correctly derives REX.R from src and REX.B from base
//! 3. encode_lea SIB path handles R13/RBP escape (disp8=0 when base=RBP/R13 and disp=0)
//!
//! 15 byte-exact test vectors cover the instruction families with high-register bases/indices/dests:
//! - mov rax, [r12 + rsi*8] → expect 49 8B 04 F4
//! - mov rax, [r13 + rsi*8] → expect 49 8B 44 F5 00
//! - mov rax, [r13 + rax*4] → expect 49 8B 44 85 00
//! - mov rbx, [rax + rcx*8] → expect 48 8B 1C C8
//! - mov r10, [rdi + rsi*8] → expect 4C 8B 14 F7
//! - mov r15, [r12 + r14*8] → expect 4F 8B 3C F4
//! - lea rax, [r13 + rsi*8] → expect 49 8D 44 F5 00

use paideia_as_encoder::CodeBuffer;
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId, Scale};
use smallvec::smallvec;

// ===== Indexed Load Tests (mov r64, [base+index*scale]) =====

#[test]
fn mov_rax_from_r12_plus_rsi_times_8() {
    // mov rax, [r12 + rsi*8]
    // rax=0, r12=12, rsi=6
    // REX: W=1, R=0, X=0, B=1 (r12 >> 3) → 0x49
    // Expected: 49 8B 04 F4
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax (dest)
            Operand::MemSib { base: RegId(12), index: Some(RegId(6)), scale: Scale::X8, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, [r12 + rsi*8]");

    assert_eq!(buf.as_slice(), &[0x49, 0x8B, 0x04, 0xF4], "mov rax, [r12+rsi*8]");
}

#[test]
fn mov_rax_from_r13_plus_rsi_times_8() {
    // mov rax, [r13 + rsi*8]
    // rax=0, r13=13, rsi=6
    // REX: W=1, R=0, X=0, B=1 (r13 >> 3) → 0x49
    // Expected: 49 8B 44 F5 00
    // (base=r13 requires disp8=0 escape)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax (dest)
            Operand::MemSib { base: RegId(13), index: Some(RegId(6)), scale: Scale::X8, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, [r13 + rsi*8]");

    assert_eq!(buf.as_slice(), &[0x49, 0x8B, 0x44, 0xF5, 0x00], "mov rax, [r13+rsi*8]");
}

#[test]
fn mov_rax_from_r13_plus_rax_times_4() {
    // mov rax, [r13 + rax*4]
    // rax=0 (as dest), r13=13, rax=0 (as index)
    // REX: W=1, R=0, X=0, B=1 (r13 >> 3) → 0x49
    // Expected: 49 8B 44 85 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax (dest)
            Operand::MemSib { base: RegId(13), index: Some(RegId(0)), scale: Scale::X4, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, [r13 + rax*4]");

    assert_eq!(buf.as_slice(), &[0x49, 0x8B, 0x44, 0x85, 0x00], "mov rax, [r13+rax*4]");
}

#[test]
fn mov_rbx_from_rax_plus_rcx_times_8() {
    // mov rbx, [rax + rcx*8]
    // rbx=3, rax=0, rcx=1
    // REX: W=1, R=0, X=0, B=0 → 0x48
    // Expected: 48 8B 1C C8
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(3)),  // rbx (dest)
            Operand::MemSib { base: RegId(0), index: Some(RegId(1)), scale: Scale::X8, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rbx, [rax + rcx*8]");

    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x1C, 0xC8], "mov rbx, [rax+rcx*8]");
}

#[test]
fn mov_r10_from_rdi_plus_rsi_times_8() {
    // mov r10, [rdi + rsi*8]
    // r10=10, rdi=7, rsi=6
    // REX: W=1, R=1 (r10 >> 3), X=0, B=0 → 0x4C
    // Expected: 4C 8B 14 F7
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(10)), // r10 (dest)
            Operand::MemSib { base: RegId(7), index: Some(RegId(6)), scale: Scale::X8, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r10, [rdi + rsi*8]");

    assert_eq!(buf.as_slice(), &[0x4C, 0x8B, 0x14, 0xF7], "mov r10, [rdi+rsi*8]");
}

#[test]
fn mov_r15_from_r12_plus_r14_times_8() {
    // mov r15, [r12 + r14*8]
    // r15=15, r12=12, r14=14
    // REX: W=1, R=1 (r15 >> 3), X=1 (r14 >> 3), B=1 (r12 >> 3) → 0x4F
    // Expected: 4F 8B 3C F4
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(15)), // r15 (dest)
            Operand::MemSib { base: RegId(12), index: Some(RegId(14)), scale: Scale::X8, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r15, [r12 + r14*8]");

    assert_eq!(buf.as_slice(), &[0x4F, 0x8B, 0x3C, 0xF4], "mov r15, [r12+r14*8]");
}

// ===== Regression: Low-register baseline (should stay unchanged) =====

#[test]
fn mov_rax_from_rdi_plus_rsi_times_8_regression() {
    // mov rax, [rdi + rsi*8]
    // rax=0, rdi=7, rsi=6 (all low registers)
    // REX: W=1, R=0, X=0, B=0 → 0x48
    // Expected: 48 8B 04 F7
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax (dest)
            Operand::MemSib { base: RegId(7), index: Some(RegId(6)), scale: Scale::X8, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, [rdi + rsi*8]");

    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x04, 0xF7], "mov rax, [rdi+rsi*8]");
}

// ===== Indexed Store Tests (mov [base+index*scale], src) =====

#[test]
fn mov_to_r12_plus_rsi_times_8_from_rax() {
    // mov [r12 + rsi*8], rax
    // r12=12 (base), rsi=6 (index), rax=0 (src)
    // REX: W=1, R=0 (rax >> 3), X=0, B=1 (r12 >> 3) → 0x49
    // Expected: 49 89 04 F4
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(12), index: Some(RegId(6)), scale: Scale::X8, disp: 0 },
            Operand::Reg(RegId(0)),  // rax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [r12 + rsi*8], rax");

    assert_eq!(buf.as_slice(), &[0x49, 0x89, 0x04, 0xF4], "mov [r12+rsi*8], rax");
}

#[test]
fn mov_to_r13_plus_rsi_times_8_from_rax() {
    // mov [r13 + rsi*8], rax
    // r13=13 (base), rsi=6 (index), rax=0 (src)
    // REX: W=1, R=0, X=0, B=1 (r13 >> 3) → 0x49
    // Expected: 49 89 44 F5 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(13), index: Some(RegId(6)), scale: Scale::X8, disp: 0 },
            Operand::Reg(RegId(0)),  // rax (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [r13 + rsi*8], rax");

    assert_eq!(buf.as_slice(), &[0x49, 0x89, 0x44, 0xF5, 0x00], "mov [r13+rsi*8], rax");
}

#[test]
fn mov_to_rax_plus_rcx_times_8_from_rbx() {
    // mov [rax + rcx*8], rbx
    // rax=0 (base), rcx=1 (index), rbx=3 (src)
    // REX: W=1, R=0, X=0, B=0 → 0x48
    // Expected: 48 89 1C C8
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(0), index: Some(RegId(1)), scale: Scale::X8, disp: 0 },
            Operand::Reg(RegId(3)),  // rbx (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [rax + rcx*8], rbx");

    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x1C, 0xC8], "mov [rax+rcx*8], rbx");
}

#[test]
fn mov_to_rdi_plus_rsi_times_8_from_r10() {
    // mov [rdi + rsi*8], r10
    // rdi=7 (base), rsi=6 (index), r10=10 (src)
    // REX: W=1, R=1 (r10 >> 3), X=0, B=0 → 0x4C
    // Expected: 4C 89 14 F7
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(7), index: Some(RegId(6)), scale: Scale::X8, disp: 0 },
            Operand::Reg(RegId(10)), // r10 (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [rdi + rsi*8], r10");

    assert_eq!(buf.as_slice(), &[0x4C, 0x89, 0x14, 0xF7], "mov [rdi+rsi*8], r10");
}

#[test]
fn mov_to_r12_plus_r14_times_8_from_r15() {
    // mov [r12 + r14*8], r15
    // r12=12 (base), r14=14 (index), r15=15 (src)
    // REX: W=1, R=1 (r15 >> 3), X=1 (r14 >> 3), B=1 (r12 >> 3) → 0x4F
    // Expected: 4F 89 3C F4
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(12), index: Some(RegId(14)), scale: Scale::X8, disp: 0 },
            Operand::Reg(RegId(15)), // r15 (src)
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [r12 + r14*8], r15");

    assert_eq!(buf.as_slice(), &[0x4F, 0x89, 0x3C, 0xF4], "mov [r12+r14*8], r15");
}

// ===== LEA Tests =====

#[test]
fn lea_rax_from_r13_plus_rsi_times_8() {
    // lea rax, [r13 + rsi*8]
    // rax=0 (dest), r13=13 (base), rsi=6 (index)
    // REX: W=1, R=0, X=0, B=1 (r13 >> 3) → 0x49
    // Expected: 49 8D 44 F5 00
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax (dest)
            Operand::MemSib { base: RegId(13), index: Some(RegId(6)), scale: Scale::X8, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lea rax, [r13 + rsi*8]");

    assert_eq!(buf.as_slice(), &[0x49, 0x8D, 0x44, 0xF5, 0x00], "lea rax, [r13+rsi*8]");
}

#[test]
fn lea_rbx_from_r12_plus_r14_times_8() {
    // lea rbx, [r12 + r14*8]
    // rbx=3 (dest), r12=12 (base), r14=14 (index)
    // REX: W=1, R=0, X=1 (r14 >> 3), B=1 (r12 >> 3) → 0x4B
    // Expected: 4B 8D 1C F4
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(3)),  // rbx (dest)
            Operand::MemSib { base: RegId(12), index: Some(RegId(14)), scale: Scale::X8, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lea rbx, [r12 + r14*8]");

    assert_eq!(buf.as_slice(), &[0x4B, 0x8D, 0x1C, 0xF4], "lea rbx, [r12+r14*8]");
}

#[test]
fn lea_r10_from_rdi_plus_rsi_times_8() {
    // lea r10, [rdi + rsi*8]
    // r10=10 (dest), rdi=7 (base), rsi=6 (index)
    // REX: W=1, R=1 (r10 >> 3), X=0, B=0 → 0x4C
    // Expected: 4C 8D 14 F7
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Lea,
        operands: smallvec![
            Operand::Reg(RegId(10)), // r10 (dest)
            Operand::MemSib { base: RegId(7), index: Some(RegId(6)), scale: Scale::X8, disp: 0 },
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
    };

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for lea r10, [rdi + rsi*8]");

    assert_eq!(buf.as_slice(), &[0x4C, 0x8D, 0x14, 0xF7], "lea r10, [rdi+rsi*8]");
}
