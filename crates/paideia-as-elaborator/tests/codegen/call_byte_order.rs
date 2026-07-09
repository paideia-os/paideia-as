//! Integration tests for SysV call argument byte ordering (Issue #1099).
//!
//! Verifies that the walker produces instruction IDs in the correct order:
//! MOVs (1_000_000+), CALL (1_050_000+), and RET (1_150_000+).
//! Byte-order verification (actual instruction encoding) is tested in
//! paideia-as-emitter-pe/tests/text_emitter.rs.

use paideia_as_ir::{IrArena, IrKind, CallMeta, Symbol, SymbolKind};
use paideia_as_diagnostics::{Span, FileId};
use paideia_as_elaborator::EmitWalker;

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Test: Verify walker produces correct unified IDs for 2-arg SysV call.
/// Regression test for issue #1099: unified ID scheme ensures MOVs sort before CALL.
#[test]
fn walker_produces_correct_instruction_ids_for_sysv_call() {
    let mut arena = IrArena::new();

    // Allocate 2 literal arguments
    let arg0_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0_id, 3);
    let arg1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1_id, 4);

    // Allocate function name and Var node
    let fn_var_id = arena.alloc(IrKind::Var, span());

    // Allocate App node with callee and 2 arguments
    let app_id = arena.alloc_with_children(IrKind::App, span(), [fn_var_id, arg0_id, arg1_id]);

    // Allocate Lambda with App as body
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Create and register a function symbol (SysV ABI, default)
    let sym = Symbol::new("add_two".to_string(), SymbolKind::Function, lambda_id);
    arena.symbols_mut().insert(sym);

    // Register the call site metadata
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "add_two".to_string(),
        arg_count: 2,
        is_intrinsic: false,
    });

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify instruction IDs are in the correct range:
    // - MOVs at 1_000_000 + L*100 + reg
    // - CALL at 1_050_000 + L*100
    // - RET at 1_150_000 + L*100
    let insts = walker.state().instructions().entries();
    let l = lambda_id.get();

    let ids: Vec<u32> = insts
        .keys()
        .map(|id: &paideia_as_ir::IrNodeId| id.get())
        .collect();

    // Should have at least 4 instructions: 2 MOVs + CALL + RET
    assert!(
        insts.len() >= 4,
        "Expected at least 4 instructions (2 MOVs + CALL + RET), got {}",
        insts.len()
    );

    // Verify CALL ID is in the correct range
    let call_expected_id = 1_050_000u32.saturating_add(l.saturating_mul(100));
    assert!(
        ids.contains(&call_expected_id),
        "CALL ID {} not found. Found IDs: {:?}",
        call_expected_id,
        ids
    );

    // Verify RET ID is in the correct range
    let ret_expected_id = 1_150_000u32.saturating_add(l.saturating_mul(100));
    assert!(
        ids.contains(&ret_expected_id),
        "RET ID {} not found. Found IDs: {:?}",
        ret_expected_id,
        ids
    );

    // Verify at least one MOV ID is in 1_000_000+ range
    let mov_ids: Vec<u32> = ids
        .iter()
        .filter(|id| **id >= 1_000_000 && **id < 1_050_000)
        .copied()
        .collect();
    assert!(
        !mov_ids.is_empty(),
        "Expected MOV IDs in range [1_000_000, 1_050_000), found IDs: {:?}",
        ids
    );

    // Verify ID ordering: MOV IDs < CALL ID < RET ID
    for &mov_id in &mov_ids {
        assert!(
            mov_id < call_expected_id,
            "MOV ID {} should be less than CALL ID {}",
            mov_id,
            call_expected_id
        );
        assert!(
            call_expected_id < ret_expected_id,
            "CALL ID {} should be less than RET ID {}",
            call_expected_id,
            ret_expected_id
        );
    }
}
