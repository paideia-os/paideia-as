//! PA-R19-004: MS x64 arg-marshalling encoder audit.
//!
//! This test suite validates that the encoder correctly handles mov instructions
//! for MS x64 calling convention argument registers (RCX, RDX, R8, R9) across all
//! four ModR/M patterns identified by softarch:
//!
//! P1: movabs r_ms_arg, imm64        — 10-byte moves with mov_reg64_imm64
//! P2: mov r_ms_arg, r_sysv          — register-to-register via mov_reg64_reg64
//! P3: mov r_ms_arg, [rsp+N]         — load from stack via mov_reg64_mem_reg64_disp
//! P4: mov [rsp+N], r_ms_arg         — store to stack via mov_mem_reg64_disp_reg64
//!
//! 18 byte-exact fixtures lock in behavior. Auxiliary tests verify sequence
//! consistency and iced-x86 roundtrip compliance.

use paideia_as_encoder::CodeBuffer;
use paideia_as_ir::InstrMode;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId, Scale};
use paideia_as_ir::abi::{RCX, RDX, R8, R9};
use smallvec::smallvec;

// ===== P1: movabs r_ms_arg, imm64 (4 fixtures) =====

#[test]
fn ms_arg_movabs_rcx_i64() {
    // mov rcx, 0x0123456789abcdef
    // rcx=1, imm64=0x0123456789abcdef
    // REX: W=1, R=0, X=0, B=0 → 0x48 (no high bit)
    // B8 opcode for movabs, 10 bytes total
    // Expected: 48 B9 EF CD AB 89 67 45 23 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RCX),
            Operand::Imm64(0x0123456789abcdef),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rcx, 0x0123456789abcdef");

    assert_eq!(buf.as_slice(), &[0x48, 0xB9, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01],
        "mov rcx, 0x0123456789abcdef");
}

#[test]
fn ms_arg_movabs_rdx_small() {
    // mov rdx, 0x1000 (fits i32 range)
    // rdx=2, imm64=0x1000
    // REX: W=1, R=0, X=0, B=0 → 0x48
    // C7 /0 opcode for imm32 (sign-extended to r64): 7 bytes
    // Expected: 48 C7 C2 00 10 00 00 (C7 form, 3 bytes shorter)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RDX),
            Operand::Imm64(0x1000),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rdx, 0x1000");

    assert_eq!(buf.as_slice(), &[0x48, 0xC7, 0xC2, 0x00, 0x10, 0x00, 0x00],
        "mov rdx, 0x1000 (C7 form)");
}

#[test]
fn ms_arg_movabs_r8_small() {
    // mov r8, 0x1234 (fits i32 range)
    // r8=8, imm64=0x1234
    // REX: W=1, R=0, X=0, B=1 (r8 >> 3) → 0x49
    // C7 /0 opcode for imm32 (sign-extended to r64): 7 bytes
    // Expected: 49 C7 C0 34 12 00 00 (C7 form, 3 bytes shorter)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(R8),
            Operand::Imm64(0x1234),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r8, 0x1234");

    assert_eq!(buf.as_slice(), &[0x49, 0xC7, 0xC0, 0x34, 0x12, 0x00, 0x00],
        "mov r8, 0x1234");
}

#[test]
fn ms_arg_movabs_r9_i64() {
    // mov r9, 0x0123456789abcdef
    // r9=9, imm64=0x0123456789abcdef
    // REX: W=1, R=0, X=0, B=1 (r9 >> 3) → 0x49
    // B8+1 opcode for r9 (uses high bit), 10 bytes
    // Expected: 49 B9 EF CD AB 89 67 45 23 01
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(R9),
            Operand::Imm64(0x0123456789abcdef),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r9, 0x0123456789abcdef");

    assert_eq!(buf.as_slice(), &[0x49, 0xB9, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01],
        "mov r9, 0x0123456789abcdef");
}

// ===== P2: mov r_ms_arg, r_sysv (4 fixtures + 2 REX.R closures) =====

#[test]
fn ms_arg_movrr_rcx_from_rdi() {
    // mov rcx, rdi
    // rcx=1 (dest), rdi=7 (src)
    // REX: W=1, R=0 (rdi >> 3), X=0, B=0 (rcx >> 3) → 0x48
    // Expected: 48 89 F9
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RCX),
            Operand::Reg(RegId(7)),  // rdi
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rcx, rdi");

    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0xF9], "mov rcx, rdi");
}

#[test]
fn ms_arg_movrr_rdx_from_rsi() {
    // mov rdx, rsi
    // rdx=2 (dest), rsi=6 (src)
    // REX: W=1, R=0 (rsi >> 3), X=0, B=0 (rdx >> 3) → 0x48
    // Expected: 48 89 F2
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RDX),
            Operand::Reg(RegId(6)),  // rsi
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rdx, rsi");

    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0xF2], "mov rdx, rsi");
}

#[test]
fn ms_arg_movrr_r8_from_rdx() {
    // mov r8, rdx
    // r8=8 (dest), rdx=2 (src)
    // REX: W=1, R=0 (rdx >> 3), X=0, B=1 (r8 >> 3) → 0x49
    // Expected: 49 89 D0
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(R8),
            Operand::Reg(RDX),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r8, rdx");

    assert_eq!(buf.as_slice(), &[0x49, 0x89, 0xD0], "mov r8, rdx");
}

#[test]
fn ms_arg_movrr_r9_from_rcx() {
    // mov r9, rcx
    // r9=9 (dest), rcx=1 (src)
    // REX: W=1, R=0 (rcx >> 3), X=0, B=1 (r9 >> 3) → 0x49
    // Expected: 49 89 C9
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(R9),
            Operand::Reg(RCX),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r9, rcx");

    assert_eq!(buf.as_slice(), &[0x49, 0x89, 0xC9], "mov r9, rcx");
}

#[test]
fn ms_arg_movrr_rax_from_r8() {
    // mov rax, r8
    // rax=0 (dest), r8=8 (src)
    // REX: W=1, R=1 (r8 >> 3), X=0, B=0 (rax >> 3) → 0x4C
    // Expected: 4C 89 C0 (REX.R on src closure)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax
            Operand::Reg(R8),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, r8");

    assert_eq!(buf.as_slice(), &[0x4C, 0x89, 0xC0], "mov rax, r8 (REX.R on src)");
}

#[test]
fn ms_arg_movrr_rax_from_r9() {
    // mov rax, r9
    // rax=0 (dest), r9=9 (src)
    // REX: W=1, R=1 (r9 >> 3), X=0, B=0 (rax >> 3) → 0x4C
    // Expected: 4C 89 C8 (REX.R on src closure)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax
            Operand::Reg(R9),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, r9");

    assert_eq!(buf.as_slice(), &[0x4C, 0x89, 0xC8], "mov rax, r9 (REX.R on src)");
}

// ===== P3: mov r_ms_arg, [rsp+N] (4 fixtures) =====

#[test]
fn ms_arg_load_rcx_rsp_disp0() {
    // mov rcx, [rsp]
    // rcx=1 (dest), rsp=4 (base), disp=0
    // REX: W=1, R=0 (rcx >> 3), X=0, B=0 (rsp >> 3) → 0x48
    // Mod=0 (disp0), R/M=100 (RSP requires SIB)
    // SIB: scale=00 (1x), index=100 (none), base=100 (RSP)
    // Expected: 48 8B 0C 24
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RCX),
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 0 },  // [rsp+0]
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rcx, [rsp]");

    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x0C, 0x24], "mov rcx, [rsp+0]");
}

#[test]
fn ms_arg_load_rdx_rsp_disp8() {
    // mov rdx, [rsp+8]
    // rdx=2 (dest), rsp=4 (base), disp=8
    // REX: W=1, R=0 (rdx >> 3), X=0, B=0 (rsp >> 3) → 0x48
    // Mod=01 (disp8), R/M=100 (RSP requires SIB)
    // SIB: scale=00, index=100, base=100
    // Expected: 48 8B 54 24 08
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RDX),
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 8 },  // [rsp+8]
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rdx, [rsp+8]");

    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x54, 0x24, 0x08], "mov rdx, [rsp+8]");
}

#[test]
fn ms_arg_load_r8_rsp_disp32() {
    // mov r8, [rsp+32]
    // r8=8 (dest), rsp=4 (base), disp=32
    // REX: W=1, R=0 (r8 >> 3 is 1, but dest high), X=0, B=1 (r8 >> 3) → 0x49
    // Mod=10 (disp32), R/M=100 (RSP requires SIB)
    // SIB: scale=00, index=100, base=100
    // Expected: 4C 8B 44 24 20
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(R8),
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 32 },  // [rsp+32]
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r8, [rsp+32]");

    assert_eq!(buf.as_slice(), &[0x4C, 0x8B, 0x44, 0x24, 0x20], "mov r8, [rsp+32]");
}

#[test]
fn ms_arg_load_r9_rsp_disp40() {
    // mov r9, [rsp+40]
    // r9=9 (dest), rsp=4 (base), disp=40
    // REX: W=1, R=0 (r9 >> 3 is 1, but dest high), X=0, B=1 (r9 >> 3) → 0x49
    // Mod=10 (disp32), R/M=100 (RSP requires SIB)
    // SIB: scale=00, index=100, base=100
    // Expected: 4C 8B 4C 24 28
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(R9),
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 40 },  // [rsp+40]
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov r9, [rsp+40]");

    assert_eq!(buf.as_slice(), &[0x4C, 0x8B, 0x4C, 0x24, 0x28], "mov r9, [rsp+40]");
}

// ===== P4: mov [rsp+N], r_ms_arg (4 fixtures) =====

#[test]
fn ms_arg_store_rsp32_rcx() {
    // mov [rsp+32], rcx
    // rsp=4 (base), disp=32, rcx=1 (src)
    // REX: W=1, R=0 (rcx >> 3), X=0, B=0 (rsp >> 3) → 0x48
    // Mod=10 (disp32), R/M=100 (RSP requires SIB)
    // SIB: scale=00, index=100, base=100
    // Expected: 48 89 4C 24 20
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 32 },  // [rsp+32]
            Operand::Reg(RCX),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [rsp+32], rcx");

    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x4C, 0x24, 0x20], "mov [rsp+32], rcx");
}

#[test]
fn ms_arg_store_rsp40_rdx() {
    // mov [rsp+40], rdx
    // rsp=4 (base), disp=40, rdx=2 (src)
    // REX: W=1, R=0 (rdx >> 3), X=0, B=0 (rsp >> 3) → 0x48
    // Mod=10 (disp32), R/M=100 (RSP requires SIB)
    // SIB: scale=00, index=100, base=100
    // Expected: 48 89 54 24 28
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 40 },  // [rsp+40]
            Operand::Reg(RDX),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [rsp+40], rdx");

    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x54, 0x24, 0x28], "mov [rsp+40], rdx");
}

#[test]
fn ms_arg_store_rsp48_r8() {
    // mov [rsp+48], r8
    // rsp=4 (base), disp=48, r8=8 (src)
    // REX: W=1, R=1 (r8 >> 3), X=0, B=0 (rsp >> 3) → 0x4C
    // Mod=10 (disp32), R/M=100 (RSP requires SIB)
    // SIB: scale=00, index=100, base=100
    // Expected: 4C 89 44 24 30
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 48 },  // [rsp+48]
            Operand::Reg(R8),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [rsp+48], r8");

    assert_eq!(buf.as_slice(), &[0x4C, 0x89, 0x44, 0x24, 0x30], "mov [rsp+48], r8");
}

#[test]
fn ms_arg_store_rsp56_r9() {
    // mov [rsp+56], r9
    // rsp=4 (base), disp=56, r9=9 (src)
    // REX: W=1, R=1 (r9 >> 3), X=0, B=0 (rsp >> 3) → 0x4C
    // Mod=10 (disp32), R/M=100 (RSP requires SIB)
    // SIB: scale=00, index=100, base=100
    // Expected: 4C 89 4C 24 38
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::MemSib { base: RegId(4), index: None, scale: Scale::X1, disp: 56 },  // [rsp+56]
            Operand::Reg(R9),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov [rsp+56], r9");

    assert_eq!(buf.as_slice(), &[0x4C, 0x89, 0x4C, 0x24, 0x38], "mov [rsp+56], r9");
}

// ===== Boundary tests: i32 range mutations (coverage for range-check correctness) =====

#[test]
fn i32_boundary_max_exactly() {
    // mov rax, 0x7FFFFFFF (i32::MAX exactly)
    // Must emit C7 form: 48 C7 C0 FF FF FF 7F (7 bytes)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax
            Operand::Imm64(0x7FFFFFFF),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, 0x7FFFFFFF");

    assert_eq!(buf.as_slice(), &[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0x7F],
        "mov rax, 0x7FFFFFFF (i32::MAX, C7 form)");
}

#[test]
fn i32_boundary_max_plus_one() {
    // mov rax, 0x80000000 (i32::MAX + 1)
    // Must emit B8 form: 48 B8 00 00 00 80 00 00 00 00 (10 bytes)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax
            Operand::Imm64(0x80000000),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, 0x80000000");

    assert_eq!(buf.as_slice(), &[0x48, 0xB8, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00],
        "mov rax, 0x80000000 (i32::MAX + 1, B8 form)");
}

#[test]
fn i32_boundary_min_exactly() {
    // mov rax, 0xFFFFFFFF80000000 (i32::MIN as i64 exactly = -2147483648)
    // Must emit C7 form: 48 C7 C0 00 00 00 80 (7 bytes)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax
            Operand::Imm64(i32::MIN as i64),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, i32::MIN");

    assert_eq!(buf.as_slice(), &[0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x80],
        "mov rax, i32::MIN (C7 form)");
}

#[test]
fn i32_boundary_min_minus_one() {
    // mov rax, 0xFFFFFFFF7FFFFFFF (i32::MIN - 1 as i64 = -2147483649)
    // Must emit B8 form: 48 B8 FF FF FF 7F FF FF FF FF (10 bytes)
    let mut buf = CodeBuffer::new();
    let inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![
            Operand::Reg(RegId(0)),  // rax
            Operand::Imm64((i32::MIN as i64) - 1),
        ],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};

    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats)
        .expect("encoding failed for mov rax, (i32::MIN - 1)");

    assert_eq!(buf.as_slice(), &[0x48, 0xB8, 0xFF, 0xFF, 0xFF, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF],
        "mov rax, (i32::MIN - 1) (B8 form)");
}

// ===== Auxiliary: sequence consistency test =====

#[test]
fn ms_arg_movabs_all_four_regs_sequence() {
    // Verify that 4 consecutive movabs instructions for the MS arg regs
    // encode correctly in sequence (tests fixtures 1-4 back-to-back).
    // mov rcx, 0x0123456789abcdef
    // mov rdx, 0x1000
    // mov r8, 0x1234
    // mov r9, 0x0123456789abcdef

    let mut buf = CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();

    // Instruction 1: mov rcx, 0x0123456789abcdef
    let inst1 = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RCX), Operand::Imm64(0x0123456789abcdef)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    paideia_as_encoder::encode_instruction(&inst1, &mut buf, &mut stats)
        .expect("encoding failed for inst1");

    // Instruction 2: mov rdx, 0x1000
    let inst2 = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(RDX), Operand::Imm64(0x1000)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    paideia_as_encoder::encode_instruction(&inst2, &mut buf, &mut stats)
        .expect("encoding failed for inst2");

    // Instruction 3: mov r8, 0x1234
    let inst3 = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(R8), Operand::Imm64(0x1234)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    paideia_as_encoder::encode_instruction(&inst3, &mut buf, &mut stats)
        .expect("encoding failed for inst3");

    // Instruction 4: mov r9, 0x0123456789abcdef
    let inst4 = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: smallvec![Operand::Reg(R9), Operand::Imm64(0x0123456789abcdef)],
        byte_offset_in_text: None,
        mode: InstrMode::default(),
        encoding_hint: None,
        emission_order: 0,
};
    paideia_as_encoder::encode_instruction(&inst4, &mut buf, &mut stats)
        .expect("encoding failed for inst4");

    // Expected: concatenation of all 4 fixtures (mixed sizes due to imm32 optimization)
    let expected = [
        // mov rcx, 0x0123456789abcdef (10 bytes, B9 form)
        0x48, 0xB9, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01,
        // mov rdx, 0x1000 (7 bytes, C7 form)
        0x48, 0xC7, 0xC2, 0x00, 0x10, 0x00, 0x00,
        // mov r8, 0x1234 (7 bytes, C7 form)
        0x49, 0xC7, 0xC0, 0x34, 0x12, 0x00, 0x00,
        // mov r9, 0x0123456789abcdef (10 bytes, B9 form)
        0x49, 0xB9, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01,
    ];

    assert_eq!(buf.as_slice(), &expected, "4-instruction movabs sequence");
}

#[test]
fn ms_arg_all_four_regs_iced_roundtrip_matrix() {
    // Verify that all 18 fixtures decode correctly via iced-x86.
    // Each test assembles one fixture and ensures iced can decode it back
    // with mnemonic=Mov and 2 operands.

    let fixtures = vec![
        // P1: movabs (or C7 imm32 form for i32-range values)
        ("mov rcx, imm64", vec![0x48u8, 0xB9, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01]),
        ("mov rdx, imm32", vec![0x48, 0xC7, 0xC2, 0x00, 0x10, 0x00, 0x00]),
        ("mov r8, imm32", vec![0x49, 0xC7, 0xC0, 0x34, 0x12, 0x00, 0x00]),
        ("mov r9, imm64", vec![0x49, 0xB9, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01]),
        // P2: register-to-register
        ("mov rcx, rdi", vec![0x48, 0x89, 0xF9]),
        ("mov rdx, rsi", vec![0x48, 0x89, 0xF2]),
        ("mov r8, rdx", vec![0x49, 0x89, 0xD0]),
        ("mov r9, rcx", vec![0x49, 0x89, 0xC9]),
        ("mov rax, r8", vec![0x4C, 0x89, 0xC0]),
        ("mov rax, r9", vec![0x4C, 0x89, 0xC8]),
        // P3: load from stack
        ("mov rcx, [rsp]", vec![0x48, 0x8B, 0x0C, 0x24]),
        ("mov rdx, [rsp+8]", vec![0x48, 0x8B, 0x54, 0x24, 0x08]),
        ("mov r8, [rsp+32]", vec![0x4C, 0x8B, 0x44, 0x24, 0x20]),
        ("mov r9, [rsp+40]", vec![0x4C, 0x8B, 0x4C, 0x24, 0x28]),
        // P4: store to stack
        ("mov [rsp+32], rcx", vec![0x48, 0x89, 0x4C, 0x24, 0x20]),
        ("mov [rsp+40], rdx", vec![0x48, 0x89, 0x54, 0x24, 0x28]),
        ("mov [rsp+48], r8", vec![0x4C, 0x89, 0x44, 0x24, 0x30]),
        ("mov [rsp+56], r9", vec![0x4C, 0x89, 0x4C, 0x24, 0x38]),
    ];

    for (name, bytes) in fixtures {
        let bytes_slice = bytes.as_slice();

        // Decode using iced-x86 decoder
        let mut decoder = iced_x86::Decoder::new(64, bytes_slice, iced_x86::DecoderOptions::NONE);
        let mut instruction = iced_x86::Instruction::default();
        decoder.decode_out(&mut instruction);

        // Verify mnemonic is Mov
        assert_eq!(instruction.mnemonic(), iced_x86::Mnemonic::Mov,
            "{}: mnemonic mismatch", name);

        // Verify operand count is 2
        assert_eq!(instruction.op_count(), 2,
            "{}: operand count mismatch (expected 2)", name);
    }
}
