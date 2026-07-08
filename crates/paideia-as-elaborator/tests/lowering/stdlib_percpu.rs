//! PA-r16-007-followup (#1056) round-trip: calls to PerCpuOps::percpu_inc
//! and PerCpuOps::percpu_add elaborate to GS-prefixed lock instructions.
//!
//! This test verifies the stdlib lowering path for PerCpuOps methods.
//! - percpu_inc(offset) → lock inc qword [gs:offset]
//! - percpu_add(offset, val) → lock add qword [gs:offset], imm

use paideia_as_ir::{InstrMode, IrArena, IrNodeId, instruction::{Mnemonic, Operand, SegPrefix, IntWidth}};

#[test]
fn percpu_inc_lowers_to_gs_lock_inc() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    arena.literal_values_mut().insert(offset_id, 0x1000);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_inc",
        InstrMode::Mode64,
        &[offset_id],
        &arena,
    );

    assert!(
        result.is_some(),
        "PerCpuOps::percpu_inc should have a lowering recipe"
    );

    let recipe = result
        .unwrap()
        .expect("percpu_inc lowering should succeed");
    assert_eq!(
        recipe.instructions.len(),
        1,
        "percpu_inc should lower to exactly one instruction"
    );

    let inst = &recipe.instructions[0];
    assert_eq!(
        inst.mnemonic,
        Mnemonic::LockInc {
            width: IntWidth::W64
        },
        "percpu_inc should lower to LockInc W64 mnemonic"
    );
    assert_eq!(
        inst.operands.len(),
        1,
        "LockInc should have exactly one operand"
    );
    assert_eq!(
        inst.mode, InstrMode::Mode64,
        "Instruction should use Mode64"
    );

    // Verify the operand is MemSeg { Gs, MemDisp { 0x1000 } }
    match &inst.operands[0] {
        Operand::MemSeg { seg, inner } => {
            assert_eq!(*seg, SegPrefix::Gs, "segment should be Gs");
            match inner.as_ref() {
                Operand::MemDisp { disp } => {
                    assert_eq!(*disp, 0x1000, "displacement should be 0x1000");
                }
                _ => panic!("expected MemDisp inner operand"),
            }
        }
        _ => panic!("expected MemSeg operand"),
    }
}

#[test]
fn percpu_add_lowers_to_gs_lock_add() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    let value_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(offset_id, 0x2000);
    arena.literal_values_mut().insert(value_id, 5);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[offset_id, value_id],
        &arena,
    );

    assert!(
        result.is_some(),
        "PerCpuOps::percpu_add should have a lowering recipe"
    );

    let recipe = result
        .unwrap()
        .expect("percpu_add lowering should succeed");
    assert_eq!(
        recipe.instructions.len(),
        1,
        "percpu_add should lower to exactly one instruction"
    );

    let inst = &recipe.instructions[0];
    assert_eq!(
        inst.mnemonic,
        Mnemonic::LockAdd {
            width: IntWidth::W64
        },
        "percpu_add should lower to LockAdd W64 mnemonic"
    );
    assert_eq!(
        inst.operands.len(),
        2,
        "LockAdd should have exactly two operands"
    );
    assert_eq!(
        inst.mode, InstrMode::Mode64,
        "Instruction should use Mode64"
    );

    // Verify the first operand is MemSeg { Gs, MemDisp { 0x2000 } }
    match &inst.operands[0] {
        Operand::MemSeg { seg, inner } => {
            assert_eq!(*seg, SegPrefix::Gs, "segment should be Gs");
            match inner.as_ref() {
                Operand::MemDisp { disp } => {
                    assert_eq!(*disp, 0x2000, "displacement should be 0x2000");
                }
                _ => panic!("expected MemDisp inner operand"),
            }
        }
        _ => panic!("expected MemSeg operand"),
    }

    // Verify the second operand is Imm64(5)
    match &inst.operands[1] {
        Operand::Imm64(val) => {
            assert_eq!(*val, 5, "immediate should be 5");
        }
        _ => panic!("expected Imm64 operand"),
    }
}

#[test]
fn percpu_add_with_large_offset() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    let value_id = IrNodeId::new(2).expect("valid node id");
    // Large offset within i32 range
    arena.literal_values_mut().insert(offset_id, 0x7FFFFFFF);
    arena.literal_values_mut().insert(value_id, 100);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[offset_id, value_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("percpu_add should succeed with large offset");
    assert_eq!(recipe.instructions.len(), 1);

    match &recipe.instructions[0].operands[0] {
        Operand::MemSeg { seg: _, inner } => match inner.as_ref() {
            Operand::MemDisp { disp } => {
                assert_eq!(*disp, 0x7FFFFFFF, "max i32 displacement should work");
            }
            _ => panic!("expected MemDisp"),
        },
        _ => panic!("expected MemSeg"),
    }
}

#[test]
fn percpu_inc_non_literal_returns_error() {
    let arena = IrArena::new();
    let missing_id = IrNodeId::new(999).expect("valid node id");

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_inc",
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
            assert_eq!(method, "PerCpuOps::percpu_inc");
        }
    }
}

#[test]
fn percpu_add_non_literal_offset_returns_error() {
    let mut arena = IrArena::new();
    let missing_offset = IrNodeId::new(999).expect("valid node id");
    let value_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(value_id, 5);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[missing_offset, value_id],
        &arena,
    );

    assert!(result.is_some());
    let err = result.unwrap().expect_err("should error on non-literal offset");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 0);
            assert_eq!(method, "PerCpuOps::percpu_add");
        }
    }
}

#[test]
fn percpu_add_non_literal_value_returns_error() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    let missing_value = IrNodeId::new(999).expect("valid node id");
    arena.literal_values_mut().insert(offset_id, 0x1000);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[offset_id, missing_value],
        &arena,
    );

    assert!(result.is_some());
    let err = result.unwrap().expect_err("should error on non-literal value");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 1);
            assert_eq!(method, "PerCpuOps::percpu_add");
        }
    }
}

#[test]
fn percpu_inc_offset_out_of_i32_range_returns_error() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    // Value larger than i32::MAX
    arena.literal_values_mut().insert(offset_id, i32::MAX as i64 + 1);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_inc",
        InstrMode::Mode64,
        &[offset_id],
        &arena,
    );

    assert!(result.is_some());
    let err = result.unwrap().expect_err("should error on offset out of range");
    match err {
        paideia_as_elaborator::stdlib_lowering::StdlibLoweringError::NonLiteralArg {
            arg_index,
            method,
        } => {
            assert_eq!(arg_index, 0);
            assert_eq!(method, "PerCpuOps::percpu_inc");
        }
    }
}

#[test]
fn percpu_add_with_negative_offset() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    let value_id = IrNodeId::new(2).expect("valid node id");
    // Negative offset (still within i32 range)
    arena.literal_values_mut().insert(offset_id, -256);
    arena.literal_values_mut().insert(value_id, 10);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[offset_id, value_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("percpu_add should succeed with negative offset");
    assert_eq!(recipe.instructions.len(), 1);

    match &recipe.instructions[0].operands[0] {
        Operand::MemSeg { seg: _, inner } => match inner.as_ref() {
            Operand::MemDisp { disp } => {
                assert_eq!(*disp, -256, "negative displacement should work");
            }
            _ => panic!("expected MemDisp"),
        },
        _ => panic!("expected MemSeg"),
    }
}

#[test]
fn percpu_add_with_negative_immediate() {
    let mut arena = IrArena::new();
    let offset_id = IrNodeId::new(1).expect("valid node id");
    let value_id = IrNodeId::new(2).expect("valid node id");
    arena.literal_values_mut().insert(offset_id, 0x1000);
    // Negative immediate value
    arena.literal_values_mut().insert(value_id, -1);

    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "PerCpuOps",
        "percpu_add",
        InstrMode::Mode64,
        &[offset_id, value_id],
        &arena,
    );

    let recipe = result
        .unwrap()
        .expect("percpu_add should succeed with negative immediate");
    assert_eq!(recipe.instructions.len(), 1);

    match &recipe.instructions[0].operands[1] {
        Operand::Imm64(val) => {
            assert_eq!(*val, -1, "negative immediate should work");
        }
        _ => panic!("expected Imm64"),
    }
}
