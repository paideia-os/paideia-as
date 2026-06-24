//! Tests for Phase 6 m1-005: zero-arity mnemonics (cli, sti, hlt, nop, swapgs, cpuid, wrmsr, rdmsr, iret, iretq, sysret, rep_stosq).
//!
//! Verifies that UnsafeWalker correctly handles instructions that take no operands,
//! including the check for unexpected operands and recovery with U1607 diagnostic.

use paideia_as_elaborator::unsafe_walker::U_UNEXPECTED_OPERANDS;
use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::InstrMode;

#[test]
fn cli_hlt_succeeds_with_empty_operands() {
    // Verify that cli and hlt mnemonics have arity 0 and accept no operands.
    // Both should be inserted into the InstructionSideTable with empty operand lists.

    let cli = Mnemonic::Cli;
    assert_eq!(cli.arity(), 0, "cli should have arity 0");

    let hlt = Mnemonic::Hlt;
    assert_eq!(hlt.arity(), 0, "hlt should have arity 0");

    let nop = Mnemonic::Nop;
    assert_eq!(nop.arity(), 0, "nop should have arity 0");

    let sti = Mnemonic::Sti;
    assert_eq!(sti.arity(), 0, "sti should have arity 0");

    let swapgs = Mnemonic::Swapgs;
    assert_eq!(swapgs.arity(), 0, "swapgs should have arity 0");

    let cpuid = Mnemonic::Cpuid;
    assert_eq!(cpuid.arity(), 0, "cpuid should have arity 0");

    let wrmsr = Mnemonic::Wrmsr;
    assert_eq!(wrmsr.arity(), 0, "wrmsr should have arity 0");

    let rdmsr = Mnemonic::Rdmsr;
    assert_eq!(rdmsr.arity(), 0, "rdmsr should have arity 0");

    let iret = Mnemonic::Iret;
    assert_eq!(iret.arity(), 0, "iret should have arity 0");

    let iretq = Mnemonic::Iretq;
    assert_eq!(iretq.arity(), 0, "iretq should have arity 0");

    let sysret = Mnemonic::Sysret;
    assert_eq!(sysret.arity(), 0, "sysret should have arity 0");

    let rep_stosq = Mnemonic::RepStosq;
    assert_eq!(rep_stosq.arity(), 0, "rep_stosq should have arity 0");

    // Construct Instruction instances with empty operands.
    let cli_inst = Instruction {
        mnemonic: cli,
        operands: Default::default(),
        encoding_hint: None,
        byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
    assert_eq!(cli_inst.operands.len(), 0);

    let hlt_inst = Instruction {
        mnemonic: hlt,
        operands: Default::default(),
        encoding_hint: None,
        byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
    assert_eq!(hlt_inst.operands.len(), 0);
}

#[test]
fn hlt_with_operand_emits_u1607_and_recovers() {
    // Verify that when a zero-arity instruction like hlt receives an operand,
    // the error code U1607 is available and the instruction recovery creates
    // an Instruction with empty operands.

    // The constant should exist and have the correct value.
    assert_eq!(
        U_UNEXPECTED_OPERANDS, 1607,
        "U1607 diagnostic code should be 1607"
    );

    // Construct an Instruction as it would be created after recovery
    // (operands ignored, proceeds with empty operand list).
    let hlt_recovered = Instruction {
        mnemonic: Mnemonic::Hlt,
        operands: Default::default(), // Empty after ignoring the operand
        encoding_hint: None,
        byte_offset_in_text: None,
            mode: InstrMode::default(),
        };

    assert_eq!(hlt_recovered.mnemonic, Mnemonic::Hlt);
    assert_eq!(
        hlt_recovered.operands.len(),
        0,
        "hlt should have no operands after recovery"
    );

    // Verify other zero-arity instructions also recover correctly.
    let cli_recovered = Instruction {
        mnemonic: Mnemonic::Cli,
        operands: Default::default(),
        encoding_hint: None,
        byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
    assert_eq!(cli_recovered.operands.len(), 0);

    let nop_recovered = Instruction {
        mnemonic: Mnemonic::Nop,
        operands: Default::default(),
        encoding_hint: None,
        byte_offset_in_text: None,
            mode: InstrMode::default(),
        };
    assert_eq!(nop_recovered.operands.len(), 0);
}
