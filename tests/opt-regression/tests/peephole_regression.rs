//! Peephole optimization pass regression tests.
//!
//! Phase-3-m3-001: Peephole is a real-rewrite pass emitting O1501/O1502
//! diagnostics. Tests assert that the pass correctly identifies and rewrites
//! canonical patterns (e.g., `mov r, r` → eliminate).

mod common;

use common::{create_instruction_node, create_test_arena};
use paideia_as_ir::instruction::{Mnemonic, Operand, RegId};
use paideia_as_ir::opt::{OptDiagSink, OptPass, PeepholePass};

/// Test that peephole pass emits "rewrote" diagnostic for nop-mov removal.
#[test]
fn peephole_rewrites_nop_mov() {
    let (mut arena, func) = create_test_arena();

    // Create instruction: `mov r0, r0` (nop-mov candidate).
    let inst_id = create_instruction_node(
        &mut arena,
        Mnemonic::Mov,
        vec![Operand::Reg(RegId(0)), Operand::Reg(RegId(0))],
    );

    let mut sink = OptDiagSink::new();
    let pass = PeepholePass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Should have detected and rewritten the nop-mov.
    assert!(changed, "Expected peephole pass to detect nop-mov");

    // Collect all diagnostic messages.
    let messages: Vec<String> = sink.diagnostics.iter().map(|d| d.message.clone()).collect();

    // Should emit at least one diagnostic containing "O1501" (rewrite code).
    assert!(
        messages.iter().any(|m| m.contains("O1501")),
        "Expected O1501 diagnostic for nop-mov rewrite; got: {:?}",
        messages
    );
}

/// Test that peephole pass rewrites zero-add pattern.
#[test]
fn peephole_rewrites_zero_add() {
    let (mut arena, func) = create_test_arena();

    // Create instruction: `add r0, 0` (zero-add candidate).
    let inst_id = create_instruction_node(
        &mut arena,
        Mnemonic::Add,
        vec![Operand::Reg(RegId(0)), Operand::Imm64(0)],
    );

    let mut sink = OptDiagSink::new();
    let pass = PeepholePass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(changed, "Expected peephole pass to detect zero-add");

    let messages: Vec<String> = sink.diagnostics.iter().map(|d| d.message.clone()).collect();

    assert!(
        messages.iter().any(|m| m.contains("O1501")),
        "Expected O1501 diagnostic for zero-add rewrite; got: {:?}",
        messages
    );
}

/// Test that peephole pass does not fire on empty arena.
#[test]
fn peephole_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = PeepholePass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(
        !changed,
        "Empty arena should produce no changes from peephole pass"
    );
    assert_eq!(
        sink.diagnostics.len(),
        0,
        "Empty arena should produce no diagnostics"
    );
}
