//! Macro-fusion pass regression tests.
//!
//! Phase-4-m1-007: MacroFusion is now a real rewrite pass.
//! It detects fusible instruction sequences (e.g., Cmp followed by Jcc)
//! and emits a fusion-flagged EncodingHint on the Cmp instruction.

mod common;

use common::{create_instruction_node, create_test_arena};
use paideia_as_ir::instruction::{Cond, Mnemonic, Operand, RegId};
use paideia_as_ir::opt::{MacroFusionPass, OptDiagSink, OptPass};

/// Test that macro-fusion pass noop on empty arena.
#[test]
fn macro_fusion_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = MacroFusionPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger any rewrite.
    assert!(
        !changed,
        "Empty arena should produce no changes from macro-fusion pass"
    );
    assert_eq!(sink.diagnostics.len(), 0);
}

/// Test that macro-fusion pass is registered and callable.
#[test]
fn macro_fusion_pass_registered() {
    let pass = MacroFusionPass;
    assert_eq!(
        pass.name(),
        "macro-fusion",
        "MacroFusion pass should have canonical name"
    );
}

/// Test that macro-fusion correctly detects and rewrites a Cmp-Jcc pair.
#[test]
fn macro_fusion_detects_and_rewrites_cmp_jcc_pair() {
    let (mut arena, func) = create_test_arena();

    // Create a Cmp instruction node
    let cmp_operands = vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))];
    let _cmp_id = create_instruction_node(&mut arena, Mnemonic::Cmp, cmp_operands);

    // Create a Jcc instruction node right after
    let jcc_operands = vec![Operand::Imm64(100)];
    let _jcc_id = create_instruction_node(&mut arena, Mnemonic::Jcc(Cond::Eq), jcc_operands);

    let mut sink = OptDiagSink::new();
    let pass = MacroFusionPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(
        changed,
        "MacroFusionPass should detect and rewrite the pair"
    );
    assert_eq!(sink.diagnostics.len(), 1);
    assert_eq!(sink.diagnostics[0].pass, "macro-fusion");
    assert!(
        sink.diagnostics[0]
            .message
            .contains("O1504 rewrote 1 sites")
    );
}

/// Test that macro-fusion emits O1504 diagnostic with correct count.
#[test]
fn macro_fusion_emits_o1504_with_correct_count() {
    let (mut arena, func) = create_test_arena();

    // Create 4 instruction nodes: Cmp, Jcc, Cmp, Jcc
    let cmp_operands = vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))];
    let jcc_operands = vec![Operand::Imm64(100)];

    let _cmp1 = create_instruction_node(&mut arena, Mnemonic::Cmp, cmp_operands.clone());
    let _jcc1 = create_instruction_node(&mut arena, Mnemonic::Jcc(Cond::Eq), jcc_operands.clone());
    let _cmp2 = create_instruction_node(&mut arena, Mnemonic::Cmp, cmp_operands);
    let _jcc2 = create_instruction_node(&mut arena, Mnemonic::Jcc(Cond::Ne), jcc_operands);

    let mut sink = OptDiagSink::new();
    let pass = MacroFusionPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(changed);
    assert_eq!(sink.diagnostics.len(), 1);
    assert!(
        sink.diagnostics[0]
            .message
            .contains("O1504 rewrote 2 sites")
    );
}

/// Test that macro-fusion skips lone Cmp without Jcc.
#[test]
fn macro_fusion_skips_lone_cmp() {
    let (mut arena, func) = create_test_arena();

    // Create only a Cmp node (no Jcc)
    let cmp_operands = vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(1))];
    let _cmp = create_instruction_node(&mut arena, Mnemonic::Cmp, cmp_operands);

    let mut sink = OptDiagSink::new();
    let pass = MacroFusionPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(
        !changed,
        "MacroFusionPass should not rewrite a lone Cmp without following Jcc"
    );
    assert_eq!(sink.diagnostics.len(), 0);
}
