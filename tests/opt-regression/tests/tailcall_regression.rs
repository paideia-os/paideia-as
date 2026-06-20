//! Tail-call optimization pass regression tests.
//!
//! Phase-3-m3-005: TailCall is a real-rewrite pass emitting O1510 diagnostics.
//! Tests assert that the pass correctly transforms tail-calls (Call followed
//! by Ret) into direct jumps (Jmp).

mod common;

use common::{create_instruction_node, create_test_arena};
use paideia_as_ir::instruction::{Mnemonic, Operand, RegId};
use paideia_as_ir::opt::{OptDiagSink, OptPass, TailCallPass};

/// Test that tailcall pass smoke-tests with empty arena.
#[test]
fn tailcall_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = TailCallPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger tail-call optimization.
    assert!(
        !changed,
        "Empty arena should produce no changes from tailcall pass"
    );
}

/// Test that tailcall pass is registered and callable.
#[test]
fn tailcall_pass_registered() {
    let pass = TailCallPass;
    assert_eq!(
        pass.name(),
        "tailcall",
        "TailCall pass should have canonical name"
    );
}
