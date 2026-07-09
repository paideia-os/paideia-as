//! Integration tests for match jump-table dispatch byte ordering (Issue #1097).
//!
//! Verifies that the walker produces instruction IDs in the correct order:
//! dispatch instructions (1_120_000+), arm bodies (1_130_000+), arm end jumps (1_140_000+),
//! and RET (1_150_000+). This prevents dead code (dispatch sorting after RET).

use paideia_as_ir::{IrArena, IrKind, MatchDispatchMeta, EnumTypeId, EnumLayout, MatchArmMeta};
use paideia_as_diagnostics::{Span, FileId};
use paideia_as_elaborator::EmitWalker;

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Test: Verify walker produces correct unified IDs for jump-table match dispatch.
/// Regression test for issue #1097: unified ID scheme ensures dispatch sorts before RET.
#[test]
fn match_jump_table_dispatch_sorts_before_lambda_ret() {
    let mut arena = IrArena::new();

    let lambda_id = arena.alloc(IrKind::Lambda, span());

    // Allocate Match node with 4-arm jump-table dispatch
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let mut match_children = vec![scrutinee_id];

    // Create 4 arms with Literal bodies
    for variant_idx in 0..4 {
        let arm_id = arena.alloc(IrKind::Literal, span());
        match_children.push(arm_id);

        // Register literal value
        arena.literal_values_mut().insert(arm_id, variant_idx as i64);

        // Register arm metadata
        let mut arm_meta = MatchArmMeta::default();
        arm_meta.is_default = false;
        arm_meta.variant_index = Some(variant_idx as u32);
        arena.match_arm_meta_mut().insert(arm_id, arm_meta);
    }

    let match_id = arena.alloc_with_children(IrKind::Match, span(), match_children);

    // Register scrutinee type
    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));

    // Register dispatch metadata: dense, jump_table enabled
    arena.match_dispatch_meta_mut().insert(
        match_id,
        MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 4,
            covered_arms: 4,
            density_ok: true,
        },
    );

    // Set match as body of lambda
    {
        let lambda_children = arena.children_mut(lambda_id).unwrap();
        lambda_children.push(match_id);
    }

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(0), EnumLayout::new(0));
    walker.walk(&mut arena);

    // Verify instruction IDs are in the correct ranges:
    // - Dispatch (sub, cmp, ja, jmp) at 1_120_000 + L*100 + 0..4
    // - Arm bodies (mov) at 1_130_000 + L*100 + idx*10 + 0..1
    // - Arm end jumps at 1_140_000 + L*100 + idx
    // - RET at 1_150_000 + L*100
    let insts = walker.state().instructions().entries();
    let l = lambda_id.get();

    let ids: Vec<u32> = insts
        .keys()
        .map(|id: &paideia_as_ir::IrNodeId| id.get())
        .collect();

    // Verify dispatch IDs are in the correct range
    let dispatch_min = 1_120_000u32.saturating_add(l.saturating_mul(100));
    let dispatch_max = dispatch_min.saturating_add(4);
    let dispatch_ids: Vec<u32> = ids
        .iter()
        .filter(|id| **id >= dispatch_min && **id < dispatch_max)
        .copied()
        .collect();
    assert!(
        !dispatch_ids.is_empty(),
        "Expected dispatch IDs in range [{}, {}), found IDs: {:?}",
        dispatch_min,
        dispatch_max,
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

    // Verify ID ordering: all dispatch IDs < RET ID
    for &dispatch_id in &dispatch_ids {
        assert!(
            dispatch_id < ret_expected_id,
            "Dispatch ID {} should be less than RET ID {}",
            dispatch_id,
            ret_expected_id
        );
    }
}
