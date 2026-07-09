use super::super::*;
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::{IrNodeId, MatchDispatchMeta, EnumTypeId, EnumLayout, MatchArmMeta, SmallVec};
use crate::emit_block_body::TailContext;

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Issue #1097: Verify that jump-table dispatch instructions are emitted
/// BEFORE the lambda RET, preventing dead code.
#[test]
fn visit_match_jump_table_dispatch_sorts_before_lambda_ret() {
    let mut arena = IrArena::new();

    // Allocate a Lambda with Match body
    let lambda_id = IrNodeId::new(100).unwrap(); // Use a real non-sentinel ID

    // Allocate Match node with 4-arm dispatch
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let mut match_children = vec![scrutinee_id];

    // Create 4 arms with Literal bodies (0u64, 1u64, 2u64, 3u64)
    for variant_idx in 0..4 {
        let arm_body_id = arena.alloc(IrKind::Literal, span());
        let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
        match_children.push(arm_id);

        // Register literal value
        arena.literal_values_mut().insert(arm_body_id, variant_idx as i64);

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

    // Create walker with enum layout
    let mut walker = EmitWalker::new();
    walker.state.insert_enum_layout(
        EnumTypeId(0),
        EnumLayout::new(0), // size 0 (register form)
    );

    // Call visit_match which will internally call visit_match_jump_table
    walker.visit_match(match_id, &arena, None, TailContext::Discard, lambda_id);

    // Manually emit RET at the unified scheme: 1_150_000 + L*100
    let ret_id = IrNodeId::new(1_150_000u32
        .saturating_add(lambda_id.get().saturating_mul(100)))
        .unwrap();
    let ret_inst = Instruction {
        mnemonic: Mnemonic::Ret,
        operands: SmallVec::new(),
        encoding_hint: None,
        byte_offset_in_text: None,
        mode: walker.current_mode(),
    };
    walker.emit_inst(ret_id, ret_inst);

    // Extract all instruction IDs and sort them
    let mut node_ids: Vec<_> = walker.state.instructions.entries().keys().copied().collect();
    node_ids.sort();

    // Verify that all dispatch-related IDs are less than RET ID
    let ret_id_val = ret_id.get();
    for &instr_id in &node_ids {
        if instr_id != ret_id {
            // All non-RET instructions should have ID < RET ID
            assert!(
                instr_id.get() < ret_id_val,
                "Dispatch instruction {} should be < RET {}",
                instr_id.get(),
                ret_id_val
            );
        }
    }

    // Verify RET is present and last
    assert_eq!(node_ids.last().map(|id| *id), Some(ret_id),
        "RET should be the last instruction");
}

/// Issue #1097: Verify that default label is registered even when
/// no explicit `_` arm exists in the match.
#[test]
fn match_jump_table_registers_default_label() {
    let mut arena = IrArena::new();

    let lambda_id = IrNodeId::new(100).unwrap();

    // Allocate Match node with 4 non-default arms (no explicit _ arm)
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let mut match_children = vec![scrutinee_id];

    for variant_idx in 0..4 {
        let arm_body_id = arena.alloc(IrKind::Literal, span());
        let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
        match_children.push(arm_id);

        arena.literal_values_mut().insert(arm_body_id, variant_idx as i64);

        let mut arm_meta = MatchArmMeta::default();
        arm_meta.is_default = false; // No default arm
        arm_meta.variant_index = Some(variant_idx as u32);
        arena.match_arm_meta_mut().insert(arm_id, arm_meta);
    }

    let match_id = arena.alloc_with_children(IrKind::Match, span(), match_children);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));
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

    let mut walker = EmitWalker::new();
    walker.state.insert_enum_layout(EnumTypeId(0), EnumLayout::new(0));

    // Call visit_match which will internally call visit_match_jump_table
    walker.visit_match(match_id, &arena, None, TailContext::Discard, lambda_id);

    // Verify that match_default_<match_id> label is registered
    let default_label = format!("match_default_{}", match_id.get());
    assert!(
        walker.state.labels.contains_key(&default_label),
        "Default label '{}' should be registered even with no explicit _arm",
        default_label
    );
}

/// Issue #1097: Verify that Literal and Var arm bodies emit proper MOV instructions.
#[test]
fn match_jump_table_emits_literal_arm_bodies() {
    let mut arena = IrArena::new();

    let lambda_id = IrNodeId::new(100).unwrap();

    // Allocate Match node with 4 Literal arms (arm bodies are Literal, not Action)
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let mut match_children = vec![scrutinee_id];

    let literal_values = vec![1u64, 2u64, 3u64, 4u64];

    for (variant_idx, &literal_val) in literal_values.iter().enumerate() {
        // Create Literal arm directly (not wrapped in Action)
        let arm_id = arena.alloc(IrKind::Literal, span());
        match_children.push(arm_id);

        // Register literal value
        arena.literal_values_mut().insert(arm_id, literal_val as i64);

        let mut arm_meta = MatchArmMeta::default();
        arm_meta.is_default = false;
        arm_meta.variant_index = Some(variant_idx as u32);
        arena.match_arm_meta_mut().insert(arm_id, arm_meta);
    }

    let match_id = arena.alloc_with_children(IrKind::Match, span(), match_children);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));
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

    let mut walker = EmitWalker::new();
    walker.state.insert_enum_layout(EnumTypeId(0), EnumLayout::new(0));

    // Call visit_match which will internally call visit_match_jump_table
    walker.visit_match(match_id, &arena, None, TailContext::Discard, lambda_id);

    // Count MOV rax, imm instructions emitted for arm bodies
    let mov_count = walker.state.instructions.iter()
        .filter(|(_, instr)| instr.mnemonic == Mnemonic::Mov)
        .filter(|(_, instr)| {
            matches!(instr.operands.get(0), Some(Operand::Reg(reg)) if *reg == abi::RAX) &&
            matches!(instr.operands.get(1), Some(Operand::Imm64(_)))
        })
        .count();

    // We expect at least 4 mov instructions (one per arm body)
    // (There may be more from discriminant loads, etc.)
    assert!(
        mov_count >= 4,
        "Expected at least 4 'mov rax, imm' instructions for 4 arm bodies, got {}",
        mov_count
    );
}
