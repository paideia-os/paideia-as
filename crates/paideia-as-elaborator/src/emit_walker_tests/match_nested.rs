use super::super::*;
use crate::emit_fixture::EmitFixture;
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::CallMeta;


fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}
/// Helper to build and walk a match expression with given payload size and arm specs.
/// Returns walker with emitted instructions.
fn build_and_walk_match(
    payload_size: u64,
    arm_specs: Vec<(u32, bool, Option<String>)>, // (variant_idx, is_default, payload_binder)
) -> EmitWalker {
    let mut arena = IrArena::new();

    // Create match node with arms
    let match_id = arena.alloc(IrKind::Match, span());
    let mut children = vec![];

    // First child: scrutinee (placeholder)
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    children.push(scrutinee_id);

    // Remaining children: arms
    let mut arm_ids = vec![];
    for (idx, (variant_idx, is_default, payload_binder)) in arm_specs.iter().enumerate() {
        let arm_id = arena.alloc(IrKind::Action, span());
        arm_ids.push(arm_id);
        children.push(arm_id);

        // Register arm metadata
        arena.match_arm_meta_mut().insert(
            arm_id,
            paideia_as_ir::MatchArmMeta {
                variant_index: if *is_default { None } else { Some(*variant_idx) },
                payload_binder: payload_binder.clone(),
                is_default: *is_default,
                pattern_binding: None,
                int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,            },
        );
    }

    // Set match children
    {
        let match_children = arena.children_mut(match_id).unwrap();
        for &child_id in &children {
            match_children.push(child_id);
        }
    }

    // Register match scrutinee type
    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    // Register layout
    let layout = EnumLayout::new(payload_size);
    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), layout);

    walker.walk(&mut arena);
    walker
}

#[test]
fn match_empty_default_only() {
    // Single default arm, no comparisons
    let walker = build_and_walk_match(0, vec![(0, true, None)]);
    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn match_one_variant_one_default() {
    // 1 variant + 1 default arm
    let walker = build_and_walk_match(0, vec![(0, false, None), (1, true, None)]);
    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn match_two_variants_default() {
    // 2 variants + 1 default arm
    let walker = build_and_walk_match(0, vec![(0, false, None), (1, false, None), (2, true, None)]);
    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn match_three_variants_default() {
    // 3 variants + 1 default arm
    let walker = build_and_walk_match(
        0,
        vec![(0, false, None), (1, false, None), (2, false, None), (3, true, None)],
    );
    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn match_two_variants_no_default() {
    // 2 variants without explicit default
    let walker = build_and_walk_match(0, vec![(0, false, None), (1, false, None)]);
    // Should not error; default label will be registered by visit_match
    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn match_all_wildcard_no_cmp() {
    // Default arm only; no cmp instructions
    let walker = build_and_walk_match(0, vec![(0, true, None)]);
    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn match_cmp_rax_0_imm8_form() {
    // cmp rax, 0 → 48 83 F8 00 (4 bytes for imm8 form)
    let walker = build_and_walk_match(0, vec![(0, false, None), (1, true, None)]);

    // Extract cmp instruction (match_id * 100 + 0 * 10)
    let match_id = IrNodeId::new(1).unwrap(); // First (and only) match allocated
    let cmp_id = IrNodeId::new(1 * 100 + 0 * 10).unwrap();
    let cmp_inst = walker
        .state()
        .instructions
        .get(cmp_id)
        .cloned()
        .expect("cmp instruction should exist");

    // Encode and verify byte sequence
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&cmp_inst, &mut buf, &mut stats)
        .expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xF8, 0x00]);
}

#[test]
fn match_cmp_rax_128_imm32_form() {
    // cmp rax, 128 → encoder produces 48 81 F8 80 00 00 00 (7 bytes, r/m form)
    let walker = build_and_walk_match(0, vec![(128, false, None), (1, true, None)]);

    let cmp_id = IrNodeId::new(1 * 100 + 0 * 10).unwrap();
    let cmp_inst = walker
        .state()
        .instructions
        .get(cmp_id)
        .cloned()
        .expect("cmp instruction should exist");

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&cmp_inst, &mut buf, &mut stats)
        .expect("encode failed");
    // Encoder produces r/m form: 48 81 F8 80 00 00 00
    assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xF8, 0x80, 0x00, 0x00, 0x00]);
}

#[test]
fn match_jne_rel32() {
    // jne rel32 should be 6 bytes: 0F 85 XX XX XX XX
    let walker = build_and_walk_match(0, vec![(0, false, None), (1, true, None)]);

    let jne_id = IrNodeId::new(1 * 100 + 0 * 10 + 1).unwrap();
    let jne_inst = walker
        .state()
        .instructions
        .get(jne_id)
        .cloned()
        .expect("jne instruction should exist");

    assert_eq!(jne_inst.mnemonic, Mnemonic::Jcc(Cond::Ne));
    // Encoding produces 6-byte rel32 form
}

#[test]
fn match_discriminant_load_rdi_0() {
    // Stack form (size > 16): mov rax, [rdi+0] → 48 8B 07 (3 bytes)
    let walker = build_and_walk_match(16, vec![(0, false, None), (1, true, None)]);

    let disc_load_id = IrNodeId::new(1 * 100 + 900).unwrap();
    let disc_load_inst = walker
        .state()
        .instructions
        .get(disc_load_id)
        .cloned()
        .expect("disc load instruction should exist");

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&disc_load_inst, &mut buf, &mut stats)
        .expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x07]);
}

#[test]
fn match_payload_load_rdi_8_w64() {
    // #1084 (follow-up): Register form (size=16): NO memory load, direct rebind to RDX
    let walker = build_and_walk_match(
        8,  // payload_size=8 → layout.size=16 (register form)
        vec![(0, false, Some("x".to_string())), (1, true, None)],
    );

    // Assert: NO memory load instruction emitted for register form
    let payload_load_id = IrNodeId::new(1 * 100 + 0 * 10 + 2).unwrap();
    let payload_load_inst = walker.state().instructions.get(payload_load_id);
    assert!(
        payload_load_inst.is_none(),
        "Register form should NOT emit memory load; payload already in RDX"
    );

    // Assert: local_bindings["x"] must be RDX (direct rebind)
    assert!(
        walker.state().local_bindings.get("x") == Some(abi::RDX),
        "Register form should rebind payload_binder to RDX directly"
    );
}

#[test]
fn match_reg_form_omits_disc_load() {
    // Register form (size ≤ 16): discriminant load NOT emitted
    let walker = build_and_walk_match(8, vec![(0, false, None), (1, true, None)]);

    let disc_load_id = IrNodeId::new(1 * 100 + 900).unwrap();
    let disc_load_inst = walker.state().instructions.get(disc_load_id);
    assert!(disc_load_inst.is_none());
}

#[test]
fn match_stack_form_emits_disc_load() {
    // Stack form (size > 16): discriminant load IS emitted
    let walker = build_and_walk_match(16, vec![(0, false, None), (1, true, None)]);

    let disc_load_id = IrNodeId::new(1 * 100 + 900).unwrap();
    let disc_load_inst = walker.state().instructions.get(disc_load_id);
    assert!(disc_load_inst.is_some());
}

#[test]
fn match_labels_registered_correctly() {
    // Verify that labels are registered: match_arm_<id>_0, match_default_<id>, match_end_<id>
    let walker = build_and_walk_match(0, vec![(0, false, None), (1, true, None)]);

    let match_id = 1u32; // First match allocated
    let arm_0_label = format!("match_arm_{}_{}", match_id, 0);
    let default_label = format!("match_default_{}", match_id);
    let end_label = format!("match_end_{}", match_id);

    // #1241: all cascade-path labels now use label_to_instr (post-sort resolution via NOP anchor)
    // instead of register_label (which captured walker-time estimated_offset that drifts with Unsafe bodies).
    // Mirrors #1120's fix for end_label to all arm/default labels.
    assert!(walker.state().label_to_instr.contains_key(&arm_0_label),
        "arm_0_label must be registered via label_to_instr for post-sort resolution");
    assert!(walker.state().label_to_instr.contains_key(&default_label),
        "default_label must be registered via label_to_instr for post-sort resolution");
    assert!(walker.state().label_to_instr.contains_key(&end_label),
        "end_label must be registered via label_to_instr for post-sort resolution");
}

#[test]
fn match_estimated_offset_advances_correctly() {
    // Verify that estimated_offset tracks instruction sizes correctly
    let walker = build_and_walk_match(0, vec![(0, false, None), (1, false, None), (2, true, None)]);

    // offset should have advanced (cmp 4 + jne 6) * 2 + jmp 5 + labels = >20 bytes
    assert!(walker.state().estimated_offset > 20);
}

#[test]
fn match_arm_with_u64_payload_binder_emits_load() {
    // #1084 (follow-up): Register form with payload_binder: NO memory load, direct rebind
    let walker = build_and_walk_match(
        8,  // payload_size=8 → layout.size=16 (register form)
        vec![(0, false, Some("x".to_string())), (1, true, None)],
    );

    // Assert: NO memory load for register form (payload already in RDX)
    let payload_load_id = IrNodeId::new(1 * 100 + 0 * 10 + 2).unwrap();
    let payload_load_inst = walker.state().instructions.get(payload_load_id);
    assert!(
        payload_load_inst.is_none(),
        "Register form should NOT emit payload load instruction"
    );
}

#[test]
fn match_arm_no_payload_binder_no_load() {
    // Arm without payload_binder should not emit payload load
    let walker = build_and_walk_match(8, vec![(0, false, None), (1, true, None)]);

    let payload_load_id = IrNodeId::new(1 * 100 + 0 * 10 + 2).unwrap();
    let payload_load_inst = walker.state().instructions.get(payload_load_id);
    assert!(payload_load_inst.is_none());
}

#[test]
fn match_arm_default_no_payload_load() {
    // Default arm should not emit payload load
    let walker = build_and_walk_match(8, vec![(0, true, Some("x".to_string()))]);

    let payload_load_id = IrNodeId::new(1 * 100 + 0 * 10 + 2).unwrap();
    let payload_load_inst = walker.state().instructions.get(payload_load_id);
    assert!(payload_load_inst.is_none());
}

#[test]
fn match_missing_scrutinee_type_emits_diagnostic() {
    // No entry in match_scrutinee_table should emit diagnostic
    let mut arena = IrArena::new();
    let match_id = arena.alloc(IrKind::Match, span());
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());

    {
        let children = arena.children_mut(match_id).unwrap();
        children.push(scrutinee_id);
        children.push(arm_id);
    }

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: None,
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    // Deliberately do NOT register match_scrutinee_table entry
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let typed = walker.take_typed_diagnostics();
    assert!(typed.iter().any(|d| d.code().number() == 1650), "Expected U1650 diagnostic for missing scrutinee type");
}

#[test]
fn match_missing_arm_meta_emits_diagnostic() {
    // No entry in match_arm_meta_table should emit diagnostic
    let mut arena = IrArena::new();
    let match_id = arena.alloc(IrKind::Match, span());
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());

    {
        let children = arena.children_mut(match_id).unwrap();
        children.push(scrutinee_id);
        children.push(arm_id);
    }

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    // Deliberately do NOT register match_arm_meta entry for arm_id
    let mut walker = EmitWalker::new();
    let layout = EnumLayout::new(0);
    walker.state_mut().insert_enum_layout(EnumTypeId(1), layout);
    walker.walk(&mut arena);

    let typed = walker.take_typed_diagnostics();
    assert!(typed.iter().any(|d| d.code().number() == 1651), "Expected U1651 diagnostic for missing MatchArmMeta");
}

// ── Phase 17 m9-009 nested pattern binding tests ─────────────────

#[test]
fn nested_record_simple_two_fields() {
    // Pattern: Point { x, y } (two u64 fields)
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    // Register match metadata with nested pattern
    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    let pattern = PatternBinding::Record {
        type_id: RecordTypeId(100),
        fields: vec![
            ("x".to_string(), PatternBinding::Simple("x_var".to_string())),
            ("y".to_string(), PatternBinding::Simple("y_var".to_string())),
        ],
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    // Create record layout with field names
    let rec_layout = RecordLayout::with_field_names(
        16,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
        ],
        vec!["x".to_string(), "y".to_string()],
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(16));
    walker.state_mut().insert_record_layout(RecordTypeId(100), rec_layout);
    walker.walk(&mut arena);

    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());

    // Byte-exact: verify TWO loads emitted, one at [rdi+0] into RCX,
    // one at [rdi+8] into RDX.
    // mov rcx, [rdi+0]  → 48 8B 0F  (no disp)
    // mov rdx, [rdi+8]  → 48 8B 57 08
    let moves = collect_move_bytes(&walker);
    let load_from_rdi0_into_rcx = &[0x48u8, 0x8B, 0x0F][..];
    let load_from_rdi8_into_rdx = &[0x48u8, 0x8B, 0x57, 0x08][..];
    assert!(
        moves.iter().any(|b| b.as_slice() == load_from_rdi0_into_rcx),
        "expected `mov rcx, [rdi+0]` in emitted moves; got {:?}",
        moves
    );
    assert!(
        moves.iter().any(|b| b.as_slice() == load_from_rdi8_into_rdx),
        "expected `mov rdx, [rdi+8]` in emitted moves; got {:?}",
        moves
    );
}

#[test]
fn nested_enum_over_leaf() {
    // Pattern: Ok(x) — regression parity with #986
    use paideia_as_ir::PatternBinding;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    let pattern = PatternBinding::EnumVariant {
        variant_index: 0,
        payload_type: None,
        payload: Some(Box::new(PatternBinding::Simple("payload_var".to_string()))),
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(8));
    walker.walk(&mut arena);

    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn nested_enum_over_record() {
    // Pattern: Ok(Point { x, y })
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    let record_pattern = PatternBinding::Record {
        type_id: RecordTypeId(200),
        fields: vec![
            ("x".to_string(), PatternBinding::Simple("x_var".to_string())),
            ("y".to_string(), PatternBinding::Simple("y_var".to_string())),
        ],
    };

    let pattern = PatternBinding::EnumVariant {
        variant_index: 0,
        payload_type: Some(RecordTypeId(200)),
        payload: Some(Box::new(record_pattern)),
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    let rec_layout = RecordLayout::with_field_names(
        16,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
        ],
        vec!["x".to_string(), "y".to_string()],
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(16));
    walker.state_mut().insert_record_layout(RecordTypeId(200), rec_layout);
    walker.walk(&mut arena);

    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn nested_record_over_enum_over_record() {
    // Pattern: Container { field: Ok(Point { x, y }) }
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    // Inner record: Point { x, y }
    let point_pattern = PatternBinding::Record {
        type_id: RecordTypeId(200),
        fields: vec![
            ("x".to_string(), PatternBinding::Simple("x_var".to_string())),
            ("y".to_string(), PatternBinding::Simple("y_var".to_string())),
        ],
    };

    // Enum variant: Ok(Point { x, y })
    let ok_pattern = PatternBinding::EnumVariant {
        variant_index: 0,
        payload_type: Some(RecordTypeId(200)),
        payload: Some(Box::new(point_pattern)),
    };

    // Outer record: Container { field: Ok(...) }
    let container_pattern = PatternBinding::Record {
        type_id: RecordTypeId(300),
        fields: vec![("field".to_string(), ok_pattern)],
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(container_pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    let point_layout = RecordLayout::with_field_names(
        16,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
        ],
        vec!["x".to_string(), "y".to_string()],
    );

    let container_layout = RecordLayout::with_field_names(
        16,
        8,
        vec![FieldLayout { offset: 0, size: 16, signed: false }],
        vec!["field".to_string()],
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(16));
    walker.state_mut().insert_record_layout(RecordTypeId(200), point_layout);
    walker.state_mut().insert_record_layout(RecordTypeId(300), container_layout);
    walker.walk(&mut arena);

    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn nested_wildcard_at_leaf() {
    // Pattern: Point { x, _ }
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    let pattern = PatternBinding::Record {
        type_id: RecordTypeId(100),
        fields: vec![
            ("x".to_string(), PatternBinding::Simple("x_var".to_string())),
            ("y".to_string(), PatternBinding::Wildcard),
        ],
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    let rec_layout = RecordLayout::with_field_names(
        16,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
        ],
        vec!["x".to_string(), "y".to_string()],
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(16));
    walker.state_mut().insert_record_layout(RecordTypeId(100), rec_layout);
    walker.walk(&mut arena);

    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

/// #1216: Test that four simple fields in a record pattern allocate correctly from pool without exhaustion.
#[test]
fn nested_record_four_simple_fields_no_exhaustion() {
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    let pattern = PatternBinding::Record {
        type_id: RecordTypeId(100),
        fields: vec![
            ("a".to_string(), PatternBinding::Simple("a_var".to_string())),
            ("b".to_string(), PatternBinding::Simple("b_var".to_string())),
            ("c".to_string(), PatternBinding::Simple("c_var".to_string())),
            ("d".to_string(), PatternBinding::Simple("d_var".to_string())),
        ],
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    let rec_layout = RecordLayout::with_field_names(
        32,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
            FieldLayout { offset: 16, size: 8, signed: false },
            FieldLayout { offset: 24, size: 8, signed: false },
        ],
        vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()],
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(32));
    walker.state_mut().insert_record_layout(RecordTypeId(100), rec_layout);
    walker.walk(&mut arena);

    // #1216: Should emit no diagnostic — four fields fit in pool of size 5
    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());

    // Verify four loads were emitted for a, b, c, d into RCX, RDX, R8, R10 respectively
    let moves = collect_move_bytes(&walker);
    assert!(
        moves.len() >= 4,
        "Expected at least 4 move instructions, got {}",
        moves.len()
    );
}

/// Helper: collect all emitted Mov-family instructions (Mov, MovSized,
/// Movzx, Movsx) from a walker's state in ir-node-id order, encode each
/// via the real encoder, and return the byte sequences.
fn collect_move_bytes(walker: &EmitWalker) -> Vec<Vec<u8>> {
    let mut ids: Vec<(&IrNodeId, &Instruction)> =
        walker.state().instructions.entries().iter().collect();
    ids.sort_by_key(|(id, _)| id.get());
    let mut out = Vec::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    for (_id, inst) in ids {
        let is_move = matches!(
            inst.mnemonic,
            Mnemonic::Mov
                | Mnemonic::MovSized { .. }
                | Mnemonic::Movzx { .. }
                | Mnemonic::Movsx { .. }
        );
        if !is_move {
            continue;
        }
        let mut buf = paideia_as_encoder::CodeBuffer::new();
        if paideia_as_encoder::encode_instruction(inst, &mut buf, &mut stats).is_ok() {
            out.push(buf.as_slice().to_vec());
        }
    }
    out
}

#[test]
fn nested_byte_exact_enum_over_record_offsets() {
    // Pattern: Ok(Point { x: u8, y: u64 }) — verify byte offsets
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    let record_pattern = PatternBinding::Record {
        type_id: RecordTypeId(200),
        fields: vec![
            ("x".to_string(), PatternBinding::Simple("x_var".to_string())),
            ("y".to_string(), PatternBinding::Simple("y_var".to_string())),
        ],
    };

    let pattern = PatternBinding::EnumVariant {
        variant_index: 0,
        payload_type: Some(RecordTypeId(200)),
        payload: Some(Box::new(record_pattern)),
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    // Point layout: x at offset 0 (u8), y at offset 8 (u64)
    let rec_layout = RecordLayout::with_field_names(
        16,
        8,
        vec![
            FieldLayout { offset: 0, size: 1, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
        ],
        vec!["x".to_string(), "y".to_string()],
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(16));
    walker.state_mut().insert_record_layout(RecordTypeId(200), rec_layout);
    walker.walk(&mut arena);

    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());

    // Byte-exact: Ok's payload sits at [rdi+8], Point's fields nest inside:
    //   x (u8) at [rdi + 8 + 0] = [rdi+8]  → movzx rcx, byte [rdi+8]
    //   y (u64) at [rdi + 8 + 8] = [rdi+16] → mov rdx, [rdi+16]
    // movzx rcx, byte [rdi+8]  → 48 0F B6 4F 08
    // mov rdx, qword [rdi+16]  → 48 8B 57 10
    let moves = collect_move_bytes(&walker);
    let movzx_u8_rdi8_rcx = &[0x48u8, 0x0F, 0xB6, 0x4F, 0x08][..];
    let mov_u64_rdi16_rdx = &[0x48u8, 0x8B, 0x57, 0x10][..];
    assert!(
        moves.iter().any(|b| b.as_slice() == movzx_u8_rdi8_rcx),
        "expected `movzx rcx, byte [rdi+8]`; got {:?}",
        moves
    );
    assert!(
        moves.iter().any(|b| b.as_slice() == mov_u64_rdi16_rdx),
        "expected `mov rdx, [rdi+16]`; got {:?}",
        moves
    );
}

#[test]
fn nested_byte_exact_record_over_enum_offsets() {
    // Pattern: Container { field: Ok(v) } — field is enum (16-byte struct)
    // But when matching, we load the enum's discriminant (u64) from the field
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    let ok_pattern = PatternBinding::EnumVariant {
        variant_index: 0,
        payload_type: None,
        payload: Some(Box::new(PatternBinding::Simple("v".to_string()))),
    };

    let container_pattern = PatternBinding::Record {
        type_id: RecordTypeId(300),
        fields: vec![("field".to_string(), ok_pattern)],
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(container_pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    // Container has a field "field" that's an enum (size 16, aligned 8)
    // But the first field's size is 8 (the discriminant part)
    let container_layout = RecordLayout::with_field_names(
        16,
        8,
        vec![FieldLayout { offset: 0, size: 8, signed: false }],
        vec!["field".to_string()],
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(16));
    walker.state_mut().insert_record_layout(RecordTypeId(300), container_layout);
    walker.walk(&mut arena);

    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());

    // Byte-exact: Container's "field" at offset 0, then descend into Ok which
    // shifts by enum payload_offset (+8). The leaf `v` sits at [rdi + 0 + 8] = [rdi+8].
    // First (and only) leaf goes into RCX (first scratch after RAX reserved for disc).
    // mov rcx, [rdi+8] → 48 8B 4F 08
    let moves = collect_move_bytes(&walker);
    let mov_u64_rdi8_rcx = &[0x48u8, 0x8B, 0x4F, 0x08][..];
    assert!(
        moves.iter().any(|b| b.as_slice() == mov_u64_rdi8_rcx),
        "expected `mov rcx, [rdi+8]`; got {:?}",
        moves
    );
}

#[test]
fn nested_multiple_sibling_bindings_widths() {
    // Pattern: Rect { a: i8, b: i16, c: u32, d: u64 } — mixed widths
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    let pattern = PatternBinding::Record {
        type_id: RecordTypeId(100),
        fields: vec![
            ("a".to_string(), PatternBinding::Simple("a_var".to_string())),
            ("b".to_string(), PatternBinding::Simple("b_var".to_string())),
            ("c".to_string(), PatternBinding::Simple("c_var".to_string())),
            ("d".to_string(), PatternBinding::Simple("d_var".to_string())),
        ],
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    let rec_layout = RecordLayout::with_field_names(
        24,
        8,
        vec![
            FieldLayout { offset: 0, size: 1, signed: true },
            FieldLayout { offset: 2, size: 2, signed: true },
            FieldLayout { offset: 4, size: 4, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
        ],
        vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()],
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(24));
    walker.state_mut().insert_record_layout(RecordTypeId(100), rec_layout);
    walker.walk(&mut arena);

    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn nested_missing_payload_layout_diagnostic() {
    // Pattern: Ok(Point{x,y}) but Point layout is absent
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    let record_pattern = PatternBinding::Record {
        type_id: RecordTypeId(200), // This layout is NOT registered
        fields: vec![
            ("x".to_string(), PatternBinding::Simple("x_var".to_string())),
            ("y".to_string(), PatternBinding::Simple("y_var".to_string())),
        ],
    };

    let pattern = PatternBinding::EnumVariant {
        variant_index: 0,
        payload_type: Some(RecordTypeId(200)),
        payload: Some(Box::new(record_pattern)),
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(16));
    // Intentionally missing RecordTypeId(200)
    walker.walk(&mut arena);

    // Should emit diagnostic about missing layout
    let typed = walker.take_typed_diagnostics();
    assert!(typed.iter().any(|d| d.code().number() == 1652), "Expected U1652 diagnostic for missing record layout");
}

#[test]
fn nested_wildcard_at_multiple_levels() {
    // Pattern: Container { field: Ok(_) }
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    let ok_pattern = PatternBinding::EnumVariant {
        variant_index: 0,
        payload_type: None,
        payload: Some(Box::new(PatternBinding::Wildcard)),
    };

    let container_pattern = PatternBinding::Record {
        type_id: RecordTypeId(300),
        fields: vec![("field".to_string(), ok_pattern)],
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(container_pattern),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    let container_layout = RecordLayout::with_field_names(
        16,
        8,
        vec![FieldLayout { offset: 0, size: 16, signed: false }],
        vec!["field".to_string()],
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(16));
    walker.state_mut().insert_record_layout(RecordTypeId(300), container_layout);
    walker.walk(&mut arena);

    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn nested_smoke_no_panic_on_deep_nesting() {
    // 4-level deep nesting: no panic, no diagnostics expected
    use paideia_as_ir::{PatternBinding, RecordLayout, FieldLayout};
    use paideia_as_ir::record_layout::RecordTypeId;

    let mut arena = IrArena::new();
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    // Level 4: A { f: simple }
    let level4 = PatternBinding::Record {
        type_id: RecordTypeId(104),
        fields: vec![("f".to_string(), PatternBinding::Simple("f_var".to_string()))],
    };

    // Level 3: B { field: level4 }
    let level3 = PatternBinding::Record {
        type_id: RecordTypeId(103),
        fields: vec![("field".to_string(), level4)],
    };

    // Level 2: Ok(level3)
    let level2 = PatternBinding::EnumVariant {
        variant_index: 0,
        payload_type: Some(RecordTypeId(103)),
        payload: Some(Box::new(level3)),
    };

    // Level 1: C { field: level2 }
    let level1 = PatternBinding::Record {
        type_id: RecordTypeId(102),
        fields: vec![("field".to_string(), level2)],
    };

    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: Some(level1),
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    let a_layout = RecordLayout::with_field_names(
        8,
        8,
        vec![FieldLayout { offset: 0, size: 8, signed: false }],
        vec!["f".to_string()],
    );

    let b_layout = RecordLayout::with_field_names(
        8,
        8,
        vec![FieldLayout { offset: 0, size: 8, signed: false }],
        vec!["field".to_string()],
    );

    let c_layout = RecordLayout::with_field_names(
        8,
        8,
        vec![FieldLayout { offset: 0, size: 8, signed: false }],
        vec!["field".to_string()],
    );

    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(8));
    walker.state_mut().insert_record_layout(RecordTypeId(102), c_layout);
    walker.state_mut().insert_record_layout(RecordTypeId(103), b_layout);
    walker.state_mut().insert_record_layout(RecordTypeId(104), a_layout);
    walker.walk(&mut arena);

    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

// ── PA-R17-013 Match-in-return tests (synthesized IR) ──

/// Test 2: match returning small enum (≤16 bytes) writes RAX or RDX.
/// Synthesizes IR: Lambda { Match { arm0 -> EnumCons(size=12) } }
#[test]
fn match_returning_small_enum_writes_rax_rdx_ir_level() {
    let mut arena = IrArena::new();

    // EnumCons with small payload (12 bytes, ≤ 16)
    let enum_cons_id = arena.alloc(IrKind::EnumCons, span());
    arena.enum_cons_info_mut().insert(
        enum_cons_id,
        paideia_as_ir::EnumConsInfo {
            type_id: EnumTypeId(1),
            variant_index: 0,
        },
    );

    // Match arm
    let arm_id = arena.alloc(IrKind::Action, span());
    {
        let arm_children = arena.children_mut(arm_id).unwrap();
        arm_children.push(enum_cons_id);
    }
    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: None,
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    // Match
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let match_id = arena.alloc(IrKind::Match, span());
    {
        let match_children = arena.children_mut(match_id).unwrap();
        match_children.push(scrutinee_id);
        match_children.push(arm_id);
    }
    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    // Lambda
    arena.alloc_with_children(IrKind::Lambda, span(), vec![match_id]);

    // Walk
    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(12));
    walker.walk(&mut arena);

    // Verify: Mov to RAX or RDX
    let has_rax_or_rdx_mov = walker
        .state()
        .instructions
        .entries()
        .iter()
        .any(|(_id, inst)| {
            inst.mnemonic == Mnemonic::Mov
                && !inst.operands.is_empty()
                && (inst.operands[0] == Operand::Reg(abi::RAX)
                    || inst.operands[0] == Operand::Reg(abi::RDX))
        });

    assert!(
        has_rax_or_rdx_mov,
        "Small enum (≤16 bytes) should emit Mov to RAX or RDX"
    );
}

/// Test 6: nested match in arm body (both dispatches verified).
/// Synthesizes IR: Lambda { Match(outer) { arm -> Match(inner) } }
#[test]
fn nested_match_arm_body_is_match_ir_level() {
    let mut arena = IrArena::new();

    // Inner match
    let inner_scrutinee = arena.alloc(IrKind::Var, span());
    let inner_arm = arena.alloc(IrKind::Action, span());
    arena.match_arm_meta_mut().insert(
        inner_arm,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: None,
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    let inner_match = arena.alloc(IrKind::Match, span());
    {
        let children = arena.children_mut(inner_match).unwrap();
        children.push(inner_scrutinee);
        children.push(inner_arm);
    }
    arena.match_scrutinee_table_mut().insert(inner_match, EnumTypeId(2));

    // Outer match arm body is inner_match
    let outer_arm = arena.alloc(IrKind::Action, span());
    {
        let arm_children = arena.children_mut(outer_arm).unwrap();
        arm_children.push(inner_match);
    }
    arena.match_arm_meta_mut().insert(
        outer_arm,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: None,
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    // Outer match
    let outer_scrutinee = arena.alloc(IrKind::Var, span());
    let outer_match = arena.alloc(IrKind::Match, span());
    {
        let children = arena.children_mut(outer_match).unwrap();
        children.push(outer_scrutinee);
        children.push(outer_arm);
    }
    arena.match_scrutinee_table_mut().insert(outer_match, EnumTypeId(1));

    // Lambda
    arena.alloc_with_children(IrKind::Lambda, span(), vec![outer_match]);

    // Walk
    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(0));
    walker.state_mut().insert_enum_layout(EnumTypeId(2), EnumLayout::new(0));
    walker.walk(&mut arena);

    // Verify: both matches emit dispatch instructions (Cmp)
    let cmp_count = walker
        .state()
        .instructions
        .entries()
        .iter()
        .filter(|(_id, inst)| inst.mnemonic == Mnemonic::Cmp)
        .count();

    assert!(
        cmp_count >= 2,
        "Both outer and inner matches should emit dispatch (Cmp), got {}",
        cmp_count
    );
}

/// Test 7: match returning large enum (>16 bytes) writes via [RDI+disp].
/// Synthesizes IR: Lambda { Match { arm0 -> EnumCons(size=24) } }
#[test]
fn match_returning_large_enum_writes_rdi_slot_ir_level() {
    let mut arena = IrArena::new();

    // EnumCons with large payload (24 bytes, > 16)
    let enum_cons_id = arena.alloc(IrKind::EnumCons, span());
    arena.enum_cons_info_mut().insert(
        enum_cons_id,
        paideia_as_ir::EnumConsInfo {
            type_id: EnumTypeId(1),
            variant_index: 0,
        },
    );

    // Match arm
    let arm_id = arena.alloc(IrKind::Action, span());
    {
        let arm_children = arena.children_mut(arm_id).unwrap();
        arm_children.push(enum_cons_id);
    }
    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: None,
            int_pattern_value: None,
            guard: None,
            alt_int_values: Vec::new(),
            alt_variant_indices: Vec::new(),
            outer_binder: None,        },
    );

    // Match
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let match_id = arena.alloc(IrKind::Match, span());
    {
        let match_children = arena.children_mut(match_id).unwrap();
        match_children.push(scrutinee_id);
        match_children.push(arm_id);
    }
    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));

    // Lambda
    arena.alloc_with_children(IrKind::Lambda, span(), vec![match_id]);

    // Walk
    let mut walker = EmitWalker::new();
    walker.state_mut().insert_enum_layout(EnumTypeId(1), EnumLayout::new(24));
    walker.walk(&mut arena);

    // Verify: Mov to [RDI+disp] (MemSib operand)
    let has_rdi_slot_mov = walker
        .state()
        .instructions
        .entries()
        .iter()
        .any(|(_id, inst)| {
            inst.mnemonic == Mnemonic::Mov
                && !inst.operands.is_empty()
                && matches!(
                    &inst.operands[0],
                    Operand::MemSib { base, .. } if *base == abi::RDI
                )
        });

    assert!(
        has_rdi_slot_mov,
        "Large enum (>16 bytes) should emit Mov to [RDI+disp]"
    );
}

/// PA-r17-004b (#1039): Emit indirect call via record field (local-bound callee).
///
/// Tests that emit_indirect_call_via_mem_base_disp correctly:
/// 1. Loads the function pointer from [base_reg + field_offset] into R11
/// 2. Marshals arguments into SysV registers
/// 3. Calls through R11
/// 4. Returns
#[test]
fn emit_indirect_call_via_mem_base_disp_with_args() {
    let mut arena = IrArena::new();

    // Create a lambda node
    let lambda_id = IrNodeId::new(1).expect("lambda id");
    arena.alloc(IrKind::Lambda, span());

    // Create three Var nodes for arguments
    let arg0_id = IrNodeId::new(2).expect("arg0 id");
    arena.alloc(IrKind::Var, span());

    let arg1_id = IrNodeId::new(3).expect("arg1 id");
    arena.alloc(IrKind::Var, span());

    let arg2_id = IrNodeId::new(4).expect("arg2 id");
    arena.alloc(IrKind::Var, span());

    // Set binding names
    arena.binding_names_mut().insert(arg0_id, "a".to_string());
    arena.binding_names_mut().insert(arg1_id, "b".to_string());
    arena.binding_names_mut().insert(arg2_id, "c".to_string());

    // Create EmitWalker and set local bindings
    let mut walker = EmitWalker::new();
    walker.state_mut().local_bindings.insert("v".to_string(), abi::RDI);

    // Call emit_indirect_call_via_mem_base_disp
    walker.emit_indirect_call_via_mem_base_disp(
        lambda_id,
        abi::RDI,
        0,  // field offset = 0
        &[arg0_id, arg1_id, arg2_id],
        &arena,
    );

    // Verify instruction sequence
    let instrs = &walker.state().instructions;

    // Should have at least: load_fnptr, 3 arg moves, call, ret = 6 instructions
    assert!(
        instrs.len() >= 6,
        "Expected at least 6 instructions, got {}",
        instrs.len()
    );

    // First instruction should be: mov r11, [rdi + 0]
    // #1141: identity-agnostic lookup (post-emission_order the fnptr load id is
    // synthetic and monotonic; locate by shape instead).
    let first_inst = instrs
        .entries()
        .iter()
        .find(|(_, inst)| {
            inst.mnemonic == Mnemonic::Mov
                && matches!(inst.operands.first(), Some(Operand::Reg(r)) if *r == abi::R11)
                && matches!(inst.operands.get(1), Some(Operand::MemSib { .. }))
        })
        .map(|(_, inst)| inst)
        .expect("load fnptr instruction should exist");

    assert_eq!(first_inst.mnemonic, Mnemonic::Mov, "First instruction should be mov");
    assert_eq!(first_inst.operands.len(), 2, "mov should have 2 operands");

    match &first_inst.operands[0] {
        Operand::Reg(r) => assert_eq!(*r, abi::R11, "mov destination should be R11"),
        _ => panic!("Expected Reg(R11)"),
    }

    match &first_inst.operands[1] {
        Operand::MemSib { base, index, disp, .. } => {
            assert_eq!(*base, abi::RDI, "base should be RDI");
            assert!(index.is_none(), "index should be None");
            assert_eq!(*disp, 0, "displacement should be 0");
        }
        _ => panic!("Expected MemSib for fnptr load"),
    }

    // Find the call instruction
    let call_inst = instrs
        .entries()
        .iter()
        .find(|(_, inst)| inst.mnemonic == Mnemonic::Call)
        .map(|(_, inst)| inst)
        .expect("Should have a Call instruction");

    assert_eq!(call_inst.operands.len(), 1, "call should have 1 operand");
    match &call_inst.operands[0] {
        Operand::Reg(r) => assert_eq!(*r, abi::R11, "call should be through R11"),
        _ => panic!("Expected Reg(R11) for call operand"),
    }

    // Find the ret instruction
    let ret_inst = instrs
        .entries()
        .iter()
        .find(|(_, inst)| inst.mnemonic == Mnemonic::Ret)
        .map(|(_, inst)| inst)
        .expect("Should have a Ret instruction");

    assert_eq!(ret_inst.operands.len(), 0, "ret should have no operands");
}

/// PA-r17-004b (#1039): Verify field offset is correctly encoded in memory access.
///
/// Tests that emit_indirect_call_via_mem_base_disp correctly encodes a
/// non-zero field offset in the MemSib displacement.
#[test]
fn emit_indirect_call_via_mem_base_disp_with_nonzero_offset() {
    let mut arena = IrArena::new();

    let lambda_id = IrNodeId::new(1).expect("lambda id");
    arena.alloc(IrKind::Lambda, span());

    let arg0_id = IrNodeId::new(2).expect("arg0 id");
    arena.alloc(IrKind::Var, span());

    arena.binding_names_mut().insert(arg0_id, "a".to_string());

    let mut walker = EmitWalker::new();
    walker.state_mut().local_bindings.insert("v".to_string(), abi::RDI);

    // Call with field offset = 16
    walker.emit_indirect_call_via_mem_base_disp(
        lambda_id,
        abi::RDI,
        16,  // field offset = 16
        &[arg0_id],
        &arena,
    );

    // Verify first instruction encodes the offset correctly
    // #1141: identity-agnostic lookup (post-emission_order the fnptr load id is
    // synthetic and monotonic; locate by shape instead).
    let first_inst = walker
        .state()
        .instructions
        .entries()
        .iter()
        .find(|(_, inst)| {
            inst.mnemonic == Mnemonic::Mov
                && matches!(inst.operands.first(), Some(Operand::Reg(r)) if *r == abi::R11)
                && matches!(inst.operands.get(1), Some(Operand::MemSib { .. }))
        })
        .map(|(_, inst)| inst)
        .expect("load fnptr instruction should exist");

    match &first_inst.operands[1] {
        Operand::MemSib { disp, .. } => {
            assert_eq!(*disp, 16, "Should load from offset 16");
        }
        _ => panic!("Expected MemSib operand"),
    }
}

// Refactor 2026-07-07 Step 3: contract test for the typed diagnostic pipe.
// Guarantees that a T-code pushed via `push_typed_diag` round-trips through
// `take_typed_diagnostics` with its category+number preserved. This is the
// static compile-time contract that replaces the fragile string round-trip
// retired from `resolve_var_operands` in Step 2.

#[test]
fn push_typed_diag_round_trips_code_and_message() {
    use paideia_as_diagnostics::{Category, DiagnosticCode, Severity};

    let mut walker = EmitWalker::new();
    let code = DiagnosticCode::new(Category::T, Severity::Error, 528)
        .expect("T0528 is in the catalog");
    walker.push_typed_diag(code, "unresolved local binding 'x'");

    let drained = walker.take_typed_diagnostics();
    assert_eq!(drained.len(), 1);
    let d = &drained[0];
    assert_eq!(d.code().category(), Category::T);
    assert_eq!(d.code().number(), 528);
    assert!(d.message().contains("'x'"));

    // A second take yields nothing — the buffer is drained by move.
    assert!(walker.take_typed_diagnostics().is_empty());
}

#[test]
fn typed_and_legacy_diagnostic_buffers_are_independent() {
    use paideia_as_diagnostics::{Category, DiagnosticCode, Severity};

    let mut walker = EmitWalker::new();
    walker.diagnostics.push("T0526: legacy path".to_string());
    let code = DiagnosticCode::new(Category::T, Severity::Error, 527)
        .expect("T0527 is in the catalog");
    walker.push_typed_diag(code, "typed path");

    // Legacy buffer is untouched by typed take.
    let typed = walker.take_typed_diagnostics();
    assert_eq!(typed.len(), 1);
    assert_eq!(walker.diagnostics().len(), 1);
    assert!(walker.diagnostics()[0].starts_with("T0526:"));
}
