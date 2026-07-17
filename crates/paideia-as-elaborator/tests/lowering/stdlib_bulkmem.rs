//! #1228 (Phase 2 of #1064): BulkMemOps REP-string bulk-memory lowering.
//!
//! Integration tests for BulkMemOps methods (memcpy, memset, memcpy_qwords,
//! memset_qwords).
//!
//! These tests verify the SysVRegs arg-marshalling path with real BulkMemOps
//! recipe hits. All use ArgConvention::SysVRegs: arg0 (dest) → RDI, arg1
//! (src/fill) → RSI, arg2 (count) → RDX. Each recipe first moves the SysV
//! count (RDX) into RCX because the REP prefix uses RCX as its implicit
//! counter; the fill variants additionally move RSI into RAX so the STOS
//! opcode stores AL/RAX.

use paideia_as_ir::abi;
use paideia_as_ir::{InstrMode, IrArena, instruction::{Mnemonic, Operand}};

#[test]
fn bulkmem_ops_memcpy_recipe_exists() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BulkMemOps",
        "memcpy",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(result.is_some(), "BulkMemOps::memcpy should have a lowering recipe");

    let recipe = result
        .unwrap()
        .expect("memcpy lowering should succeed");

    assert_eq!(
        recipe.instructions.len(),
        2,
        "memcpy should lower to two instructions (mov rcx, rdx + rep movsb)"
    );

    // Verify arg convention: SysVRegs
    assert_eq!(
        recipe.arg_convention,
        paideia_as_elaborator::stdlib_lowering::ArgConvention::SysVRegs,
        "memcpy should use SysVRegs convention"
    );

    // First instruction: mov rcx, rdx (REP implicit counter)
    let inst0 = &recipe.instructions[0];
    assert_eq!(inst0.mnemonic, Mnemonic::Mov, "first instruction should be Mov");
    assert_eq!(inst0.operands.len(), 2, "Mov should have two operands");
    match &inst0.operands[0] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RCX, "mov dest should be RCX"),
        _ => panic!("expected Reg(RCX)"),
    }
    match &inst0.operands[1] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RDX, "mov src should be RDX (SysV arg2 count)"),
        _ => panic!("expected Reg(RDX)"),
    }

    // Terminal instruction: rep movsb (zero-arity)
    let inst1 = &recipe.instructions[1];
    assert_eq!(inst1.mnemonic, Mnemonic::RepMovsb, "terminal instruction should be RepMovsb");
    assert!(inst1.operands.is_empty(), "rep movsb takes no explicit operands");
}

#[test]
fn bulkmem_ops_memset_recipe_exists() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BulkMemOps",
        "memset",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(result.is_some(), "BulkMemOps::memset should have a lowering recipe");

    let recipe = result
        .unwrap()
        .expect("memset lowering should succeed");

    assert_eq!(
        recipe.instructions.len(),
        3,
        "memset should lower to three instructions (mov rax, rsi + mov rcx, rdx + rep stosb)"
    );

    // Verify arg convention: SysVRegs
    assert_eq!(
        recipe.arg_convention,
        paideia_as_elaborator::stdlib_lowering::ArgConvention::SysVRegs,
        "memset should use SysVRegs convention"
    );

    // First instruction: mov rax, rsi (STOSB stores AL)
    let inst0 = &recipe.instructions[0];
    assert_eq!(inst0.mnemonic, Mnemonic::Mov, "first instruction should be Mov");
    match &inst0.operands[0] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RAX, "mov dest should be RAX"),
        _ => panic!("expected Reg(RAX)"),
    }
    match &inst0.operands[1] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RSI, "mov src should be RSI (SysV arg1 fill)"),
        _ => panic!("expected Reg(RSI)"),
    }

    // Second instruction: mov rcx, rdx (REP implicit counter)
    let inst1 = &recipe.instructions[1];
    assert_eq!(inst1.mnemonic, Mnemonic::Mov, "second instruction should be Mov");
    match &inst1.operands[0] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RCX, "mov dest should be RCX"),
        _ => panic!("expected Reg(RCX)"),
    }
    match &inst1.operands[1] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RDX, "mov src should be RDX (SysV arg2 count)"),
        _ => panic!("expected Reg(RDX)"),
    }

    // Terminal instruction: rep stosb (zero-arity)
    let inst2 = &recipe.instructions[2];
    assert_eq!(inst2.mnemonic, Mnemonic::RepStosb, "terminal instruction should be RepStosb");
    assert!(inst2.operands.is_empty(), "rep stosb takes no explicit operands");
}

#[test]
fn bulkmem_ops_memcpy_qwords_recipe_exists() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BulkMemOps",
        "memcpy_qwords",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(result.is_some(), "BulkMemOps::memcpy_qwords should have a lowering recipe");

    let recipe = result
        .unwrap()
        .expect("memcpy_qwords lowering should succeed");

    assert_eq!(
        recipe.instructions.len(),
        2,
        "memcpy_qwords should lower to two instructions (mov rcx, rdx + rep movsq)"
    );

    // Verify arg convention: SysVRegs
    assert_eq!(
        recipe.arg_convention,
        paideia_as_elaborator::stdlib_lowering::ArgConvention::SysVRegs,
        "memcpy_qwords should use SysVRegs convention"
    );

    // First instruction: mov rcx, rdx (REP implicit counter)
    let inst0 = &recipe.instructions[0];
    assert_eq!(inst0.mnemonic, Mnemonic::Mov, "first instruction should be Mov");
    match &inst0.operands[0] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RCX, "mov dest should be RCX"),
        _ => panic!("expected Reg(RCX)"),
    }
    match &inst0.operands[1] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RDX, "mov src should be RDX (SysV arg2 count)"),
        _ => panic!("expected Reg(RDX)"),
    }

    // Terminal instruction: rep movsq (zero-arity)
    let inst1 = &recipe.instructions[1];
    assert_eq!(inst1.mnemonic, Mnemonic::RepMovsq, "terminal instruction should be RepMovsq");
    assert!(inst1.operands.is_empty(), "rep movsq takes no explicit operands");
}

#[test]
fn bulkmem_ops_memset_qwords_recipe_exists() {
    let arena = IrArena::new();
    let result = paideia_as_elaborator::stdlib_lowering::lower_stdlib_method(
        "BulkMemOps",
        "memset_qwords",
        InstrMode::Mode64,
        &[],
        &arena,
    );

    assert!(result.is_some(), "BulkMemOps::memset_qwords should have a lowering recipe");

    let recipe = result
        .unwrap()
        .expect("memset_qwords lowering should succeed");

    assert_eq!(
        recipe.instructions.len(),
        3,
        "memset_qwords should lower to three instructions (mov rax, rsi + mov rcx, rdx + rep stosq)"
    );

    // Verify arg convention: SysVRegs
    assert_eq!(
        recipe.arg_convention,
        paideia_as_elaborator::stdlib_lowering::ArgConvention::SysVRegs,
        "memset_qwords should use SysVRegs convention"
    );

    // First instruction: mov rax, rsi (STOSQ stores RAX)
    let inst0 = &recipe.instructions[0];
    assert_eq!(inst0.mnemonic, Mnemonic::Mov, "first instruction should be Mov");
    match &inst0.operands[0] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RAX, "mov dest should be RAX"),
        _ => panic!("expected Reg(RAX)"),
    }
    match &inst0.operands[1] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RSI, "mov src should be RSI (SysV arg1 fill)"),
        _ => panic!("expected Reg(RSI)"),
    }

    // Second instruction: mov rcx, rdx (REP implicit counter)
    let inst1 = &recipe.instructions[1];
    assert_eq!(inst1.mnemonic, Mnemonic::Mov, "second instruction should be Mov");
    match &inst1.operands[0] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RCX, "mov dest should be RCX"),
        _ => panic!("expected Reg(RCX)"),
    }
    match &inst1.operands[1] {
        Operand::Reg(reg) => assert_eq!(*reg, abi::RDX, "mov src should be RDX (SysV arg2 count)"),
        _ => panic!("expected Reg(RDX)"),
    }

    // Terminal instruction: rep stosq (zero-arity)
    let inst2 = &recipe.instructions[2];
    assert_eq!(inst2.mnemonic, Mnemonic::RepStosq, "terminal instruction should be RepStosq");
    assert!(inst2.operands.is_empty(), "rep stosq takes no explicit operands");
}
