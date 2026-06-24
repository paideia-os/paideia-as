//! Alignment pass regression tests.
//!
//! Phase-4-m1-009: Align is now a real rewrite pass.
//! This pass detects loop-entry candidates via LoopMetaTable and emits
//! alignment markers for loop entry points.

mod common;

use common::create_test_arena;
use paideia_as_ir::IrKind;
use paideia_as_ir::instruction::InstrMode;
use paideia_as_ir::loop_meta::LoopMeta;
use paideia_as_ir::opt::{AlignPass, OptDiagSink, OptPass};

/// Test that align pass noop on empty arena.
#[test]
fn align_noop_on_empty_arena() {
    let (mut arena, func) = create_test_arena();

    let mut sink = OptDiagSink::new();
    let pass = AlignPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    // Empty arena should not trigger any rewrite.
    assert!(
        !changed,
        "Empty arena should produce no changes from align pass"
    );
    assert_eq!(sink.diagnostics.len(), 0);
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

/// Test that align correctly detects and emits marker for a Loop node.
#[test]
fn align_detects_and_marks_loop_entry() {
    let (mut arena, func) = create_test_arena();

    // Create a Loop node with metadata
    let file = paideia_as_diagnostics::FileId::new(1).unwrap();
    let span = paideia_as_diagnostics::Span::new(file, 0, 5);
    let loop_id = arena.alloc(IrKind::Loop, span);

    // Add loop metadata
    let meta = LoopMeta {
        entry_label: 100,
        exit_label: 200,
    };
    arena.loop_meta_mut().insert(loop_id, meta);

    let mut sink = OptDiagSink::new();
    let pass = AlignPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(
        changed,
        "AlignPass should detect and emit marker for loop entry"
    );
    assert_eq!(sink.diagnostics.len(), 1);
    assert_eq!(sink.diagnostics[0].pass, "align");
    assert!(
        sink.diagnostics[0]
            .message
            .contains("O1508 rewrote 1 sites")
    );
}

/// Test that align emits O1508 diagnostic with correct count.
#[test]
fn align_emits_o1508_with_correct_count() {
    let (mut arena, func) = create_test_arena();

    let file = paideia_as_diagnostics::FileId::new(1).unwrap();
    let span = paideia_as_diagnostics::Span::new(file, 0, 5);

    // Create 3 Loop nodes with metadata
    let loop1_id = arena.alloc(IrKind::Loop, span);
    let meta1 = LoopMeta {
        entry_label: 100,
        exit_label: 200,
    };
    arena.loop_meta_mut().insert(loop1_id, meta1);

    let loop2_id = arena.alloc(IrKind::Loop, span);
    let meta2 = LoopMeta {
        entry_label: 110,
        exit_label: 210,
    };
    arena.loop_meta_mut().insert(loop2_id, meta2);

    let loop3_id = arena.alloc(IrKind::Loop, span);
    let meta3 = LoopMeta {
        entry_label: 120,
        exit_label: 220,
    };
    arena.loop_meta_mut().insert(loop3_id, meta3);

    let mut sink = OptDiagSink::new();
    let pass = AlignPass;

    let changed = pass.apply(&mut arena, func, &mut sink);

    assert!(changed);
    assert_eq!(sink.diagnostics.len(), 1);
    assert!(
        sink.diagnostics[0]
            .message
            .contains("O1508 rewrote 3 sites")
    );
}
