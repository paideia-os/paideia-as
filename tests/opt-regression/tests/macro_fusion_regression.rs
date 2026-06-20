//! Macro-fusion pass regression tests.
//!
//! Phase-3-m3-007: MacroFusion is a would-fire pass emitting O1504 diagnostics.
//! This pass detects fusible instruction sequences (e.g., Cmp followed by Jcc)
//! but does not rewrite; it emits diagnostic markers for the encoder to fuse
//! at code-generation time.

mod common;

use common::create_test_arena;
use paideia_as_ir::opt::{MacroFusionPass, OptDiagSink, OptPass};

/// Test that macro-fusion pass smoke-tests with empty arena.
#[test]
fn macro_fusion_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = MacroFusionPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger macro-fusion detection.
    // (Would-fire passes do not mutate the IR; they only emit diagnostics.)
    assert!(
        !changed,
        "Empty arena should produce no changes from macro-fusion pass"
    );
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
