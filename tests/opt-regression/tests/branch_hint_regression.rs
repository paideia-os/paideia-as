//! Branch-hint pass regression tests.
//!
//! Phase-3-m3-007: BranchHint is a would-fire pass emitting O1507 diagnostics.
//! This pass detects Jcc instructions and emits hints for branch-prediction
//! optimizations but does not rewrite the IR.

mod common;

use common::create_test_arena;
use paideia_as_ir::opt::{BranchHintPass, OptDiagSink, OptPass};

/// Test that branch-hint pass smoke-tests with empty arena.
#[test]
fn branch_hint_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = BranchHintPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger branch-hint detection.
    // (Would-fire passes do not mutate the IR; they only emit diagnostics.)
    assert!(
        !changed,
        "Empty arena should produce no changes from branch-hint pass"
    );
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
