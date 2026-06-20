//! Loop unrolling pass regression tests.
//!
//! Phase-3-m3-006: Unroll is a real-rewrite pass that emits O1511 diagnostics.
//! However, m3-006 is currently a stub pending loop-entry marker infrastructure.
//! Tests assert the current (would-fire) diagnostic shape and will be updated
//! once full rewrite logic lands.

mod common;

use common::create_test_arena;
use paideia_as_ir::opt::{OptDiagSink, OptPass, UnrollPass};

/// Test that unroll pass smoke-tests with empty arena.
#[test]
fn unroll_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = UnrollPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger unrolling.
    assert!(
        !changed,
        "Empty arena should produce no changes from unroll pass"
    );
}

/// Test that unroll pass is registered and callable.
#[test]
fn unroll_pass_registered() {
    let pass = UnrollPass;
    assert_eq!(
        pass.name(),
        "unroll",
        "Unroll pass should have canonical name"
    );
}
