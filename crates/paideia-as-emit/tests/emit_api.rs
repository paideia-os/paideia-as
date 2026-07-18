//! Integration tests for the public emit_instruction API.
//!
//! These tests verify that emit_instruction produces correct byte sequences,
//! handles errors appropriately, and maintains the rollback contract (buffer
//! unchanged on Err).

use paideia_as_emit::{CodeBuffer, EmitError, Instruction, Mnemonic, Operand, emit_instruction};
use paideia_as_runtime::{InstrMode, RegId};
use smallvec::SmallVec;

// Helper to construct instructions more concisely
fn make_instr(mnemonic: Mnemonic, operands: Vec<Operand>) -> Instruction {
    let mut ops = SmallVec::new();
    for op in operands {
        ops.push(op);
    }
    Instruction {
        mnemonic,
        operands: ops,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: InstrMode::Mode64,
        emission_order: 0,
    }
}

// --- Success canaries (6 tests) ---

#[test]
fn emit_add_r64_r64_writes_expected_bytes() {
    let mut buf = CodeBuffer::new();

    // ADD RAX, RBX (Reg 0, Reg 3)
    let ins = make_instr(Mnemonic::Add, vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))]);

    let result = emit_instruction(&mut buf, ins);
    assert!(result.is_ok(), "ADD RAX, RBX should emit successfully");

    // ADD RAX, RBX → 48 01 D8 (3 bytes)
    assert_eq!(buf.as_slice(), &[0x48, 0x01, 0xD8]);
}

#[test]
fn emit_mov_r64_imm64_writes_10_bytes() {
    let mut buf = CodeBuffer::new();

    // MOV RAX, 0x0123456789ABCDEF
    let ins = make_instr(
        Mnemonic::Mov,
        vec![
            Operand::Reg(RegId(0)),
            Operand::Imm64(0x0123456789ABCDEF as i64),
        ],
    );

    let result = emit_instruction(&mut buf, ins);
    assert!(result.is_ok(), "MOV RAX, imm64 should emit successfully");

    // MOV RAX, 0x0123456789ABCDEF → 48 B8 EF CD AB 89 67 45 23 01 (10 bytes)
    assert_eq!(
        buf.as_slice(),
        &[0x48, 0xB8, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01]
    );
}

#[test]
fn emit_ret_writes_single_byte() {
    let mut buf = CodeBuffer::new();

    let ins = make_instr(Mnemonic::Ret, vec![]);

    let result = emit_instruction(&mut buf, ins);
    assert!(result.is_ok(), "RET should emit successfully");

    // RET → C3 (1 byte)
    assert_eq!(buf.as_slice(), &[0xC3]);
}

#[test]
fn emit_nop_writes_single_byte() {
    let mut buf = CodeBuffer::new();

    let ins = make_instr(Mnemonic::Nop, vec![]);

    let result = emit_instruction(&mut buf, ins);
    assert!(result.is_ok(), "NOP should emit successfully");

    // NOP → 90 (1 byte)
    assert_eq!(buf.as_slice(), &[0x90]);
}

#[test]
fn emit_sequence_of_three_instructions() {
    let mut buf = CodeBuffer::new();

    // Emit NOP, ADD, RET in sequence
    let nop = make_instr(Mnemonic::Nop, vec![]);
    let add = make_instr(Mnemonic::Add, vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))]);
    let ret = make_instr(Mnemonic::Ret, vec![]);

    assert!(emit_instruction(&mut buf, nop).is_ok());
    assert!(emit_instruction(&mut buf, add).is_ok());
    assert!(emit_instruction(&mut buf, ret).is_ok());

    // NOP (90) + ADD RAX, RBX (48 01 D8) + RET (C3) = 5 bytes total
    assert_eq!(buf.as_slice(), &[0x90, 0x48, 0x01, 0xD8, 0xC3]);
}

#[test]
fn emit_from_empty_buffer_starts_at_zero() {
    let mut buf = CodeBuffer::new();
    assert_eq!(buf.len(), 0);

    let ret = make_instr(Mnemonic::Ret, vec![]);

    emit_instruction(&mut buf, ret).unwrap();
    assert_eq!(buf.len(), 1);
    assert_eq!(buf.as_slice()[0], 0xC3);
}

#[test]
fn emit_appends_to_nonempty_buffer() {
    let mut buf = CodeBuffer::new();

    // Pre-fill with a NOP
    buf.bytes.push(0x90);
    assert_eq!(buf.len(), 1);

    // Emit RET
    let ret = make_instr(Mnemonic::Ret, vec![]);

    emit_instruction(&mut buf, ret).unwrap();

    // Should have [0x90, 0xC3]
    assert_eq!(buf.as_slice(), &[0x90, 0xC3]);
}

// --- Error canaries (4 tests) ---

#[test]
fn emit_operand_count_mismatch_returns_error() {
    let mut buf = CodeBuffer::new();

    // RET should have 0 operands, but we give it one
    let ins = make_instr(Mnemonic::Ret, vec![Operand::Reg(RegId(0))]);

    let result = emit_instruction(&mut buf, ins);
    assert!(
        result.is_err(),
        "RET with operand should fail with OperandCount"
    );

    match result {
        Err(EmitError::OperandCount {
            mnemonic,
            expected,
            got,
        }) => {
            assert_eq!(mnemonic, Mnemonic::Ret);
            assert_eq!(expected, 0);
            assert_eq!(got, 1);
        }
        _ => panic!("Expected OperandCount error"),
    }
}

#[test]
fn emit_operand_shape_error() {
    let mut buf = CodeBuffer::new();

    // MOV with immediate as first operand (invalid shape)
    let ins = make_instr(
        Mnemonic::Mov,
        vec![
            Operand::Imm64(0x1234567890ABCDEF as i64),
            Operand::Reg(RegId(0)),
        ],
    );

    let result = emit_instruction(&mut buf, ins);
    assert!(
        result.is_err(),
        "MOV imm, reg should fail with OperandShape or InvalidOperand"
    );

    // The encoder may return different errors depending on how it validates.
    // We just need to verify it returns an error for an invalid operand combination.
    match result {
        Err(EmitError::OperandShape { mnemonic }) => {
            assert_eq!(mnemonic, Mnemonic::Mov);
        }
        Err(EmitError::InvalidOperand) => {
            // Also acceptable - immediate as destination is invalid
        }
        Err(EmitError::Unsupported) => {
            // Also acceptable - this operand combination may not be supported
        }
        _ => panic!("Expected error for invalid operand, got {:?}", result),
    }
}

#[test]
fn emit_invalid_operand_var() {
    let mut buf = CodeBuffer::new();

    // ADD with an unresolved Var operand
    let ins = make_instr(
        Mnemonic::Add,
        vec![Operand::Var { name: "x".into() }, Operand::Reg(RegId(3))],
    );

    let result = emit_instruction(&mut buf, ins);
    assert!(
        result.is_err(),
        "ADD with Var operand should fail with InvalidOperand"
    );

    match result {
        Err(EmitError::InvalidOperand) => {}
        _ => panic!("Expected InvalidOperand error"),
    }
}

#[test]
fn emit_unresolved_symbol_ref_returns_error() {
    let mut buf = CodeBuffer::new();

    // CALL to an unresolved symbol
    let ins = make_instr(
        Mnemonic::Call,
        vec![Operand::SymbolRef {
            name: "printf".into(),
            addend: 0,
        }],
    );

    let result = emit_instruction(&mut buf, ins);
    assert!(
        result.is_err(),
        "CALL SymbolRef should fail with UnresolvedRelocation"
    );

    match result {
        Err(EmitError::UnresolvedRelocation) => {}
        _ => panic!("Expected UnresolvedRelocation error"),
    }
}

#[test]
fn emit_unresolved_label_ref_returns_error() {
    let mut buf = CodeBuffer::new();

    // JMP to an unresolved label
    let ins = make_instr(
        Mnemonic::Jmp,
        vec![Operand::LabelRef {
            name: "loop".into(),
            addend: 0,
        }],
    );

    let result = emit_instruction(&mut buf, ins);
    assert!(
        result.is_err(),
        "JMP LabelRef should fail with UnresolvedRelocation"
    );

    match result {
        Err(EmitError::UnresolvedRelocation) => {}
        _ => panic!("Expected UnresolvedRelocation error"),
    }
}

#[test]
fn emit_unresolved_mem_rip_rel_sym_returns_error() {
    let mut buf = CodeBuffer::new();

    // MOV RAX, [RIP + symbol]
    let ins = make_instr(
        Mnemonic::Mov,
        vec![
            Operand::Reg(RegId(0)),
            Operand::MemRipRelSym {
                name: "data".into(),
                addend: 0,
            },
        ],
    );

    let result = emit_instruction(&mut buf, ins);
    assert!(
        result.is_err(),
        "MOV with MemRipRelSym should fail with UnresolvedRelocation"
    );

    match result {
        Err(EmitError::UnresolvedRelocation) => {}
        _ => panic!("Expected UnresolvedRelocation error"),
    }
}

#[test]
fn emit_unresolved_mem_sym_indexed_returns_error() {
    let mut buf = CodeBuffer::new();

    // MOV RAX, [symbol + RBX*8]
    let ins = make_instr(
        Mnemonic::Mov,
        vec![
            Operand::Reg(RegId(0)),
            Operand::MemSymIndexed {
                name: "table".into(),
                addend: 0,
                index: RegId(3),
                scale: paideia_as_runtime::Scale::X8,
            },
        ],
    );

    let result = emit_instruction(&mut buf, ins);
    assert!(
        result.is_err(),
        "MOV with MemSymIndexed should fail with UnresolvedRelocation"
    );

    match result {
        Err(EmitError::UnresolvedRelocation) => {}
        _ => panic!("Expected UnresolvedRelocation error"),
    }
}

// --- Discipline canaries (2 tests) ---

#[test]
fn emit_error_rolls_back_buffer() {
    let mut buf = CodeBuffer::new();

    // Pre-fill with two NOPs
    buf.bytes.push(0x90);
    buf.bytes.push(0x90);
    assert_eq!(buf.as_slice(), &[0x90, 0x90]);

    // Try to emit MOV with invalid operand shape (should fail and roll back)
    let ins = make_instr(
        Mnemonic::Mov,
        vec![
            Operand::Imm64(0x1234567890ABCDEF as i64),
            Operand::Reg(RegId(0)),
        ],
    );

    let result = emit_instruction(&mut buf, ins);
    assert!(result.is_err(), "Should fail with OperandShape");

    // Buffer should be unchanged
    assert_eq!(buf.as_slice(), &[0x90, 0x90]);
}

#[test]
fn emit_unresolved_relocation_rolls_back_buffer() {
    let mut buf = CodeBuffer::new();

    // Pre-fill with a NOP
    buf.bytes.push(0x90);
    assert_eq!(buf.as_slice(), &[0x90]);

    // Try to emit CALL to unresolved symbol (should fail and roll back)
    let ins = make_instr(
        Mnemonic::Call,
        vec![Operand::SymbolRef {
            name: "printf".into(),
            addend: 0,
        }],
    );

    let result = emit_instruction(&mut buf, ins);
    assert!(
        matches!(result, Err(EmitError::UnresolvedRelocation)),
        "Should fail with UnresolvedRelocation"
    );

    // Buffer should be unchanged (no zero-filled placeholder bytes)
    assert_eq!(buf.as_slice(), &[0x90]);
}

#[test]
fn emit_iced_roundtrip_add_r64_r64() {
    use iced_x86::{Decoder, DecoderOptions};

    let mut buf = CodeBuffer::new();

    // Emit ADD RAX, RBX
    let ins = make_instr(Mnemonic::Add, vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))]);

    emit_instruction(&mut buf, ins).unwrap();

    // Decode the emitted bytes using iced-x86
    let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
    let decoded = decoder.decode();

    // Verify the decoded instruction is ADD
    assert_eq!(decoded.mnemonic(), iced_x86::Mnemonic::Add);
}
