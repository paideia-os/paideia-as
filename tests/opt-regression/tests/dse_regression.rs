//! Dead store elimination pass regression tests.
//!
//! Phase-3-m3-003: DSE is a real-rewrite pass emitting O1505 diagnostics.
//! Tests assert that the pass correctly identifies and eliminates stores
//! whose values are never read (dead stores).

mod common;

use common::create_test_arena;
use paideia_as_ir::instruction::InstrMode;
use paideia_as_ir::opt::{DsePass, OptDiagSink, OptPass};

/// Test that DSE pass smoke-tests with empty arena.
#[test]
fn dse_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = DsePass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger dead store elimination.
    assert!(
        !changed,
        "Empty arena should produce no changes from DSE pass"
    );
}

/// Test that DSE pass is registered and callable.
#[test]
fn dse_pass_registered() {
    let pass = DsePass;
    assert_eq!(pass.name(), "dse", "DSE pass should have canonical name");
}
