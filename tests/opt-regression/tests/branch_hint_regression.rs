//! Branch-hint pass regression tests.
//!
//! Phase-4-m1-008: BranchHint is now a real rewrite pass.
//! It detects Jcc instructions and emits branch-hint prefix directives
//! (0x2E for not-taken / 0x3E for taken) via EncodingHint markers.

mod common;

use common::{create_instruction_node, create_test_arena};
use paideia_as_ir::instruction::{Cond, Mnemonic, Operand};
use paideia_as_ir::opt::{BranchHintPass, OptDiagSink, OptPass};

/// Test that branch-hint pass noop on empty arena.
#[test]
fn branch_hint_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = BranchHintPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger any rewrite.
    assert!(
        !changed,
        "Empty arena should produce no changes from branch-hint pass"
    );
    assert_eq!(sink.diagnostics.len(), 0);
}

/// Test that branch-hint pass is registered and callable.
#[test]
fn branch_hint_pass_registered() {
    let pass = BranchHintPass;
    assert_eq!(
        pass.name(),
        "branch-hint",
        "BranchHint pass should have canonical name"
    );
}

/// Test that branch-hint correctly detects and rewrites a Jcc instruction.
#[test]
fn branch_hint_detects_and_rewrites_jcc() {
    let (mut arena, func) = create_test_arena();

    // Create a Jcc instruction node
    let jcc_operands = vec![Operand::Imm64(100)];
    let _jcc_id = create_instruction_node(&mut arena, Mnemonic::Jcc(Cond::Eq), jcc_operands);

    let mut sink = OptDiagSink::new();
    let pass = BranchHintPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(
        changed,
        "BranchHintPass should detect and rewrite the Jcc instruction"
    );
    assert_eq!(sink.diagnostics.len(), 1);
    assert_eq!(sink.diagnostics[0].pass, "branch-hint");
    assert!(
        sink.diagnostics[0]
            .message
            .contains("O1507 rewrote 1 sites")
    );
}

/// Test that branch-hint emits O1507 diagnostic with correct count.
#[test]
fn branch_hint_emits_o1507_with_correct_count() {
    let (mut arena, func) = create_test_arena();

    // Create 3 Jcc instructions
    let jcc_operands = vec![Operand::Imm64(100)];

    let _jcc1 = create_instruction_node(&mut arena, Mnemonic::Jcc(Cond::Eq), jcc_operands.clone());
    let _jcc2 = create_instruction_node(&mut arena, Mnemonic::Jcc(Cond::Ne), jcc_operands.clone());
    let _jcc3 = create_instruction_node(&mut arena, Mnemonic::Jcc(Cond::Lt), jcc_operands);

    let mut sink = OptDiagSink::new();
    let pass = BranchHintPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(changed);
    assert_eq!(sink.diagnostics.len(), 1);
    assert!(
        sink.diagnostics[0]
            .message
            .contains("O1507 rewrote 3 sites")
    );
}
