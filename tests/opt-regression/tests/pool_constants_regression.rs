//! Constant pooling pass regression tests.
//!
//! Phase-3-m3-007: PoolConstants is a would-fire pass emitting O1509 diagnostics.
//! This pass detects repeated immediate operands that could be pooled into
//! a constant table but does not rewrite; it emits diagnostic markers for
//! downstream constant-pool creation.

mod common;

use common::create_test_arena;
use paideia_as_ir::opt::{OptDiagSink, OptPass, PoolConstantsPass};

/// Test that pool-constants pass smoke-tests with empty arena.
#[test]
fn pool_constants_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = PoolConstantsPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger constant pooling detection.
    // (Would-fire passes do not mutate the IR; they only emit diagnostics.)
    assert!(
        !changed,
        "Empty arena should produce no changes from pool-constants pass"
    );
}

/// Test that pool-constants pass is registered and callable.
#[test]
fn pool_constants_pass_registered() {
    let pass = PoolConstantsPass;
    assert_eq!(
        pass.name(),
        "pool-constants",
        "PoolConstants pass should have canonical name"
    );
}
