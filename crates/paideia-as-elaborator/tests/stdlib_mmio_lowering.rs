//! PA-r16-007-followup (#1057) round-trip: calls to MmioOps::mmio_read_u32
//! and MmioOps::mmio_write_u32 elaborate to MOV instructions with absolute displacement.
//!
//! This test verifies the stdlib lowering path for MmioOps methods.
//! - mmio_read_u32(addr) → mov eax, dword [addr]
//! - mmio_write_u32(addr, val) → mov dword [addr], imm

use paideia_as_ir::{InstrMode, IrArena, IrNodeId, instruction::{Mnemonic, Operand, IntWidth}};
use paideia_as_encoder::{CodeBuffer, EncodeStats, encode_instruction};

#[test]
fn mmio_read_u32_lowers_to_mov_eax_mem_disp32() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, 0x1000);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_read_u32",
        InstrMode::Mode64,
        &[addr_id],
        &arena,
    );

    assert!(
        result.is_some(),
        "MmioOps::mmio_read_u32 should have a lowering recipe"
    );

    let recipe = result
        .unwrap()
        .expect("mmio_read_u32 lowering should succeed");
    assert_eq!(
        recipe.instructions.len(),
        1,
        "mmio_read_u32 should lower to exactly one instruction"
    );

    let inst = &recipe.instructions[0];
    assert_eq!(
        inst.mnemonic,
        Mnemonic::MovSized {
            width: IntWidth::W32
        },
        "mmio_read_u32 should lower to MovSized W32 mnemonic"
    );
    assert_eq!(
        inst.operands.len(),
        2,
        "MovSized should have exactly two operands"
    );
    assert_eq!(
        inst.mode, InstrMode::Mode64,
        "Instruction should use Mode64"
    );

    // Verify first operand is Reg(RAX)
    match &inst.operands[0] {
        Operand::Reg(reg) => {
            assert_eq!(*reg, paideia_as_ir::abi::RAX, "first operand should be RAX");
        }
        _ => panic!("expected Reg(RAX) operand"),
    }

    // Verify second operand is MemDisp { 0x1000 }
    match &inst.operands[1] {
        Operand::MemDisp { disp } => {
            assert_eq!(*disp, 0x1000, "displacement should be 0x1000");
        }
        _ => panic!("expected MemDisp operand"),
    }
}

#[test]
fn mmio_write_u32_lowers_to_mov_mem_disp32_imm32() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    let val_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, 0x1000);
    arena.literal_values_mut().insert(val_id, 0x12345678);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_write_u32",
        InstrMode::Mode64,
        &[addr_id, val_id],
        &arena,
    );

    assert!(
        result.is_some(),
        "MmioOps::mmio_write_u32 should have a lowering recipe"
    );

    let recipe = result
        .unwrap()
        .expect("mmio_write_u32 lowering should succeed");
    assert_eq!(
        recipe.instructions.len(),
        1,
        "mmio_write_u32 should lower to exactly one instruction"
    );

    let inst = &recipe.instructions[0];
    assert_eq!(
        inst.mnemonic,
        Mnemonic::MovSized {
            width: IntWidth::W32
        },
        "mmio_write_u32 should lower to MovSized W32 mnemonic"
    );
    assert_eq!(
        inst.operands.len(),
        2,
        "MovSized should have exactly two operands"
    );
    assert_eq!(
        inst.mode, InstrMode::Mode64,
        "Instruction should use Mode64"
    );

    // Verify first operand is MemDisp { 0x1000 }
    match &inst.operands[0] {
        Operand::MemDisp { disp } => {
            assert_eq!(*disp, 0x1000, "displacement should be 0x1000");
        }
        _ => panic!("expected MemDisp operand"),
    }

    // Verify second operand is Imm64(0x12345678)
    match &inst.operands[1] {
        Operand::Imm64(val) => {
            assert_eq!(*val, 0x12345678, "immediate should be 0x12345678");
        }
        _ => panic!("expected Imm64 operand"),
    }
}

#[test]
fn mmio_read_u32_with_large_addr() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    // Large address within i32 range
    arena.literal_values_mut().insert(addr_id, 0x7FFFFFFF);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_read_u32",
        InstrMode::Mode64,
        &[addr_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("mmio_read_u32 should succeed with large addr");
    assert_eq!(recipe.instructions.len(), 1);

    match &recipe.instructions[0].operands[1] {
        Operand::MemDisp { disp } => {
            assert_eq!(*disp, 0x7FFFFFFF, "max i32 displacement should work");
        }
        _ => panic!("expected MemDisp"),
    }
}

#[test]
fn mmio_read_u32_non_literal_returns_error() {
    let arena = IrArena::new();
    let missing_id = IrNodeId::new(999).expect("valid node id");

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_read_u32",
        InstrMode::Mode64,
        &[missing_id],
        &arena,
    );

    assert!(result.is_some());
    let err = result.unwrap().expect_err("should error on non-literal arg");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 0);
            assert_eq!(method, "MmioOps::mmio_read_u32");
        }
    }
}

#[test]
fn mmio_write_u32_non_literal_val_returns_error() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    let missing_val = IrNodeId::new(999).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, 0x1000);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_write_u32",
        InstrMode::Mode64,
        &[addr_id, missing_val],
        &arena,
    );

    assert!(result.is_some());
    let err = result.unwrap().expect_err("should error on non-literal val");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 1);
            assert_eq!(method, "MmioOps::mmio_write_u32");
        }
    }
}

#[test]
fn mmio_read_u32_encodes_to_correct_bytes() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, 0x1000);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_read_u32",
        InstrMode::Mode64,
        &[addr_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("mmio_read_u32 lowering should succeed");
    assert_eq!(recipe.instructions.len(), 1);

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::default();
    let _output = encode_instruction(&recipe.instructions[0], &mut buf, &mut stats)
        .expect("encoding should succeed");

    // Expected bytes for: mov eax, dword [0x1000]
    // Opcode: 8B 04 25 (mov r32, r/m32 with SIB byte 04 25 for [disp32])
    // Displacement: 0x1000 in little-endian = 00 10 00 00
    // Full sequence: 8B 04 25 00 10 00 00
    let expected = vec![0x8B, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00];
    assert_eq!(
        buf.bytes, expected,
        "mmio_read_u32 should encode to mov eax, dword [0x1000]"
    );
}

#[test]
fn mmio_write_u32_encodes_to_correct_bytes() {
    let mut arena = IrArena::new();
    let addr_id = IrNodeId::new(1).expect("valid node id");
    let val_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(addr_id, 0x1000);
    arena.literal_values_mut().insert(val_id, 0x12345678);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "MmioOps",
        "mmio_write_u32",
        InstrMode::Mode64,
        &[addr_id, val_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("mmio_write_u32 lowering should succeed");
    assert_eq!(recipe.instructions.len(), 1);

    let mut buf = CodeBuffer::new();
    let mut stats = EncodeStats::default();
    let _output = encode_instruction(&recipe.instructions[0], &mut buf, &mut stats)
        .expect("encoding should succeed");

    // Expected bytes for: mov dword [0x1000], 0x12345678
    // Opcode: C7 04 25 (mov r/m32, imm32 with SIB byte 04 25 for [disp32])
    // Displacement: 0x1000 in little-endian = 00 10 00 00
    // Immediate: 0x12345678 in little-endian = 78 56 34 12
    // Full sequence: C7 04 25 00 10 00 00 78 56 34 12
    let expected = vec![0xC7, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00, 0x78, 0x56, 0x34, 0x12];
    assert_eq!(
        buf.bytes, expected,
        "mmio_write_u32 should encode to mov dword [0x1000], 0x12345678"
    );
}
