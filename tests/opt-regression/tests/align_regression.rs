//! Alignment pass regression tests.
//!
//! Phase-3-m3-007: Align is a would-fire pass emitting O1508 diagnostics.
//! This pass detects code regions that could benefit from alignment directives
//! (e.g., loop entry points) but does not rewrite; it emits diagnostic markers
//! for the assembler to apply alignment.

mod common;

use common::create_test_arena;
use paideia_as_ir::opt::{AlignPass, OptDiagSink, OptPass};

/// Test that align pass smoke-tests with empty arena.
#[test]
fn align_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = AlignPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger alignment detection.
    // (Would-fire passes do not mutate the IR; they only emit diagnostics.)
    assert!(
        !changed,
        "Empty arena should produce no changes from align pass"
    );
}

/// Test that align pass is registered and callable.
#[test]
fn align_pass_registered() {
    let pass = AlignPass;
    assert_eq!(
        pass.name(),
        "align",
        "Align pass should have canonical name"
    );
}
