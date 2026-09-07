//! Unit tests for enum + match lowering, extracted verbatim from the
//! pre-split `emit_enum_match.rs` (issue #1409).

use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::instruction::{Cond, Instruction, Mnemonic, Operand};
use paideia_as_ir::{abi, EnumTypeId, IrArena, IrKind, MatchDispatchMeta};

use crate::emit_block_body::TailContext;
use crate::emit_walker::EmitWalker;

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// PA-r15-009b unit test: jump-table codegen emits 4-instruction sequence.
#[test]
fn visit_match_jump_table_emits_dispatch_sequence() {
    // Construct a minimal IR arena with a Match node
    let mut arena = IrArena::new();

    // Allocate a Match node with a single Scrutinee and one Arm
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_body_id = arena.alloc(IrKind::Literal, span());
    let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

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

    // Register per-arm values: value 0 at arm index 0
    arena.match_jump_table_arm_values_mut().insert(
        match_id,
        vec![(0, 0)],
    );

    // Register arm metadata: non-default arm with index 0
    let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
    arm_meta.is_default = false;
    arm_meta.variant_index = Some(0);
    arena.match_arm_meta_mut().insert(arm_id, arm_meta);

    // Register literal value for the arm body
    arena.literal_values_mut().insert(arm_body_id, 0x1234i64);

    // Construct an EmitWalker with a mock state that provides enum layout
    let mut walker = EmitWalker::new();
    walker.state.insert_enum_layout(
        EnumTypeId(0),
        paideia_as_ir::EnumLayout::new(0),
    );

    // Call visit_match_jump_table
    walker.visit_match_jump_table(match_id, &arena, &MatchDispatchMeta {
        jump_table: true,
        min_arm: 0,
        range: 4,
        covered_arms: 4,
        density_ok: true,
    });

    // Verify the instruction sequence
    // PA-r15-009c: Sort instruction IDs to avoid HashMap order randomization
    // (production text_emitter.rs also sorts for deterministic ordering)
    let mut node_ids: Vec<_> = walker.state.instructions.entries().keys().copied().collect();
    node_ids.sort();

    let instructions: Vec<&Instruction> = node_ids.iter()
        .filter_map(|node_id| walker.state.instructions.get(*node_id))
        .collect();

    // We expect at least: cmp, ja, jmp (+ possibly arm body literal load)
    // The exact count may vary based on the arm body emission, but the key sequence should be there.
    assert!(!instructions.is_empty(), "Should emit instructions");

    // Find the dispatch sequence (look for cmp followed by ja/jmp)
    let mut found_cmp = false;
    let mut found_ja = false;
    let mut found_jmp_memidx = false;

    for instr in instructions.iter() {
        if instr.mnemonic == Mnemonic::Cmp {
            found_cmp = true;
        }
        if found_cmp && !found_ja && matches!(instr.mnemonic, Mnemonic::Jcc(Cond::Above)) {
            found_ja = true;
        }
        if found_ja && instr.mnemonic == Mnemonic::Jmp {
            // Check if the operand is MemSymIndexed
            if let Some(Operand::MemSymIndexed { name, index, scale, .. }) = instr.operands.first() {
                if name.starts_with("_jt_") && *index == abi::RAX && *scale == paideia_as_ir::instruction::Scale::X8 {
                    found_jmp_memidx = true;
                }
            }
        }
    }

    assert!(found_cmp, "Should emit cmp instruction");
    assert!(found_ja, "Should emit ja (Above) conditional jump");
    assert!(found_jmp_memidx, "Should emit jmp with MemSymIndexed operand");
}

/// PA-r15-009b fallback test: non-dense match uses cmp/jne cascade.
#[test]
fn visit_match_fallback_uses_cmp_jne_cascade() {
    let mut arena = IrArena::new();

    // Allocate a Match node
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_body_id = arena.alloc(IrKind::Literal, span());
    let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    // Register scrutinee type
    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));

    // Register dispatch metadata: sparse (density_ok = false)
    arena.match_dispatch_meta_mut().insert(
        match_id,
        MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 100, // Large range, low density
            covered_arms: 1,
            density_ok: false, // Not dense enough
        },
    );

    arena.match_jump_table_arm_values_mut().insert(
        match_id,
        vec![(0, 0)],
    );

    let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
    arm_meta.is_default = false;
    arm_meta.variant_index = Some(0);
    arena.match_arm_meta_mut().insert(arm_id, arm_meta);

    arena.literal_values_mut().insert(arm_body_id, 0x5678i64);

    let mut walker = EmitWalker::new();
    walker.state.insert_enum_layout(
        EnumTypeId(0),
        paideia_as_ir::EnumLayout::new(0),
    );

    // Call visit_match (not visit_match_jump_table, since density_ok = false)
    // PA-r17-013 (#991): pass TailContext::Discard since this is a top-level test
    walker.visit_match(match_id, &arena, None, TailContext::Discard);

    let instructions: Vec<&Instruction> = walker.state.instructions.iter()
        .map(|(_, instr)| instr)
        .collect();

    // Should NOT contain MemSymIndexed or _jt_ symbols
    let has_memsymindexed = instructions.iter().any(|instr| {
        matches!(instr.mnemonic, Mnemonic::Jmp) &&
        instr.operands.first().map(|op| matches!(op, Operand::MemSymIndexed { .. })).unwrap_or(false)
    });
    assert!(!has_memsymindexed, "Sparse match should not use MemSymIndexed jmp");

    // Should use cmp/jne cascade instead
    let has_cmp_jne = instructions.iter().any(|instr| {
        matches!(instr.mnemonic, Mnemonic::Cmp)
    }) && instructions.iter().any(|instr| {
        matches!(instr.mnemonic, Mnemonic::Jcc(Cond::Ne))
    });
    assert!(has_cmp_jne, "Sparse match should use cmp/jne cascade");
}

/// #1120: Verify end_label is registered via label_to_instr (jump-table path)
#[test]
fn end_label_resolves_via_label_to_instr_jump_table() {
    let mut arena = IrArena::new();

    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_body_id = arena.alloc(IrKind::Literal, span());
    let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

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
    arena.match_jump_table_arm_values_mut().insert(match_id, vec![(0, 0)]);

    let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
    arm_meta.is_default = false;
    arm_meta.variant_index = Some(0);
    arena.match_arm_meta_mut().insert(arm_id, arm_meta);
    arena.literal_values_mut().insert(arm_body_id, 0x1234i64);

    let mut walker = EmitWalker::new();
    walker.state.insert_enum_layout(EnumTypeId(0), paideia_as_ir::EnumLayout::new(0));

    walker.visit_match_jump_table(match_id, &arena, &MatchDispatchMeta {
        jump_table: true,
        min_arm: 0,
        range: 4,
        covered_arms: 4,
        density_ok: true,
    });

    let end_label = format!("match_end_{}", match_id.get());
    // #1120: end_label must be in label_to_instr, not labels
    assert!(walker.state().label_to_instr.contains_key(&end_label),
        "jump-table end_label must use label_to_instr");
    assert!(!walker.state().labels.contains_key(&end_label),
        "jump-table end_label should not be in labels (walker-time offsets)");

    // #1120: Verify the NOP anchor's byte offset is correct via real encoding
    let mut table = walker.state().instructions().clone();
    let mut buf = Vec::new();
    let result = paideia_as_emitter_pe::emit_text_from_instructions(&mut table, &mut buf)
        .expect("encode should succeed");

    let &nop_id = walker.state().label_to_instr().get(&end_label)
        .expect("end_label must resolve via label_to_instr");
    let &nop_offset = result.offset_map.get(&nop_id)
        .expect("NOP anchor must have an offset_map entry");

    assert_eq!(
        nop_offset as usize,
        buf.len() - 1,
        "end_label NOP anchor must be the LAST byte in .text \
         (sort-order proof — this is what the walker-order bug broke). \
         Actual: nop at offset {}, buf.len()={}",
        nop_offset,
        buf.len()
    );
}

/// #1120: Verify end_label is registered via label_to_instr (cmp/jne cascade path)
#[test]
fn end_label_resolves_via_label_to_instr_cmp_jne() {
    let mut arena = IrArena::new();

    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_body_id = arena.alloc(IrKind::Literal, span());
    let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));
    // density_ok = false forces cmp/jne cascade
    arena.match_dispatch_meta_mut().insert(
        match_id,
        MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 100,
            covered_arms: 1,
            density_ok: false, // Force cmp/jne cascade
        },
    );
    arena.match_jump_table_arm_values_mut().insert(match_id, vec![(0, 0)]);

    let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
    arm_meta.is_default = false;
    arm_meta.variant_index = Some(0);
    arena.match_arm_meta_mut().insert(arm_id, arm_meta);
    arena.literal_values_mut().insert(arm_body_id, 0x5678i64);

    let mut walker = EmitWalker::new();
    walker.state.insert_enum_layout(EnumTypeId(0), paideia_as_ir::EnumLayout::new(0));

    walker.visit_match(match_id, &arena, None, TailContext::Discard);

    let end_label = format!("match_end_{}", match_id.get());
    // #1120: end_label must be in label_to_instr, not labels
    assert!(walker.state().label_to_instr.contains_key(&end_label),
        "cmp/jne end_label must use label_to_instr");
    assert!(!walker.state().labels.contains_key(&end_label),
        "cmp/jne end_label should not be in labels (walker-time offsets)");

    // #1120: Verify the NOP anchor's byte offset is correct via real encoding
    let mut table = walker.state().instructions().clone();
    let mut buf = Vec::new();
    let result = paideia_as_emitter_pe::emit_text_from_instructions(&mut table, &mut buf)
        .expect("encode should succeed");

    let &nop_id = walker.state().label_to_instr().get(&end_label)
        .expect("end_label must resolve via label_to_instr");
    let &nop_offset = result.offset_map.get(&nop_id)
        .expect("NOP anchor must have an offset_map entry");

    assert_eq!(
        nop_offset as usize,
        buf.len() - 1,
        "end_label NOP anchor must be the LAST byte in .text \
         (sort-order proof — this is what the walker-order bug broke). \
         Actual: nop at offset {}, buf.len()={}",
        nop_offset,
        buf.len()
    );
}

#[test]
fn arm_label_resolves_via_label_to_instr_cmp_jne() {
    // #1241: Verify arm_label resolves via label_to_instr NOP anchor (not walker-time offset)
    let mut arena = IrArena::new();

    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_body_id = arena.alloc(IrKind::Literal, span());
    let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));
    // density_ok = false forces cmp/jne cascade
    arena.match_dispatch_meta_mut().insert(
        match_id,
        MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 100,
            covered_arms: 1,
            density_ok: false, // Force cmp/jne cascade
        },
    );
    arena.match_jump_table_arm_values_mut().insert(match_id, vec![(0, 0)]);

    let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
    arm_meta.is_default = false;
    arm_meta.variant_index = Some(0);
    arena.match_arm_meta_mut().insert(arm_id, arm_meta);
    arena.literal_values_mut().insert(arm_body_id, 0x5678i64);

    let mut walker = EmitWalker::new();
    walker.state.insert_enum_layout(EnumTypeId(0), paideia_as_ir::EnumLayout::new(0));

    walker.visit_match(match_id, &arena, None, TailContext::Discard);

    let arm_label = format!("match_arm_{}_{}", match_id.get(), 0);
    // #1241: arm_label must be in label_to_instr, not labels
    assert!(walker.state().label_to_instr.contains_key(&arm_label),
        "cmp/jne arm_label must use label_to_instr");
    assert!(!walker.state().labels.contains_key(&arm_label),
        "cmp/jne arm_label should not be in labels (walker-time offsets)");

    // #1241: Verify the NOP anchor's byte offset via real encoding
    let mut table = walker.state().instructions().clone();
    let mut buf = Vec::new();
    let result = paideia_as_emitter_pe::emit_text_from_instructions(&mut table, &mut buf)
        .expect("encode should succeed");

    let &arm_nop_id = walker.state().label_to_instr().get(&arm_label)
        .expect("arm_label must resolve via label_to_instr");
    let &arm_nop_offset = result.offset_map.get(&arm_nop_id)
        .expect("arm NOP anchor must have an offset_map entry");

    // Verify the NOP is at the start of the arm's dispatch sequence
    assert!(arm_nop_offset < 100, "arm_label NOP should be early in .text");
}

#[test]
fn default_label_resolves_via_label_to_instr_cmp_jne_inline_default() {
    // #1241: Verify default_label (in-loop) resolves via label_to_instr NOP anchor
    let mut arena = IrArena::new();

    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_body_id = arena.alloc(IrKind::Literal, span());
    let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));
    arena.match_dispatch_meta_mut().insert(
        match_id,
        MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 100,
            covered_arms: 1,
            density_ok: false,
        },
    );
    arena.match_jump_table_arm_values_mut().insert(match_id, vec![(0, 0)]);

    // Create a default arm (is_default = true)
    let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
    arm_meta.is_default = true;
    arena.match_arm_meta_mut().insert(arm_id, arm_meta);
    arena.literal_values_mut().insert(arm_body_id, 0xABCDi64);

    let mut walker = EmitWalker::new();
    walker.state.insert_enum_layout(EnumTypeId(0), paideia_as_ir::EnumLayout::new(0));

    walker.visit_match(match_id, &arena, None, TailContext::Discard);

    let default_label = format!("match_default_{}", match_id.get());
    // #1241: default_label must be in label_to_instr, not labels
    assert!(walker.state().label_to_instr.contains_key(&default_label),
        "cmp/jne default_label must use label_to_instr");
    assert!(!walker.state().labels.contains_key(&default_label),
        "cmp/jne default_label should not be in labels (walker-time offsets)");

    // Verify encoding succeeds
    let mut table = walker.state().instructions().clone();
    let mut buf = Vec::new();
    let result = paideia_as_emitter_pe::emit_text_from_instructions(&mut table, &mut buf)
        .expect("encode should succeed");

    let &default_nop_id = walker.state().label_to_instr().get(&default_label)
        .expect("default_label must resolve via label_to_instr");
    let &default_nop_offset = result.offset_map.get(&default_nop_id)
        .expect("default NOP anchor must have an offset_map entry");

    // Default label should be present and have a valid offset
    assert!(default_nop_offset < buf.len() as u64, "default_label NOP must be within .text");
}

#[test]
fn body_label_resolves_via_label_to_instr_cmp_jne_multi_alt() {
    // #1241: Verify body_label (multi-alt or-pattern) resolves via label_to_instr NOP anchor
    let mut arena = IrArena::new();

    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_body_id = arena.alloc(IrKind::Literal, span());
    let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));
    arena.match_dispatch_meta_mut().insert(
        match_id,
        MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 100,
            covered_arms: 1,
            density_ok: false,
        },
    );
    arena.match_jump_table_arm_values_mut().insert(match_id, vec![(0, 0), (1, 0)]);

    // Create a multi-alt arm (or-pattern)
    let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
    arm_meta.is_default = false;
    arm_meta.alt_variant_indices = vec![0, 1]; // Two alternatives
    arena.match_arm_meta_mut().insert(arm_id, arm_meta);
    arena.literal_values_mut().insert(arm_body_id, 0xFEDCi64);

    let mut walker = EmitWalker::new();
    walker.state.insert_enum_layout(EnumTypeId(0), paideia_as_ir::EnumLayout::new(0));

    walker.visit_match(match_id, &arena, None, TailContext::Discard);

    let body_label = format!("match_arm_{}_{}_body", match_id.get(), 0);
    // #1241: body_label must be in label_to_instr, not labels
    assert!(walker.state().label_to_instr.contains_key(&body_label),
        "cmp/jne body_label must use label_to_instr");
    assert!(!walker.state().labels.contains_key(&body_label),
        "cmp/jne body_label should not be in labels (walker-time offsets)");

    // Verify encoding succeeds
    let mut table = walker.state().instructions().clone();
    let mut buf = Vec::new();
    let result = paideia_as_emitter_pe::emit_text_from_instructions(&mut table, &mut buf)
        .expect("encode should succeed");

    let &body_nop_id = walker.state().label_to_instr().get(&body_label)
        .expect("body_label must resolve via label_to_instr");
    let &body_nop_offset = result.offset_map.get(&body_nop_id)
        .expect("body NOP anchor must have an offset_map entry");

    // Body label should be present and have a valid offset
    assert!(body_nop_offset < buf.len() as u64, "body_label NOP must be within .text");
}
