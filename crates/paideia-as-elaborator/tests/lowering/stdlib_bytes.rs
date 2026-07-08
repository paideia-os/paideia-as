//! PA-r16-007 (#1063): BytesOps typed accessor lowering.
//!
//! Integration tests for BytesOps methods (get_u8, get_u16_le, get_u32_le, get_u32_be,
//! get_u64_le, get_u64_be, put_u8, put_u16_le, put_u32_le, put_u32_be, put_u64_le,
//! put_u64_be).
//!
//! These tests verify the SysVRegs arg-marshalling path with real BytesOps recipe hits.
//! All use ArgConvention::SysVRegs: arg0 (buf) → RDI, arg1 (off) → RSI, arg2 (val) → RDX
//! (setters only).

use paideia_as_ir::{InstrMode, IrArena, instruction::{Mnemonic, Operand, IntWidth}};
use paideia_as_ir::abi;

#[test]
fn bytes_ops_get_u8_recipe_exists() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BytesOps",
        "get_u8",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(result.is_some(), "BytesOps::get_u8 should have a lowering recipe");

    let recipe = result
        .unwrap()
        .expect("get_u8 lowering should succeed");

    assert_eq!(recipe.instructions.len(), 1, "get_u8 should lower to exactly one instruction");

    let inst = &recipe.instructions[0];
    assert_eq!(
        inst.mnemonic,
        Mnemonic::MovSized { width: IntWidth::W8 },
        "get_u8 should lower to MovSized W8 mnemonic"
    );
    assert_eq!(inst.operands.len(), 2, "MovSized should have exactly two operands");
    assert_eq!(inst.mode, InstrMode::Mode64, "Instruction should use Mode64");

    // Verify arg convention: SysVRegs
    assert_eq!(
        recipe.arg_convention,
        paideia_as_elaborator::stdlib_lowering::ArgConvention::SysVRegs,
        "get_u8 should use SysVRegs convention"
    );

    // Verify first operand is Reg(RAX) — return value
    match &inst.operands[0] {
        Operand::Reg(reg) => {
            assert_eq!(*reg, abi::RAX, "first operand should be RAX (return value)");
        }
        _ => panic!("expected Reg(RAX) operand"),
    }

    // Verify second operand is MemSib{RDI, Some(RSI), X1, 0} — buf[off]
    match &inst.operands[1] {
        Operand::MemSib { base, index, scale, disp } => {
            assert_eq!(*base, abi::RDI, "base should be RDI (SysV arg0 buf)");
            assert_eq!(*index, Some(abi::RSI), "index should be RSI (SysV arg1 off)");
            assert_eq!(*scale, paideia_as_ir::instruction::Scale::X1, "scale should be X1");
            assert_eq!(*disp, 0, "displacement should be 0");
        }
        _ => panic!("expected MemSib operand"),
    }
}

#[test]
fn bytes_ops_get_u32_le_recipe_exists() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BytesOps",
        "get_u32_le",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(result.is_some(), "BytesOps::get_u32_le should have a lowering recipe");

    let recipe = result
        .unwrap()
        .expect("get_u32_le lowering should succeed");

    assert_eq!(
        recipe.instructions.len(),
        1,
        "get_u32_le should lower to exactly one instruction (load only, no byte swap)"
    );

    let inst = &recipe.instructions[0];
    assert_eq!(
        inst.mnemonic,
        Mnemonic::MovSized { width: IntWidth::W32 },
        "get_u32_le should lower to MovSized W32"
    );
    assert_eq!(inst.operands.len(), 2, "MovSized should have exactly two operands");

    // Verify arg convention: SysVRegs
    assert_eq!(
        recipe.arg_convention,
        paideia_as_elaborator::stdlib_lowering::ArgConvention::SysVRegs,
        "get_u32_le should use SysVRegs convention"
    );

    // Verify operand 0: Reg(RAX)
    match &inst.operands[0] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RAX),
        _ => panic!("expected Reg(RAX)"),
    }

    // Verify operand 1: MemSib{RDI, Some(RSI), X1, 0}
    match &inst.operands[1] {
        Operand::MemSib { base, index, scale, disp } => {
            assert_eq!(*base, abi::RDI);
            assert_eq!(*index, Some(abi::RSI));
            assert_eq!(*scale, paideia_as_ir::instruction::Scale::X1);
            assert_eq!(*disp, 0);
        }
        _ => panic!("expected MemSib operand"),
    }
}

#[test]
fn bytes_ops_get_u64_be_recipe_exists_with_bswap() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BytesOps",
        "get_u64_be",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(result.is_some(), "BytesOps::get_u64_be should have a lowering recipe");

    let recipe = result
        .unwrap()
        .expect("get_u64_be lowering should succeed");

    assert_eq!(
        recipe.instructions.len(),
        2,
        "get_u64_be should lower to two instructions (load + bswap)"
    );

    let inst0 = &recipe.instructions[0];
    let inst1 = &recipe.instructions[1];

    // First instruction: load
    assert_eq!(
        inst0.mnemonic,
        Mnemonic::MovSized { width: IntWidth::W64 },
        "first instruction should be MovSized W64"
    );
    assert_eq!(inst0.operands.len(), 2, "load should have two operands");

    // Second instruction: byte swap
    assert_eq!(
        inst1.mnemonic,
        Mnemonic::Bswap,
        "second instruction should be Bswap (W64 variant)"
    );
    assert_eq!(inst1.operands.len(), 1, "Bswap should have one operand");

    // Verify second operand of Bswap is Reg(RAX)
    match &inst1.operands[0] {
        Operand::Reg(reg) => {
            assert_eq!(*reg, abi::RAX, "Bswap should operate on RAX (return register)");
        }
        _ => panic!("expected Reg(RAX)"),
    }

    // Verify arg convention: SysVRegs
    assert_eq!(
        recipe.arg_convention,
        paideia_as_elaborator::stdlib_lowering::ArgConvention::SysVRegs,
        "get_u64_be should use SysVRegs convention"
    );
}

#[test]
fn bytes_ops_put_u64_le_recipe_exists() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BytesOps",
        "put_u64_le",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(result.is_some(), "BytesOps::put_u64_le should have a lowering recipe");

    let recipe = result
        .unwrap()
        .expect("put_u64_le lowering should succeed");

    assert_eq!(
        recipe.instructions.len(),
        2,
        "put_u64_le should lower to two instructions (Add + store)"
    );

    let inst0 = &recipe.instructions[0];
    let inst1 = &recipe.instructions[1];

    // First instruction: Add RDI, RSI (fold offset)
    assert_eq!(
        inst0.mnemonic,
        Mnemonic::Add,
        "first instruction should be Add"
    );
    assert_eq!(inst0.operands.len(), 2, "Add should have two operands");

    // Verify Add operands: RDI, RSI
    match &inst0.operands[0] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RDI, "Add dest should be RDI (buf)"),
        _ => panic!("expected Reg(RDI)"),
    }
    match &inst0.operands[1] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RSI, "Add src should be RSI (off)"),
        _ => panic!("expected Reg(RSI)"),
    }

    // Second instruction: MovSized W64 [RDI+0], RDX (store)
    assert_eq!(
        inst1.mnemonic,
        Mnemonic::MovSized { width: IntWidth::W64 },
        "second instruction should be MovSized W64"
    );
    assert_eq!(inst1.operands.len(), 2, "store should have two operands");

    // Verify store operands: MemSib{RDI, None, X1, 0}, Reg(RDX)
    match &inst1.operands[0] {
        Operand::MemSib { base, index, scale, disp } => {
            assert_eq!(*base, abi::RDI, "store address base should be RDI (buf+off after Add)");
            assert_eq!(*index, None, "store address index should be None (Add already folded RSI)");
            assert_eq!(*scale, paideia_as_ir::instruction::Scale::X1);
            assert_eq!(*disp, 0, "store address displacement should be 0");
        }
        _ => panic!("expected MemSib operand"),
    }

    match &inst1.operands[1] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RDX, "store value should be RDX (SysV arg2 val)"),
        _ => panic!("expected Reg(RDX)"),
    }

    // Verify arg convention: SysVRegs
    assert_eq!(
        recipe.arg_convention,
        paideia_as_elaborator::stdlib_lowering::ArgConvention::SysVRegs,
        "put_u64_le should use SysVRegs convention"
    );
}
