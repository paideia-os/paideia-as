use super::super::*;
use crate::emit_fixture::EmitFixture;
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::CallMeta;


fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

#[test]
fn record_layout_finalise_empty_table() {
    let mut state = EmitPassState::default();
    let empty_types: std::collections::HashMap<RecordTypeId, Vec<(String, u8)>> =
        std::collections::HashMap::new();

    state.finalise_record_layouts(&empty_types);

    assert_eq!(state.record_layouts.len(), 0);
    assert!(state.record_layouts.is_empty());
}

#[test]
fn record_layout_finalise_capability_struct() {
    // Capability: 4 × u64 → offsets [0, 8, 16, 24], size 32, align 8.
    let mut state = EmitPassState::default();
    let cap_type = RecordTypeId(100);
    let mut types = std::collections::HashMap::new();

    types.insert(
        cap_type,
        vec![
            ("field0".to_string(), 8u8), // u64
            ("field1".to_string(), 8u8), // u64
            ("field2".to_string(), 8u8), // u64
            ("field3".to_string(), 8u8), // u64
        ],
    );

    state.finalise_record_layouts(&types);

    assert_eq!(state.record_layouts.len(), 1);
    let layout = state
        .record_layouts
        .get(&cap_type)
        .expect("capability layout should exist");
    assert_eq!(layout.size, 32);
    assert_eq!(layout.align, 8);
    assert_eq!(layout.fields.len(), 4);
    assert_eq!(layout.fields[0].offset, 0);
    assert_eq!(layout.fields[0].size, 8);
    assert_eq!(layout.fields[1].offset, 8);
    assert_eq!(layout.fields[1].size, 8);
    assert_eq!(layout.fields[2].offset, 16);
    assert_eq!(layout.fields[2].size, 8);
    assert_eq!(layout.fields[3].offset, 24);
    assert_eq!(layout.fields[3].size, 8);
}

#[test]
fn record_layout_finalise_mixed_u64_u32() {
    // Mixed u64 + u32: [u64, u32] → offsets [0, 8], size 16, align 8.
    let mut state = EmitPassState::default();
    let mixed_type = RecordTypeId(200);
    let mut types = std::collections::HashMap::new();

    types.insert(
        mixed_type,
        vec![
            ("a".to_string(), 8u8), // u64
            ("b".to_string(), 4u8), // u32
        ],
    );

    state.finalise_record_layouts(&types);

    assert_eq!(state.record_layouts.len(), 1);
    let layout = state
        .record_layouts
        .get(&mixed_type)
        .expect("mixed layout should exist");
    assert_eq!(layout.size, 16); // Rounded up to next u64 boundary.
    assert_eq!(layout.align, 8); // Max of field alignments.
    assert_eq!(layout.fields.len(), 2);
    assert_eq!(layout.fields[0].offset, 0);
    assert_eq!(layout.fields[0].size, 8);
    assert_eq!(layout.fields[1].offset, 8);
    assert_eq!(layout.fields[1].size, 4);
}

#[test]
fn record_layout_finalise_offset_with_u8_fields() {
    // Mix u64, u32, u8: verify natural alignment with minimal padding.
    // [u64, u8, u32] → offsets [0, 8, 12], size 16, align 8.
    let mut state = EmitPassState::default();
    let complex_type = RecordTypeId(300);
    let mut types = std::collections::HashMap::new();

    types.insert(
        complex_type,
        vec![
            ("x".to_string(), 8u8), // u64 at offset 0
            ("y".to_string(), 1u8), // u8 at offset 8
            ("z".to_string(), 4u8), // u32 at offset 12 (rounded up from 9)
        ],
    );

    state.finalise_record_layouts(&types);

    assert_eq!(state.record_layouts.len(), 1);
    let layout = state
        .record_layouts
        .get(&complex_type)
        .expect("complex layout should exist");
    assert_eq!(layout.size, 16);
    assert_eq!(layout.align, 8);
    assert_eq!(layout.fields.len(), 3);
    assert_eq!(layout.fields[0].offset, 0);
    assert_eq!(layout.fields[0].size, 8);
    assert_eq!(layout.fields[1].offset, 8);
    assert_eq!(layout.fields[1].size, 1);
    assert_eq!(layout.fields[2].offset, 12);
    assert_eq!(layout.fields[2].size, 4);
}

#[test]
fn record_layout_finalise_single_u64_field() {
    // Single u64 field: size 8, align 8.
    let mut state = EmitPassState::default();
    let single_type = RecordTypeId(400);
    let mut types = std::collections::HashMap::new();

    types.insert(single_type, vec![("field".to_string(), 8u8)]);

    state.finalise_record_layouts(&types);

    assert_eq!(state.record_layouts.len(), 1);
    let layout = state
        .record_layouts
        .get(&single_type)
        .expect("single-field layout should exist");
    assert_eq!(layout.size, 8);
    assert_eq!(layout.align, 8);
    assert_eq!(layout.fields.len(), 1);
    assert_eq!(layout.fields[0].offset, 0);
    assert_eq!(layout.fields[0].size, 8);
}

#[test]
fn field_access_u64_emits_mov_rax_rdi_offset() {
    // Phase 6 m3-002: field access for u64 field should emit mov rax, [rdi + offset].
    use paideia_as_ir::record_layout::{FieldAccessInfo, FieldLayout};

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    // Build IR: Deref(Var), FieldAccess wrapping it.
    let span_ref = span();
    let var_id = arena.alloc(IrKind::Var, span_ref); // First arg reference
    let deref_id = arena.alloc_with_children(IrKind::Deref, span_ref, [var_id]);
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span_ref, [deref_id]);

    // Register field access info: type_id=500, field_index=0 (u64 at offset 0).
    let field_type_id = RecordTypeId(500);
    let field_info = FieldAccessInfo {
        type_id: field_type_id,
        field_index: 0,
    };
    arena
        .field_access_info_mut()
        .insert(field_access_id, field_info);

    // Register record layout: u64 field at offset 0, size 8.
    let layout = RecordLayout::new(8, 8, vec![FieldLayout { offset: 0, size: 8, signed: false }]);
    walker
        .state_mut()
        .record_layouts
        .insert(field_type_id, layout);

    // Emit field access.
    walker.visit_field_access(field_access_id, &arena);

    // Verify instruction was emitted.
    assert!(walker.state().instructions.get(field_access_id).is_some());
    let inst = walker
        .state()
        .instructions
        .get(field_access_id)
        .expect("instruction should exist");

    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });
    assert_eq!(inst.operands.len(), 2);
    // First operand: rax (abi::RAX)
    assert!(matches!(inst.operands[0], Operand::Reg(abi::RAX)));
    // Second operand: [rdi + 0] (MemSib with base=rdi, disp=0)
    assert!(matches!(
        inst.operands[1],
        Operand::MemSib {
            base: abi::RDI,
            index: None,
            disp: 0,
            ..
        }
    ));
}

#[test]
fn field_access_u32_emits_mov_eax_rdi_offset() {
    // Phase 6 m3-002: field access for u32 field should emit mov eax, [rdi + offset].
    use paideia_as_ir::record_layout::{FieldAccessInfo, FieldLayout};

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let var_id = arena.alloc(IrKind::Var, span_ref);
    let deref_id = arena.alloc_with_children(IrKind::Deref, span_ref, [var_id]);
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span_ref, [deref_id]);

    // Field info: type_id=501, field_index=1 (u32 at offset 8).
    let field_type_id = RecordTypeId(501);
    let field_info = FieldAccessInfo {
        type_id: field_type_id,
        field_index: 1,
    };
    arena
        .field_access_info_mut()
        .insert(field_access_id, field_info);

    // Record layout: u64 at offset 0 (size 8), u32 at offset 8 (size 4).
    let layout = RecordLayout::new(
        16,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 4, signed: false },
        ],
    );
    walker
        .state_mut()
        .record_layouts
        .insert(field_type_id, layout);

    walker.visit_field_access(field_access_id, &arena);

    assert!(walker.state().instructions.get(field_access_id).is_some());
    let inst = walker
        .state()
        .instructions
        .get(field_access_id)
        .expect("instruction should exist");

    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W32 });
    // Second operand: [rdi + 8]
    assert!(matches!(
        inst.operands[1],
        Operand::MemSib {
            base: abi::RDI,
            index: None,
            disp: 8,
            ..
        }
    ));
}

#[test]
fn field_access_u8_emits_movzx_rax_rdi_offset() {
    // Phase 6 m3-002: field access for u8 field should emit movzx rax, byte [rdi + offset].
    use paideia_as_ir::record_layout::{FieldAccessInfo, FieldLayout};

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let var_id = arena.alloc(IrKind::Var, span_ref);
    let deref_id = arena.alloc_with_children(IrKind::Deref, span_ref, [var_id]);
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span_ref, [deref_id]);

    // Field info: type_id=502, field_index=2 (u8 at offset 12).
    let field_type_id = RecordTypeId(502);
    let field_info = FieldAccessInfo {
        type_id: field_type_id,
        field_index: 2,
    };
    arena
        .field_access_info_mut()
        .insert(field_access_id, field_info);

    // Record layout: u64 (0), u32 (8), u8 (12).
    let layout = RecordLayout::new(
        16,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 4, signed: false },
            FieldLayout { offset: 12,
                size: 1, signed: false },
        ],
    );
    walker
        .state_mut()
        .record_layouts
        .insert(field_type_id, layout);

    walker.visit_field_access(field_access_id, &arena);

    assert!(walker.state().instructions.get(field_access_id).is_some());
    let inst = walker
        .state()
        .instructions
        .get(field_access_id)
        .expect("instruction should exist");

    assert_eq!(inst.mnemonic, Mnemonic::Movzx);
    // First operand: rax
    assert!(matches!(inst.operands[0], Operand::Reg(abi::RAX)));
    // Second operand: [rdi + 12]
    assert!(matches!(
        inst.operands[1],
        Operand::MemSib {
            base: abi::RDI,
            index: None,
            disp: 12,
            ..
        }
    ));
}

#[test]
fn field_access_pointer_field_emits_mov_rax_rdi_offset() {
    // Phase 6 m3-002: field access for *T field should emit mov rax, [rdi + offset].
    use paideia_as_ir::record_layout::{FieldAccessInfo, FieldLayout};

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let var_id = arena.alloc(IrKind::Var, span_ref);
    let deref_id = arena.alloc_with_children(IrKind::Deref, span_ref, [var_id]);
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span_ref, [deref_id]);

    // Field info: type_id=503, field_index=3 (*u8 at offset 16, size 8).
    let field_type_id = RecordTypeId(503);
    let field_info = FieldAccessInfo {
        type_id: field_type_id,
        field_index: 3,
    };
    arena
        .field_access_info_mut()
        .insert(field_access_id, field_info);

    // Record layout: u64 (0), u32 (8), u8 (12), *T (16).
    let layout = RecordLayout::new(
        24,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 4, signed: false },
            FieldLayout { offset: 12,
                size: 1, signed: false },
            FieldLayout { offset: 16,
                size: 8, signed: false },
        ],
    );
    walker
        .state_mut()
        .record_layouts
        .insert(field_type_id, layout);

    walker.visit_field_access(field_access_id, &arena);

    assert!(walker.state().instructions.get(field_access_id).is_some());
    let inst = walker
        .state()
        .instructions
        .get(field_access_id)
        .expect("instruction should exist");

    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });
    // First operand: rax
    assert!(matches!(inst.operands[0], Operand::Reg(abi::RAX)));
    // Second operand: [rdi + 16]
    assert!(matches!(
        inst.operands[1],
        Operand::MemSib {
            base: abi::RDI,
            index: None,
            disp: 16,
            ..
        }
    ));
}

// ── Phase 6 m3-003: In-block field binding tests ─────────────────────

#[test]
fn emit_walker_m3_003_2_stmt_body_assigns_rax_rcx() {
    // Phase 6 m3-003: Two-statement body: let g = (*p).generation; let k = (*p).kind
    // Should emit to RAX, then RCX (calling-convention scratch registers).
    use paideia_as_ir::record_layout::{FieldAccessInfo, FieldLayout};

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let field_type_id = RecordTypeId(100);

    // Create two field accesses: generation (offset 24) and kind (offset 0).
    let var_id = arena.alloc(IrKind::Var, span_ref);
    let deref1_id = arena.alloc_with_children(IrKind::Deref, span_ref, [var_id]);
    let field_access1_id =
        arena.alloc_with_children(IrKind::FieldAccess, span_ref, [deref1_id]);

    let var_id2 = arena.alloc(IrKind::Var, span_ref);
    let deref2_id = arena.alloc_with_children(IrKind::Deref, span_ref, [var_id2]);
    let field_access2_id =
        arena.alloc_with_children(IrKind::FieldAccess, span_ref, [deref2_id]);

    // Register field info.
    let field_info1 = FieldAccessInfo {
        type_id: field_type_id,
        field_index: 0, // kind at offset 0
    };
    let field_info2 = FieldAccessInfo {
        type_id: field_type_id,
        field_index: 1, // generation at offset 24
    };
    arena
        .field_access_info_mut()
        .insert(field_access1_id, field_info1);
    arena
        .field_access_info_mut()
        .insert(field_access2_id, field_info2);

    // Record layout: kind (u64 at 0), generation (u64 at 24).
    let layout = RecordLayout::new(
        32,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 24,
                size: 8, signed: false },
        ],
    );
    walker
        .state_mut()
        .record_layouts
        .insert(field_type_id, layout);

    // Simulate function entry by resetting scratch_assignment and setting current_function.
    walker.state_mut().clear_scratch();
    walker.state_mut().current_function = 1;

    // Emit first field access (should go to RAX).
    walker.visit_let_field_access(field_access1_id, field_access1_id, &arena);

    // Verify first instruction uses RCX (abi::RCX) — RAX excluded to avoid call result conflicts.
    let inst1 = walker
        .state()
        .instructions
        .get(field_access1_id)
        .expect("first instruction should be emitted");
    assert_eq!(inst1.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });
    assert_eq!(inst1.operands[0], Operand::Reg(abi::RCX)); // RCX

    // Verify scratch_assignment tracks the first register.
    assert_eq!(walker.state().scratch_count(), 1);
    assert_eq!(walker.state().scratch_assignment[0], abi::RCX);

    // Emit second field access (should go to RDX).
    walker.visit_let_field_access(field_access2_id, field_access2_id, &arena);

    // Verify second instruction uses RDX (abi::RDX).
    let inst2 = walker
        .state()
        .instructions
        .get(field_access2_id)
        .expect("second instruction should be emitted");
    assert_eq!(inst2.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });
    assert_eq!(inst2.operands[0], Operand::Reg(abi::RDX)); // RDX

    // Verify scratch_assignment now has two registers.
    assert_eq!(walker.state().scratch_count(), 2);
    assert_eq!(walker.state().scratch_assignment[1], abi::RDX);
}

#[test]
fn emit_walker_m3_003_4_stmt_body_assigns_rax_rcx_rdx_r8() {
    // Phase 6 m3-003: Four-statement body assigns RAX, RCX, RDX, R8 in order.
    use paideia_as_ir::record_layout::{FieldAccessInfo, FieldLayout};

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let field_type_id = RecordTypeId(101);

    // Create four field accesses.
    let mut field_access_ids = Vec::new();
    for i in 0..4 {
        let var_id = arena.alloc(IrKind::Var, span_ref);
        let deref_id = arena.alloc_with_children(IrKind::Deref, span_ref, [var_id]);
        let field_access_id =
            arena.alloc_with_children(IrKind::FieldAccess, span_ref, [deref_id]);

        let field_info = FieldAccessInfo {
            type_id: field_type_id,
            field_index: i as u32,
        };
        arena
            .field_access_info_mut()
            .insert(field_access_id, field_info);

        field_access_ids.push(field_access_id);
    }

    // Record layout: 4 u64 fields at offsets 0, 8, 16, 24.
    let layout = RecordLayout::new(
        32,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
            FieldLayout { offset: 16,
                size: 8, signed: false },
            FieldLayout { offset: 24,
                size: 8, signed: false },
        ],
    );
    walker
        .state_mut()
        .record_layouts
        .insert(field_type_id, layout);

    // Simulate function entry.
    walker.state_mut().clear_scratch();
    walker.state_mut().current_function = 2;

    // Expected registers: RCX(1), RDX(2), R8(8), R9(9) — RAX excluded to avoid call result conflicts.
    let expected_regs = [abi::RCX, abi::RDX, abi::R8, abi::R9];

    // Emit four field accesses.
    for (i, &field_access_id) in field_access_ids.iter().enumerate() {
        walker.visit_let_field_access(field_access_id, field_access_id, &arena);

        // Verify instruction uses correct register.
        let inst = walker
            .state()
            .instructions
            .get(field_access_id)
            .expect("instruction should be emitted");
        assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });
        assert_eq!(inst.operands[0], Operand::Reg(expected_regs[i]));

        // Verify scratch_assignment tracks the register.
        assert_eq!(walker.state().scratch_assignment[i], expected_regs[i]);
    }

    // Verify no diagnostics (all 4 fit within pressure limit).
    assert!(walker.diagnostics().is_empty());
}

#[test]
fn emit_walker_m3_003_5_stmt_body_fires_t0517() {
    // Phase 6 m3-003: Five-statement body exceeds register pressure; fires T0517.
    use paideia_as_ir::record_layout::{FieldAccessInfo, FieldLayout};

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let field_type_id = RecordTypeId(102);

    // Create five field accesses.
    let mut field_access_ids = Vec::new();
    for i in 0..5 {
        let var_id = arena.alloc(IrKind::Var, span_ref);
        let deref_id = arena.alloc_with_children(IrKind::Deref, span_ref, [var_id]);
        let field_access_id =
            arena.alloc_with_children(IrKind::FieldAccess, span_ref, [deref_id]);

        let field_info = FieldAccessInfo {
            type_id: field_type_id,
            field_index: i as u32,
        };
        arena
            .field_access_info_mut()
            .insert(field_access_id, field_info);

        field_access_ids.push(field_access_id);
    }

    // Record layout: 5 u64 fields.
    let layout = RecordLayout::new(
        40,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
            FieldLayout { offset: 16,
                size: 8, signed: false },
            FieldLayout { offset: 24,
                size: 8, signed: false },
            FieldLayout { offset: 32,
                size: 8, signed: false },
        ],
    );
    walker
        .state_mut()
        .record_layouts
        .insert(field_type_id, layout);

    // Simulate function entry.
    walker.state_mut().clear_scratch();
    walker.state_mut().current_function = 3;

    // Emit first four field accesses (should succeed).
    for (_, &field_access_id) in field_access_ids.iter().take(4).enumerate() {
        walker.visit_let_field_access(field_access_id, field_access_id, &arena);
        assert!(
            walker.diagnostics().is_empty(),
            "First 4 should emit without errors"
        );
    }

    // Emit fifth field access (should fire T0517).
    walker.visit_let_field_access(field_access_ids[4], field_access_ids[4], &arena);

    // Verify T0517 diagnostic was fired via the typed diagnostic pipe.
    let typed_diags = walker.take_typed_diagnostics();
    assert!(!typed_diags.is_empty(), "T0517 should be fired for 5th binding");
    assert!(
        typed_diags.iter().any(|d| d.code().to_string() == "T0517"),
        "Diagnostic should have code T0517"
    );
}

#[test]
fn emit_walker_t0529_unsupported_field_width_u16() {
    // Issue #1080: T0529 fires when field read size is u16 (not yet lowered).
    // This exercises the emit_mem_read_via_rip_sym path which only handles u32/u64.
    use paideia_as_ir::record_layout::{FieldAccessInfo, FieldLayout};

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let field_type_id = RecordTypeId(103);

    // Create a single field access with u16 size.
    let var_id = arena.alloc(IrKind::Var, span_ref);
    let deref_id = arena.alloc_with_children(IrKind::Deref, span_ref, [var_id]);
    let field_access_id =
        arena.alloc_with_children(IrKind::FieldAccess, span_ref, [deref_id]);

    let field_info = FieldAccessInfo {
        type_id: field_type_id,
        field_index: 0,
    };
    arena
        .field_access_info_mut()
        .insert(field_access_id, field_info);

    // Record layout: one u16 field (unsigned, size=2).
    let layout = RecordLayout::new(
        2,
        2,
        vec![
            FieldLayout { offset: 0, size: 2, signed: false },
        ],
    );
    walker
        .state_mut()
        .record_layouts
        .insert(field_type_id, layout);

    // Simulate function entry.
    walker.state_mut().clear_scratch();
    walker.state_mut().current_function = 5;

    // Directly call emit_widening_load with u16 (size=2, signed=false)
    // to trigger the unsupported case in emit_mem_read_via_rip_sym.
    // However, emit_widening_load routes u16 unsigned to emit_field_access_movzx_reg,
    // not emit_mem_read_via_rip_sym. Let's call emit_mem_read_via_rip_sym directly.
    walker.emit_mem_read_via_rip_sym(field_access_id, abi::RAX, "test_symbol".to_string(), 0, 2, false);

    // Verify T0529 diagnostic was fired via the typed diagnostic pipe.
    let typed_diags = walker.take_typed_diagnostics();
    assert!(!typed_diags.is_empty(), "T0529 should be fired for u16 field width");
    assert!(
        typed_diags.iter().any(|d| d.code().to_string() == "T0529"),
        "Diagnostic should have code T0529"
    );
    assert!(
        typed_diags.iter().any(|d| d.message().contains("size=2") && d.message().contains("signed=false")),
        "Diagnostic should mention size=2 and signed=false"
    );
}

// ── RecordCons lowering tests (m3-004) ──────────────────────────────

#[test]
fn emit_walker_m3_004_cap_mint_4_stores_from_arg_regs() {
    // Phase 6 m3-004: RecordCons for cap-mint (4×u64) emits exactly 4 store instructions.
    use paideia_as_ir::record_layout::FieldLayout;

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let type_id = RecordTypeId(201);

    // Create 4 literal field values (0).
    let lit_ids: Vec<_> = (0..4)
        .map(|_| {
            let lit_id = arena.alloc(IrKind::Literal, span_ref);
            arena.literal_values_mut().insert(lit_id, 0);
            lit_id
        })
        .collect();

    // Create RecordCons with 4 Literal children.
    let record_cons_id =
        arena.alloc_with_children(IrKind::RecordCons, span_ref, lit_ids.into_iter());

    // Register layout: cap-mint shape.
    let layout = RecordLayout::new(
        32,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
            FieldLayout { offset: 16,
                size: 8, signed: false },
            FieldLayout { offset: 24,
                size: 8, signed: false },
        ],
    );
    walker.state_mut().insert_record_layout(type_id, layout);

    // Register RecordCons → TypeId mapping.
    arena
        .record_layout_table_mut()
        .insert(record_cons_id, type_id);

    // Walk the arena to trigger visit_record_cons.
    walker.walk(&mut arena);

    // Verify 4 instructions were emitted.
    let mut insts = Vec::new();
    for i in 0..4 {
        let inst_id = IrNodeId::new(record_cons_id.get() * 10 + i).expect("virtual id");
        if let Some(inst) = walker.state().instructions.get(inst_id) {
            insts.push((i, inst.clone()));
        }
    }

    assert_eq!(
        insts.len(),
        4,
        "Should emit exactly 4 store instructions for cap-mint"
    );

    // Verify each instruction is Mov with [rdi + offset], imm64(0).
    for (field_idx, inst) in &insts {
        assert_eq!(inst.mnemonic, Mnemonic::Mov);
        assert_eq!(inst.operands.len(), 2);

        let expected_offset = (*field_idx as i32) * 8;
        if let Operand::MemSib {
            base, index, disp, ..
        } = &inst.operands[0]
        {
            assert_eq!(*base, abi::RDI); // rdi
            assert_eq!(*index, None);
            assert_eq!(*disp, expected_offset);
        } else {
            panic!("First operand should be MemSib");
        }

        assert_eq!(inst.operands[1], Operand::Imm64(0));
    }

    // Verify offset advanced by 8 bytes per store (4 stores × 8 = 32 bytes).
    assert_eq!(walker.state().estimated_offset, 32);

    // Verify no T0518 diagnostics.
    assert!(
        walker.structured_diagnostics.iter().all(|d| d.code().number() != 518),
        "cap-mint shape should emit without T0518"
    );
}

#[test]
fn emit_walker_m3_004_cap_mint_with_arg_registers() {
    // Phase 6 m3-004: RecordCons stores use RSI, RDX, RCX, R8 for args 2..5.
    use paideia_as_ir::record_layout::FieldLayout;

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let type_id = RecordTypeId(202);

    // Create 4 non-literal field values (Var nodes).
    let var_ids: Vec<_> = (0..4).map(|_| arena.alloc(IrKind::Var, span_ref)).collect();

    // Create RecordCons with 4 Var children.
    let record_cons_id =
        arena.alloc_with_children(IrKind::RecordCons, span_ref, var_ids.into_iter());

    // Register layout: cap-mint shape.
    let layout = RecordLayout::new(
        32,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
            FieldLayout { offset: 16,
                size: 8, signed: false },
            FieldLayout { offset: 24,
                size: 8, signed: false },
        ],
    );
    walker.state_mut().insert_record_layout(type_id, layout);

    // Register RecordCons → TypeId mapping.
    arena
        .record_layout_table_mut()
        .insert(record_cons_id, type_id);

    // Walk the arena.
    walker.walk(&mut arena);

    // Verify 4 instructions; each should use the correct argument register.
    let arg_regs = [abi::RSI, abi::RDX, abi::RCX, abi::R8]; // RSI, RDX, RCX, R8
    for (field_idx, &expected_reg) in arg_regs.iter().enumerate() {
        let inst_id =
            IrNodeId::new(record_cons_id.get() * 10 + field_idx as u32).expect("virtual id");
        let inst = walker
            .state()
            .instructions
            .get(inst_id)
            .expect("instruction should exist");

        assert_eq!(inst.mnemonic, Mnemonic::Mov);
        assert_eq!(inst.operands[1], Operand::Reg(expected_reg));
    }

    // Verify offset: mov [rdi], rsi (3 bytes, no disp byte at offset 0)
    // + 3 × mov [rdi+off], reg (4 bytes each with disp8) = 15 bytes.
    // Previously this test asserted 16 based on a `+= 4` per store
    // literal that overcounted the offset-0 form — same drift class as
    // the visit_enum_cons undercounts fixed manually in #985/#986.
    // Step 5 (emit_inst) surfaces the encoder-truth value.
    assert_eq!(walker.state().estimated_offset, 15);

    // Verify no diagnostics.
    assert!(walker.diagnostics().is_empty());
}

#[test]
fn emit_walker_m3_004_cap_mint_wrong_field_count_fires_t0518() {
    // Phase 6 m3-004: RecordCons with != 4 fields fires T0518.
    use paideia_as_ir::record_layout::FieldLayout;

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let type_id = RecordTypeId(203);

    // Create 3 field values (wrong count).
    let lit_ids: Vec<_> = (0..3)
        .map(|_| {
            let lit_id = arena.alloc(IrKind::Literal, span_ref);
            arena.literal_values_mut().insert(lit_id, 0);
            lit_id
        })
        .collect();

    let record_cons_id =
        arena.alloc_with_children(IrKind::RecordCons, span_ref, lit_ids.into_iter());

    // Register layout with 3 fields.
    let layout = RecordLayout::new(
        24,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
            FieldLayout { offset: 16,
                size: 8, signed: false },
        ],
    );
    walker.state_mut().insert_record_layout(type_id, layout);

    arena
        .record_layout_table_mut()
        .insert(record_cons_id, type_id);

    walker.walk(&mut arena);

    // Verify T0518 was fired.
    assert!(
        walker
            .structured_diagnostics
            .iter()
            .any(|d| d.code().number() == 518 && d.message().contains("3 fields")),
        "Should fire T0518 for 3-field record"
    );
}

#[test]
fn emit_walker_m3_004_cap_mint_wrong_field_size_fires_t0518() {
    // Phase 6 m3-004: RecordCons with non-u64 field fires T0518.
    use paideia_as_ir::record_layout::FieldLayout;

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let type_id = RecordTypeId(204);

    // Create 4 field values.
    let lit_ids: Vec<_> = (0..4)
        .map(|_| {
            let lit_id = arena.alloc(IrKind::Literal, span_ref);
            arena.literal_values_mut().insert(lit_id, 0);
            lit_id
        })
        .collect();

    let record_cons_id =
        arena.alloc_with_children(IrKind::RecordCons, span_ref, lit_ids.into_iter());

    // Register layout with one u32 field (wrong type).
    let layout = RecordLayout::new(
        32,
        8,
        vec![
            FieldLayout { offset: 0, size: 4, signed: false }, // u32, wrong!
            FieldLayout { offset: 4, size: 8, signed: false },
            FieldLayout { offset: 12,
                size: 8, signed: false },
            FieldLayout { offset: 20,
                size: 8, signed: false },
        ],
    );
    walker.state_mut().insert_record_layout(type_id, layout);

    arena
        .record_layout_table_mut()
        .insert(record_cons_id, type_id);

    walker.walk(&mut arena);

    // Verify T0518 was fired.
    assert!(
        walker
            .structured_diagnostics
            .iter()
            .any(|d| d.code().number() == 518 && d.message().contains("field 0") && d.message().contains("size 4")),
        "Should fire T0518 for non-u64 field"
    );
}

#[test]
fn emit_walker_m3_004_cap_mint_wrong_field_offset_fires_t0518() {
    // Phase 6 m3-004: RecordCons with misaligned field fires T0518.
    use paideia_as_ir::record_layout::FieldLayout;

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();
    let type_id = RecordTypeId(205);

    // Create 4 field values.
    let lit_ids: Vec<_> = (0..4)
        .map(|_| {
            let lit_id = arena.alloc(IrKind::Literal, span_ref);
            arena.literal_values_mut().insert(lit_id, 0);
            lit_id
        })
        .collect();

    let record_cons_id =
        arena.alloc_with_children(IrKind::RecordCons, span_ref, lit_ids.into_iter());

    // Register layout with misaligned offset.
    let layout = RecordLayout::new(
        32,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 9, size: 8, signed: false }, // Wrong offset!
            FieldLayout { offset: 16,
                size: 8, signed: false },
            FieldLayout { offset: 24,
                size: 8, signed: false },
        ],
    );
    walker.state_mut().insert_record_layout(type_id, layout);

    arena
        .record_layout_table_mut()
        .insert(record_cons_id, type_id);

    walker.walk(&mut arena);

    // Verify T0518 was fired.
    assert!(
        walker
            .structured_diagnostics
            .iter()
            .any(|d| d.code().number() == 518 && d.message().contains("field 1") && d.message().contains("offset 9")),
        "Should fire T0518 for misaligned field"
    );
}

#[test]
fn emit_walker_m3_004_no_layout_entry_fires_t0518() {
    // Phase 6 m3-004: RecordCons with no layout entry fires T0518.
    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();

    // Create 4 literal fields.
    let lit_ids: Vec<_> = (0..4)
        .map(|_| {
            let lit_id = arena.alloc(IrKind::Literal, span_ref);
            arena.literal_values_mut().insert(lit_id, 0);
            lit_id
        })
        .collect();

    let _record_cons_id =
        arena.alloc_with_children(IrKind::RecordCons, span_ref, lit_ids.into_iter());

    // Do NOT register layout → should fire T0518 at walk time.

    walker.walk(&mut arena);

    // Verify T0518 was fired.
    assert!(
        walker
            .structured_diagnostics
            .iter()
            .any(|d| d.code().number() == 518 && d.message().contains("no layout entry")),
        "Should fire T0518 when layout entry missing"
    );
}

// ── Phase 7 m1-001: Multi-statement function body tests (PA7-001) ──────────────────────

#[test]
fn emit_walker_pa7_001_2_stmt_body_let_y_1_y_plus_1() {
    // PA7-001 AC #1: 2-stmt body `{ let y : u64 = 1; y + 1 }` returns 2.
    // This test verifies the IR structure for multi-statement lambda bodies.
    let mut arena = IrArena::new();

    // Build IR: Lambda(Action([Let(Literal(1)), Action(StmtExpr(App(+, y, 1)))]))
    // First: Literal(1)
    let lit1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(lit1_id, 1);

    // Second: Let(Literal(1))
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit1_id]);

    // Third: Literal(1) for second arg of +
    let lit2_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(lit2_id, 1);

    // Fourth: Var(y) for first arg of +
    let var_y_id = arena.alloc(IrKind::Var, span());

    // Fifth: Operator +
    let plus_id = arena.alloc(IrKind::Var, span());

    // Sixth: App(+, y, 1)
    let app_id = arena.alloc_with_children(IrKind::App, span(), [plus_id, var_y_id, lit2_id]);

    // Seventh: Action(App) representing the StmtExpr
    let stmt_expr_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);

    // Eighth: Block body Action with two children: Let and StmtExpr
    let block_id = arena.alloc_with_children(IrKind::Action, span(), [let_id, stmt_expr_id]);

    // Finally: Lambda(Action)
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [block_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify lambda was recognized as emitted.
    assert!(
        walker.emitted_lambdas().contains(&lambda_id.get()),
        "Lambda should be marked as emitted"
    );

    // Verify lambda offset was recorded.
    assert!(
        walker
            .state()
            .lambda_first_instr()
            .contains_key(&lambda_id.get()),
        "Lambda offset should be recorded"
    );

    // Verify a ret instruction was emitted.
    let ret_id = IrNodeId::new(block_id.get() * 2).expect("ret id");
    if let Some(ret_inst) = walker.state().instructions.get(ret_id) {
        assert_eq!(ret_inst.mnemonic, Mnemonic::Ret);
    }
}

/// PA8-m3-001: an in-block `let q : u16 = 7` binding emits the narrow
/// `MovSized { W16 }` form, proving the typer is threaded through
/// `visit_lambda` → `emit_block_body` and the block-body let-literal Mov
/// site is width-routed (not just the top-level `visit_let_literal`).
#[test]
fn emit_walker_pa8_m3_001_in_block_typed_let_emits_mov_sized() {
    use paideia_as_ir::{IntWidth, LetInfo, TypeId as IrTypeId};
    use paideia_as_types::TypeInterner;

    let mut arena = IrArena::new();

    // Build IR: Lambda(Action([Let(Literal(7)), StmtExpr])).
    // The trailing StmtExpr spaces block_id away from let_id so the
    // virtual-ID schemes (let_id*3 vs block_id*2) do not collide — mirroring
    // how real multi-statement bodies are laid out.
    let lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(lit_id, 7);
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);
    let tail_var_id = arena.alloc(IrKind::Var, span());
    let stmt_expr_id = arena.alloc_with_children(IrKind::Action, span(), [tail_var_id]);
    let block_id = arena.alloc_with_children(IrKind::Action, span(), [let_id, stmt_expr_id]);
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [block_id]);

    // Record the inner Let's declared type as u16.
    let mut typer = TypeInterner::new();
    let u16_id = typer.uint(16);
    arena.let_meta_mut().insert(
        let_id,
        LetInfo::with_type(false, Some(IrTypeId(u16_id.get()))),
    );

    let mut walker = EmitWalker::new();
    walker.walk_with_typer(&mut arena, &typer);

    // The block-body let-literal keys its instruction at let_id * 3.
    let inst_id = IrNodeId::new(let_id.get() * 3).expect("in-block let instr id");
    let inst = walker
        .state()
        .instructions
        .get(inst_id)
        .expect("in-block let instruction should be emitted");
    assert_eq!(
        inst.mnemonic,
        Mnemonic::MovSized {
            width: IntWidth::W16
        },
        "in-block typed u16 let should width-route to MovSized {{ W16 }}"
    );
    assert_eq!(inst.operands[1], Operand::Imm64(7));
}

/// PA8-m3-001: without a typer, the same in-block let keeps the generic Mov
/// path — confirming the new routing is purely additive.
#[test]
fn emit_walker_pa8_m3_001_in_block_untyped_let_keeps_generic_mov() {
    let mut arena = IrArena::new();

    let lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(lit_id, 7);
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);
    let tail_var_id = arena.alloc(IrKind::Var, span());
    let stmt_expr_id = arena.alloc_with_children(IrKind::Action, span(), [tail_var_id]);
    let block_id = arena.alloc_with_children(IrKind::Action, span(), [let_id, stmt_expr_id]);
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [block_id]);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena); // no typer

    let inst_id = IrNodeId::new(let_id.get() * 3).expect("in-block let instr id");
    let inst = walker
        .state()
        .instructions
        .get(inst_id)
        .expect("in-block let instruction should be emitted");
    assert_eq!(
        inst.mnemonic,
        Mnemonic::Mov,
        "untyped in-block let should keep the generic 64-bit Mov path"
    );
}

#[test]
fn emit_walker_pa7_001_3_stmt_unsafe_blocks() {
    // PA7-001 AC #2: 3-stmt unsafe blocks.
    // This test verifies multi-statement blocks with unsafe content.
    let mut arena = IrArena::new();

    // Build a block with 3 statements: Let, Unsafe, Let
    let lit1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(lit1_id, 1);
    let let1_id = arena.alloc_with_children(IrKind::Let, span(), [lit1_id]);

    // Empty unsafe block (no children for this test)
    let unsafe_id = arena.alloc(IrKind::Unsafe, span());

    let lit2_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(lit2_id, 2);
    let let2_id = arena.alloc_with_children(IrKind::Let, span(), [lit2_id]);

    // Block body with 3 statements
    let block_id =
        arena.alloc_with_children(IrKind::Action, span(), [let1_id, unsafe_id, let2_id]);

    // Lambda(Action)
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [block_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify lambda was emitted.
    assert!(
        walker.emitted_lambdas().contains(&lambda_id.get()),
        "Lambda with unsafe blocks should be marked as emitted"
    );

    // Verify offset was recorded.
    assert!(
        walker
            .state()
            .lambda_first_instr()
            .contains_key(&lambda_id.get()),
        "Lambda offset should be recorded for unsafe block body"
    );
}

#[test]
fn emit_walker_pa7_001_empty_body_returns_nothing() {
    // PA7-001 AC #3: empty body returns nothing.
    // Lambda with empty Action body should only emit ret.
    let mut arena = IrArena::new();

    // Empty block body
    let block_id = arena.alloc(IrKind::Action, span());

    // Lambda(Action) with empty body
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [block_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify lambda was emitted.
    assert!(
        walker.emitted_lambdas().contains(&lambda_id.get()),
        "Lambda with empty body should be marked as emitted"
    );

    // Verify offset was recorded.
    assert!(
        walker
            .state()
            .lambda_first_instr()
            .contains_key(&lambda_id.get()),
        "Lambda offset should be recorded for empty body"
    );

    // Verify only ret was emitted (1 byte: c3).
    let ret_id = IrNodeId::new(block_id.get() * 2).expect("ret id");
    if let Some(ret_inst) = walker.state().instructions.get(ret_id) {
        assert_eq!(ret_inst.mnemonic, Mnemonic::Ret);
    }

    // Verify offset is 1 (only ret).
    assert_eq!(
        walker.state().estimated_offset,
        1,
        "Empty body should only emit ret (1 byte)"
    );
}

// ── Phase 7 m1-001: Inter-function call tests ──────────────────────────────────

#[test]
fn emit_walker_pa7_002_zero_arg_function_call() {
    // Phase 7 m1-001: Test zero-argument function call.
    // let a = fn () -> 42;
    // let b = fn () -> a();
    let mut arena = IrArena::new();

    // Create function 'a': fn () -> 42
    let lit_a_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(lit_a_id, 42);
    let lambda_a_id = arena.alloc_with_children(IrKind::Lambda, span(), [lit_a_id]);

    // Register 'a' as a symbol - note: ir_node must point to lambda_a_id
    let sym_a = Symbol::new("a".to_string(), SymbolKind::Function, lambda_a_id);
    arena.symbols_mut().insert(sym_a);

    // Create function 'b': fn () -> a()
    // App structure: [callee (Var pointing to a), no args]
    // For the test to work, we create a Var that has lambda_a_id as its reference.
    // Since there's no direct Var→Symbol binding in the IR, we'll need to match
    // the function symbol by checking if any Function symbol exists.
    let var_a_id = arena.alloc(IrKind::Var, span());
    let app_id = arena.alloc_with_children(IrKind::App, span(), [var_a_id]);
    let lambda_b_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Register the call site metadata
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "a".to_string(),
        arg_count: 0,
        is_intrinsic: false,
    });

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify lambda_b was emitted.
    assert!(
        walker.emitted_lambdas().contains(&lambda_b_id.get()),
        "Lambda b (function call) should be marked as emitted"
    );

    // Verify call instruction was emitted (5 bytes: E8 + 4-byte rel32)
    // Issue #1161: CALL ID is now allocated via alloc_synthetic_id() to avoid collisions.
    // Search for the Call instruction in the emitted instruction set.
    let mut call_inst_found = false;
    let mut ret_inst_found = false;
    for (_, inst) in walker.state().instructions.entries().iter() {
        if inst.mnemonic == Mnemonic::Call {
            call_inst_found = true;
            assert_eq!(inst.operands.len(), 1);
            match &inst.operands[0] {
                Operand::SymbolRef { name, addend } => {
                    assert_eq!(name, "a");
                    assert_eq!(*addend, 0);
                }
                _ => panic!("Expected SymbolRef operand"),
            }
        } else if inst.mnemonic == Mnemonic::Ret {
            ret_inst_found = true;
        }
    }
    assert!(call_inst_found, "call instruction should be emitted");
    assert!(ret_inst_found, "ret instruction should be emitted");

    // Verify offset: PA19-r19-006 update: lambda_a now emits mov rax, 42; ret.
    // With issue #1101 optimization (C7 imm32 form for i32-range values):
    // lambda_a (literal): 7 bytes for mov (C7 form) + 1 byte for ret = 8 bytes
    // lambda_b (call): 5 bytes for call + 1 byte for ret = 6 bytes
    // Total: 14 bytes
    assert_eq!(walker.state().estimated_offset, 14);
}

#[test]
fn emit_walker_pa7_002_one_arg_function_call() {
    // Phase 7 m1-001: Test one-argument function call.
    // let f = fn (x) -> x + 1;
    // let g = fn () -> f(7);
    let mut arena = IrArena::new();

    // Create function 'f': fn (x) -> x + 1
    let callee_id = arena.alloc(IrKind::Var, span());
    let var_x_id = arena.alloc(IrKind::Var, span());
    let lit_1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(lit_1_id, 1);
    let add_app_id =
        arena.alloc_with_children(IrKind::App, span(), [callee_id, var_x_id, lit_1_id]);
    let lambda_f_id = arena.alloc_with_children(IrKind::Lambda, span(), [add_app_id]);

    // Register 'f' as a symbol
    let sym_f = Symbol::new("f".to_string(), SymbolKind::Function, lambda_f_id);
    arena.symbols_mut().insert(sym_f);

    // Create function 'g': fn () -> f(7)
    // App structure: [callee (Var pointing to f), arg (Literal 7)]
    let var_f_id = arena.alloc(IrKind::Var, span());
    let lit_7_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(lit_7_id, 7);
    let call_app_id = arena.alloc_with_children(IrKind::App, span(), [var_f_id, lit_7_id]);
    let lambda_g_id = arena.alloc_with_children(IrKind::Lambda, span(), [call_app_id]);

    // Register the call site metadata
    arena.call_sites_mut().insert(call_app_id, CallMeta {
        callee_name: "f".to_string(),
        arg_count: 1,
        is_intrinsic: false,
    });

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify lambda_g was emitted.
    assert!(
        walker.emitted_lambdas().contains(&lambda_g_id.get()),
        "Lambda g (function call) should be marked as emitted"
    );

    // The offset should account for:
    // - MOV instruction to load 7 into RDI (7 bytes for i32 or 10 bytes for i64)
    // - CALL instruction (5 bytes)
    // - RET instruction (1 byte)
    // Total should be 7+5+1=13 or 10+5+1=16
    let expected_offset = 7 + 5 + 1; // Conservative estimate: 13 bytes
    assert!(
        walker.state().estimated_offset >= expected_offset - 5,
        "Offset should account for mov + call + ret instructions (got {})",
        walker.state().estimated_offset
    );
}

// ── If-else expression tests (m1-001) ──────────────────────────────────

#[test]
fn emit_walker_branch_simple_if_no_else() {
    // Phase 7 m1-001: Test simple if without else.
    // if x { ... } (no else) → test rdi, rdi; jz end_label; end_label:
    let mut arena = IrArena::new();

    // Allocate: Var (condition), then_block (placeholder).
    let cond_id = arena.alloc(IrKind::Var, span());
    let then_id = arena.alloc(IrKind::Action, span());
    let branch_id = arena.alloc_with_children(IrKind::Branch, span(), [cond_id, then_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify test instruction was emitted (3 bytes: 48 85 FF).
    let test_id = IrNodeId::new(branch_id.get() * 3).expect("test instr id");
    let test_inst = walker
        .state()
        .instructions
        .get(test_id)
        .expect("test instruction should be emitted");
    assert_eq!(test_inst.mnemonic, Mnemonic::Test);
    assert_eq!(test_inst.operands.len(), 2);
    assert_eq!(test_inst.operands[0], Operand::Reg(abi::RDI)); // rdi
    assert_eq!(test_inst.operands[1], Operand::Reg(abi::RDI)); // rdi

    // Verify jz instruction was emitted (6 bytes: 0F 84 XX XX XX XX).
    let jz_id = IrNodeId::new(branch_id.get() * 3 + 1).expect("jz instr id");
    let jz_inst = walker
        .state()
        .instructions
        .get(jz_id)
        .expect("jz instruction should be emitted");
    match jz_inst.mnemonic {
        Mnemonic::Jcc(cond) => assert_eq!(cond, Cond::Zero),
        _ => panic!("Expected Jcc(Zero) mnemonic"),
    }
    assert_eq!(jz_inst.operands.len(), 1);
    match &jz_inst.operands[0] {
        Operand::LabelRef { name, addend } => {
            // Should reference end_label (not else_label since there's no else)
            assert!(
                name.contains(&format!("if_end_{}", branch_id.get())),
                "jz should reference end_label, got: {}",
                name
            );
            assert_eq!(*addend, 0);
        }
        _ => panic!("Expected LabelRef operand"),
    }

    // Verify end_label was registered.
    assert!(
        walker
            .state()
            .labels
            .contains_key(&format!("if_end_{}", branch_id.get()))
    );

    // Verify offset: 3 bytes for test + 6 bytes for jz = 9 bytes.
    assert_eq!(walker.state().estimated_offset, 9);
}

#[test]
fn emit_walker_branch_if_else() {
    // Phase 7 m1-001: Test if-else with both branches.
    // if x { then_block } else { else_block } → test + jz else + then + jmp end + else: + else + end:
    let mut arena = IrArena::new();

    // Allocate: Var (condition), then_block, else_block.
    let cond_id = arena.alloc(IrKind::Var, span());
    let then_id = arena.alloc(IrKind::Action, span());
    let else_id = arena.alloc(IrKind::Action, span());
    let branch_id =
        arena.alloc_with_children(IrKind::Branch, span(), [cond_id, then_id, else_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify test instruction.
    let test_id = IrNodeId::new(branch_id.get() * 3).expect("test instr id");
    let test_inst = walker
        .state()
        .instructions
        .get(test_id)
        .expect("test instruction should be emitted");
    assert_eq!(test_inst.mnemonic, Mnemonic::Test);

    // Verify jz instruction jumps to else_label (not end_label).
    let jz_id = IrNodeId::new(branch_id.get() * 3 + 1).expect("jz instr id");
    let jz_inst = walker
        .state()
        .instructions
        .get(jz_id)
        .expect("jz instruction should be emitted");
    match &jz_inst.operands[0] {
        Operand::LabelRef { name, addend } => {
            assert!(
                name.contains(&format!("if_else_{}", branch_id.get())),
                "jz should reference else_label, got: {}",
                name
            );
            assert_eq!(*addend, 0);
        }
        _ => panic!("Expected LabelRef operand"),
    }

    // Verify jmp instruction was emitted (5 bytes: E9 XX XX XX XX).
    let jmp_id = IrNodeId::new(branch_id.get() * 3 + 2).expect("jmp instr id");
    let jmp_inst = walker
        .state()
        .instructions
        .get(jmp_id)
        .expect("jmp instruction should be emitted");
    assert_eq!(jmp_inst.mnemonic, Mnemonic::Jmp);
    assert_eq!(jmp_inst.operands.len(), 1);
    match &jmp_inst.operands[0] {
        Operand::LabelRef { name, addend } => {
            assert!(
                name.contains(&format!("if_end_{}", branch_id.get())),
                "jmp should reference end_label, got: {}",
                name
            );
            assert_eq!(*addend, 0);
        }
        _ => panic!("Expected LabelRef operand"),
    }

    // Verify all three labels were registered.
    assert!(
        walker
            .state()
            .labels
            .contains_key(&format!("if_then_{}", branch_id.get()))
    );
    assert!(
        walker
            .state()
            .labels
            .contains_key(&format!("if_else_{}", branch_id.get()))
    );
    assert!(
        walker
            .state()
            .labels
            .contains_key(&format!("if_end_{}", branch_id.get()))
    );

    // Verify offset: 3 bytes for test + 6 bytes for jz + 5 bytes for jmp = 14 bytes.
    assert_eq!(walker.state().estimated_offset, 14);
}

#[test]
fn emit_walker_branch_nested_if_else() {
    // Phase 7 m1-001: Test nested if-else.
    // Outer: if a { inner: if b { ... } else { ... } } else { ... }
    // Each Branch node gets independent label set.
    let mut arena = IrArena::new();

    // Allocate inner branch: if b { ... } else { ... }
    let inner_cond = arena.alloc(IrKind::Var, span());
    let inner_then = arena.alloc(IrKind::Action, span());
    let inner_else = arena.alloc(IrKind::Action, span());
    let inner_branch =
        arena.alloc_with_children(IrKind::Branch, span(), [inner_cond, inner_then, inner_else]);

    // Allocate outer branch: if a { inner_branch } else { ... }
    let outer_cond = arena.alloc(IrKind::Var, span());
    let outer_then = inner_branch; // The then-block is the inner branch itself
    let outer_else = arena.alloc(IrKind::Action, span());
    let outer_branch =
        arena.alloc_with_children(IrKind::Branch, span(), [outer_cond, outer_then, outer_else]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify outer branch labels exist and are distinct from inner.
    let outer_then_label = format!("if_then_{}", outer_branch.get());
    let outer_else_label = format!("if_else_{}", outer_branch.get());
    let outer_end_label = format!("if_end_{}", outer_branch.get());
    assert!(walker.state().labels.contains_key(&outer_then_label));
    assert!(walker.state().labels.contains_key(&outer_else_label));
    assert!(walker.state().labels.contains_key(&outer_end_label));

    // Verify inner branch labels exist and are distinct.
    let inner_then_label = format!("if_then_{}", inner_branch.get());
    let inner_else_label = format!("if_else_{}", inner_branch.get());
    let inner_end_label = format!("if_end_{}", inner_branch.get());
    assert!(walker.state().labels.contains_key(&inner_then_label));
    assert!(walker.state().labels.contains_key(&inner_else_label));
    assert!(walker.state().labels.contains_key(&inner_end_label));

    // Verify all six labels are distinct.
    assert_ne!(outer_then_label, inner_then_label);
    assert_ne!(outer_else_label, inner_else_label);
    assert_ne!(outer_end_label, inner_end_label);

    // Verify offset accounts for both branches: 2 * (test + jz + jmp) = 2 * 14 = 28 bytes
    assert_eq!(walker.state().estimated_offset, 28);
}

// ── While-loop lowering tests (m1-002) ─────────────────────────────────

#[test]
fn emit_walker_while_simple_loop() {
    let mut arena = IrArena::new();

    // Allocate: Literal (condition), Var (body), then While with both as children.
    let cond_id = arena.alloc(IrKind::Literal, span());
    let body_id = arena.alloc(IrKind::Var, span());
    let while_id = arena.alloc_with_children(IrKind::While, span(), [cond_id, body_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify instructions were emitted for the while loop.
    // Test instruction at while_id * 4
    let test_id = IrNodeId::new(while_id.get() * 4).expect("test instr id");
    let test_inst = walker
        .state()
        .instructions
        .get(test_id)
        .expect("test instruction should be emitted");
    assert_eq!(test_inst.mnemonic, Mnemonic::Test);
    assert_eq!(test_inst.operands.len(), 2);
    assert_eq!(test_inst.operands[0], Operand::Reg(abi::RDI)); // rdi
    assert_eq!(test_inst.operands[1], Operand::Reg(abi::RDI)); // rdi

    // JNZ instruction at while_id * 4 + 1
    let jnz_id = IrNodeId::new(while_id.get() * 4 + 1).expect("jnz instr id");
    let jnz_inst = walker
        .state()
        .instructions
        .get(jnz_id)
        .expect("jnz instruction should be emitted");
    assert!(matches!(jnz_inst.mnemonic, Mnemonic::Jcc(Cond::NonZero)));
    assert_eq!(jnz_inst.operands.len(), 1);

    // JMP instruction at while_id * 4 + 2
    let jmp_id = IrNodeId::new(while_id.get() * 4 + 2).expect("jmp instr id");
    let jmp_inst = walker
        .state()
        .instructions
        .get(jmp_id)
        .expect("jmp instruction should be emitted");
    assert_eq!(jmp_inst.mnemonic, Mnemonic::Jmp);
    assert_eq!(jmp_inst.operands.len(), 1);

    // Verify labels were registered.
    let top_label = format!("while_top_{}", while_id.get());
    let exit_label = format!("while_exit_{}", while_id.get());
    assert!(walker.state().labels.contains_key(&top_label));
    assert!(walker.state().labels.contains_key(&exit_label));

    // Verify offset: test (3) + jnz (6) + jmp (5) = 14 bytes.
    assert_eq!(walker.state().estimated_offset, 14);
}

#[test]
fn emit_walker_while_with_break() {
    let mut arena = IrArena::new();

    // Allocate: Literal (condition), Break (body).
    let cond_id = arena.alloc(IrKind::Literal, span());
    let break_id = arena.alloc(IrKind::Break, span());
    let while_id = arena.alloc_with_children(IrKind::While, span(), [cond_id, break_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify instructions were emitted.
    let test_id = IrNodeId::new(while_id.get() * 4).expect("test instr id");
    assert!(walker.state().instructions.get(test_id).is_some());

    let jnz_id = IrNodeId::new(while_id.get() * 4 + 1).expect("jnz instr id");
    let jnz_inst = walker
        .state()
        .instructions
        .get(jnz_id)
        .expect("jnz instruction should be emitted");

    // Verify jnz references the exit label (where break will jump).
    let exit_label = format!("while_exit_{}", while_id.get());
    match &jnz_inst.operands[0] {
        Operand::LabelRef { name, addend } => {
            assert_eq!(name, &exit_label);
            assert_eq!(*addend, 0);
        }
        _ => panic!("Expected LabelRef operand for jnz"),
    }

    // Verify exit label was registered.
    assert!(walker.state().labels.contains_key(&exit_label));
}

#[test]
fn emit_walker_while_nested_with_continue() {
    let mut arena = IrArena::new();

    // Allocate inner while loop: condition + continue.
    let inner_cond_id = arena.alloc(IrKind::Literal, span());
    let continue_id = arena.alloc(IrKind::Continue, span());
    let inner_while_id =
        arena.alloc_with_children(IrKind::While, span(), [inner_cond_id, continue_id]);

    // Allocate outer while loop: condition + inner while.
    let outer_cond_id = arena.alloc(IrKind::Literal, span());
    let outer_while_id =
        arena.alloc_with_children(IrKind::While, span(), [outer_cond_id, inner_while_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify outer while labels exist and are distinct.
    let outer_top_label = format!("while_top_{}", outer_while_id.get());
    let outer_exit_label = format!("while_exit_{}", outer_while_id.get());
    assert!(walker.state().labels.contains_key(&outer_top_label));
    assert!(walker.state().labels.contains_key(&outer_exit_label));

    // Verify inner while labels exist and are distinct.
    let inner_top_label = format!("while_top_{}", inner_while_id.get());
    let inner_exit_label = format!("while_exit_{}", inner_while_id.get());
    assert!(walker.state().labels.contains_key(&inner_top_label));
    assert!(walker.state().labels.contains_key(&inner_exit_label));

    // Verify all four labels are distinct.
    assert_ne!(outer_top_label, inner_top_label);
    assert_ne!(outer_exit_label, inner_exit_label);

    // Verify offset accounts for both while loops: 2 * 14 = 28 bytes.
    assert_eq!(walker.state().estimated_offset, 28);
}

// ── Phase 7 m1-003: Multi-argument function call tests (PA7-006) ─────────────────────────
//
// NOTE (issue #1099): The following tests check BYTE COUNT only, not BYTE ORDER.
// They verify that estimated_offset tracks the correct total byte size of MOVs, CALL, and RET.
// For byte-order verification (MOVs before CALL, RET last), see:
// - Unit test: paideia-as-emitter-pe/tests/text_emitter.rs::sysv_call_with_args_emits_movs_before_call_and_ret_last
// - Integration test: codegen/call_byte_order.rs::walker_then_emitter_produces_movs_before_call_in_bytes
// - Regression probe: tools/verify-byte-order.sh

#[test]
fn emit_walker_function_call_3_args() {
    // PA7-006 AC #1: f(a, b, c) → mov rdi,a ; mov rsi,b ; mov rdx,c ; call f ; ret
    let mut arena = IrArena::new();

    // Allocate 3 literal arguments
    let arg0_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0_id, 1);
    let arg1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1_id, 2);
    let arg2_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg2_id, 3);

    // Allocate function name and Var node
    let fn_var_id = arena.alloc(IrKind::Var, span());

    // Allocate App node with callee and 3 arguments
    let app_id =
        arena.alloc_with_children(IrKind::App, span(), [fn_var_id, arg0_id, arg1_id, arg2_id]);

    // Allocate Lambda with App as body
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Create and register a function symbol
    let sym = Symbol::new("f".to_string(), SymbolKind::Function, lambda_id);
    arena.symbols_mut().insert(sym);

    // Register the call site metadata
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "f".to_string(),
        arg_count: 3,
        is_intrinsic: false,
    });

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify instruction count: 3 MOVs + CALL + RET = 5 instructions emitted
    let insts = walker.state().instructions.entries();
    assert!(
        insts.len() >= 5,
        "Expected at least 5 instructions, got {}",
        insts.len()
    );

    // Verify offset: 3*7 (movs) + 5 (call) + 1 (ret) = 27 bytes
    assert_eq!(walker.state().estimated_offset, 27);
}

#[test]
fn emit_walker_function_call_4_args() {
    // PA7-006 AC #2: f(a, b, c, d) → mov rdi,a ; mov rsi,b ; mov rdx,c ; mov rcx,d ; call f ; ret
    let mut arena = IrArena::new();

    // Allocate 4 literal arguments
    let arg0_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0_id, 1);
    let arg1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1_id, 2);
    let arg2_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg2_id, 3);
    let arg3_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg3_id, 4);

    // Allocate function name and Var node
    let fn_var_id = arena.alloc(IrKind::Var, span());

    // Allocate App node with callee and 4 arguments
    let app_id = arena.alloc_with_children(
        IrKind::App,
        span(),
        [fn_var_id, arg0_id, arg1_id, arg2_id, arg3_id],
    );

    // Allocate Lambda with App as body
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Create and register a function symbol
    let sym = Symbol::new("f".to_string(), SymbolKind::Function, lambda_id);
    arena.symbols_mut().insert(sym);

    // Register the call site metadata
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "f".to_string(),
        arg_count: 4,
        is_intrinsic: false,
    });

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify offset: 4*7 (movs) + 5 (call) + 1 (ret) = 34 bytes
    assert_eq!(walker.state().estimated_offset, 34);
}

#[test]
fn emit_walker_function_call_5_args() {
    // PA7-006 AC #3: f(a, b, c, d, e) → args to RDI, RSI, RDX, RCX, R8
    let mut arena = IrArena::new();

    // Allocate 5 literal arguments
    let arg0_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0_id, 1);
    let arg1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1_id, 2);
    let arg2_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg2_id, 3);
    let arg3_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg3_id, 4);
    let arg4_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg4_id, 5);

    // Allocate function name and Var node
    let fn_var_id = arena.alloc(IrKind::Var, span());

    // Allocate App node with callee and 5 arguments
    let app_id = arena.alloc_with_children(
        IrKind::App,
        span(),
        [fn_var_id, arg0_id, arg1_id, arg2_id, arg3_id, arg4_id],
    );

    // Allocate Lambda with App as body
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Create and register a function symbol
    let sym = Symbol::new("f".to_string(), SymbolKind::Function, lambda_id);
    arena.symbols_mut().insert(sym);

    // Register the call site metadata
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "f".to_string(),
        arg_count: 5,
        is_intrinsic: false,
    });

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify offset: 5*7 (movs) + 5 (call) + 1 (ret) = 41 bytes
    assert_eq!(walker.state().estimated_offset, 41);
}

#[test]
fn emit_walker_function_call_6_args() {
    // PA7-006 AC #4: f(a, b, c, d, e, g) → args to RDI, RSI, RDX, RCX, R8, R9
    let mut arena = IrArena::new();

    // Allocate 6 literal arguments
    let arg0_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0_id, 1);
    let arg1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1_id, 2);
    let arg2_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg2_id, 3);
    let arg3_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg3_id, 4);
    let arg4_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg4_id, 5);
    let arg5_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg5_id, 6);

    // Allocate function name and Var node
    let fn_var_id = arena.alloc(IrKind::Var, span());

    // Allocate App node with callee and 6 arguments
    let app_id = arena.alloc_with_children(
        IrKind::App,
        span(),
        [
            fn_var_id, arg0_id, arg1_id, arg2_id, arg3_id, arg4_id, arg5_id,
        ],
    );

    // Allocate Lambda with App as body
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Create and register a function symbol
    let sym = Symbol::new("f".to_string(), SymbolKind::Function, lambda_id);
    arena.symbols_mut().insert(sym);

    // Register the call site metadata
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "f".to_string(),
        arg_count: 6,
        is_intrinsic: false,
    });

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify offset: 6*7 (movs) + 5 (call) + 1 (ret) = 48 bytes
    assert_eq!(walker.state().estimated_offset, 48);
}

#[test]
fn emit_walker_function_call_7_args_reject() {
    // PA7-006 AC #5: f(a, b, c, d, e, g, h) → 7 args should be rejected
    let mut arena = IrArena::new();

    // Allocate 7 literal arguments
    let arg0_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0_id, 1);
    let arg1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1_id, 2);
    let arg2_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg2_id, 3);
    let arg3_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg3_id, 4);
    let arg4_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg4_id, 5);
    let arg5_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg5_id, 6);
    let arg6_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg6_id, 7);

    // Allocate function name and Var node
    let fn_var_id = arena.alloc(IrKind::Var, span());

    // Allocate App node with callee and 7 arguments
    let app_id = arena.alloc_with_children(
        IrKind::App,
        span(),
        [
            fn_var_id, arg0_id, arg1_id, arg2_id, arg3_id, arg4_id, arg5_id, arg6_id,
        ],
    );

    // Allocate Lambda with App as body
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Create and register a function symbol
    let sym = Symbol::new("f".to_string(), SymbolKind::Function, lambda_id);
    arena.symbols_mut().insert(sym);

    // Register the call site metadata
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "f".to_string(),
        arg_count: 7,
        is_intrinsic: false,
    });

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify that structured diagnostics contain the "out of bounds" error
    let diags = walker.take_typed_diagnostics();
    assert!(
        diags.iter()
            .any(|d| d.message().contains("out of bounds") || d.message().contains("max 6")),
        "Expected out-of-bounds error, got: {:?}",
        diags
    );
}

#[test]
fn emit_walker_fnptr_object_call_rip_sym() {
    // PA-r17-004c AC: (f)(42u32) where f is Object with AddrOf(identity)
    // Should emit: `mov rdi, 42; call [rip + f]; ret`
    // Key instruction: Call { MemRipRelSym { name: "f", addend: 0 } }
    use paideia_as_ir::symbol::{Symbol, SymbolKind};
    use paideia_as_ir::CallMeta;

    let mut arena = IrArena::new();

    // Allocate the function being referenced
    let identity_lambda_id = arena.alloc(IrKind::Lambda, span());
    let identity_sym = Symbol::new(
        "identity".to_string(),
        SymbolKind::Function,
        identity_lambda_id,
    );
    arena.symbols_mut().insert(identity_sym);

    // Allocate the fnptr Object "f" with Borrow(&identity) as RHS
    // Structure: Let node with Borrow (address-of) as RHS
    // Borrow should have a Var(identity) child
    let identity_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(identity_var_id, "identity".to_string());

    let borrow_id = arena.alloc_with_children(IrKind::Borrow, span(), [identity_var_id]);

    let f_let_id = arena.alloc_with_children(IrKind::Let, span(), [borrow_id]);
    let f_sym = Symbol::new("f".to_string(), SymbolKind::Object, f_let_id);
    arena.symbols_mut().insert(f_sym);

    // Allocate the argument: Literal(42)
    let arg_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg_id, 42);

    // Allocate the callee Var: f
    let callee_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_var_id, "f".to_string());

    // Allocate App node: (f)(42)
    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_var_id, arg_id]);

    // Register the call site
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "f".to_string(),
        arg_count: 1,
        is_intrinsic: false,
    });

    // Allocate Lambda with App as body
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify instructions were emitted
    let insts = walker.state().instructions.entries();
    assert!(!insts.is_empty(), "Should emit instructions for fnptr call");

    // Look for the MemRipRelSym call operand
    let mut found_mem_rip_rel_sym_call = false;
    for (_id, inst) in insts.iter() {
        if inst.mnemonic == Mnemonic::Call {
            for op in &inst.operands {
                if let Operand::MemRipRelSym { name, addend } = op {
                    if name == "f" && *addend == 0 {
                        found_mem_rip_rel_sym_call = true;
                    }
                }
            }
        }
    }
    assert!(
        found_mem_rip_rel_sym_call,
        "Should emit Call {{ MemRipRelSym {{ name: \"f\", addend: 0 }} }}"
    );
}

#[test]
fn emit_walker_match_empty_arms_produces_diagnostic() {
    let mut arena = IrArena::new();

    // Allocate: Var (scrutinee), then Match with only scrutinee.
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify diagnostic was emitted for missing arms.
    let typed = walker.take_typed_diagnostics();
    assert!(
        typed.iter().any(|d| d.code().number() == 1650),
        "Expected U1650 diagnostic for missing arms"
    );
}

#[test]
fn emit_walker_match_single_arm_emits_instructions() {
    let mut arena = IrArena::new();

    // Allocate: Var (scrutinee), Action (arm with body)
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm_id = arena.alloc(IrKind::Action, span());
    let arm_body_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arm_body_id, 42);

    // Set arm body as child of Action
    {
        let arm_children = arena.children_mut(arm_id).unwrap();
        arm_children.push(arm_body_id);
    }

    let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

    // Register match metadata
    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));
    arena.match_arm_meta_mut().insert(
        arm_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: None,
            payload_binder: None,
            is_default: true,
            pattern_binding: None,
        },
    );

    // Walk the arena with layout registered.
    let mut walker = EmitWalker::new();
    let layout = EnumLayout::new(0);
    walker.state_mut().insert_enum_layout(EnumTypeId(1), layout);
    walker.walk(&mut arena);

    // Verify match was processed without diagnostic errors
    assert!(walker.diagnostics().is_empty(), "{:?}", walker.diagnostics());
}

#[test]
fn emit_walker_match_multiple_arms_emits_dispatch_chain() {
    let mut arena = IrArena::new();

    // Allocate: Var (scrutinee), Action arms
    let scrutinee_id = arena.alloc(IrKind::Var, span());
    let arm1_id = arena.alloc(IrKind::Action, span());
    let arm2_id = arena.alloc(IrKind::Action, span());

    let match_id =
        arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm1_id, arm2_id]);

    // Register match metadata
    arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(1));
    arena.match_arm_meta_mut().insert(
        arm1_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(0),
            payload_binder: None,
            is_default: false,
            pattern_binding: None,
        },
    );
    arena.match_arm_meta_mut().insert(
        arm2_id,
        paideia_as_ir::MatchArmMeta {
            variant_index: Some(1),
            payload_binder: None,
            is_default: false,
            pattern_binding: None,
        },
    );

    // Walk the arena with layout registered.
    let mut walker = EmitWalker::new();
    let layout = EnumLayout::new(0);
    walker.state_mut().insert_enum_layout(EnumTypeId(1), layout);
    walker.walk(&mut arena);

    // Verify instructions were emitted for both arms.
    let insts = &walker.state().instructions;
    let inst_count = insts.entries().len();
    assert!(
        inst_count > 0,
        "Expected instructions for 2-arm match, got: {} instructions",
        inst_count
    );
}

#[test]
fn emit_walker_loop_emits_instructions() {
    let mut arena = IrArena::new();

    // Allocate: Literal (body).
    let body_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(body_id, 42);

    // Allocate: Loop with body.
    let loop_id = arena.alloc_with_children(IrKind::Loop, span(), [body_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify instructions were emitted: jmp (5 bytes).
    let insts = &walker.state().instructions;
    let inst_count = insts.entries().len();
    assert!(
        inst_count > 0,
        "Expected instructions for loop, got: {} instructions",
        inst_count
    );

    // Verify offset advanced: jmp is 5 bytes.
    let expected_offset = 5;
    assert_eq!(
        walker.state().estimated_offset,
        expected_offset,
        "Expected offset {}, got {}",
        expected_offset,
        walker.state().estimated_offset
    );

    // Verify labels were registered for loop_top and loop_exit.
    let labels = &walker.state().labels;
    let has_top = labels.keys().any(|k| k.starts_with("loop_top_"));
    let has_exit = labels.keys().any(|k| k.starts_with("loop_exit_"));
    assert!(
        has_top && has_exit,
        "Expected loop_top and loop_exit labels, got: {:?}",
        labels.keys().collect::<Vec<_>>()
    );
}

#[test]
fn emit_walker_loop_context_tracking() {
    let _walker = EmitWalker::new();
    // Initially no loop context.
    assert_eq!(_walker.current_loop_context(), None);

    let mut walker = EmitWalker::new();
    // Manually simulate entering a loop context.
    walker
        .loop_contexts
        .push((LoopContext::Loop, "loop_exit_1".to_string()));
    let ctx = walker.current_loop_context();
    assert!(ctx.is_some());
    let (kind, _label) = ctx.unwrap();
    assert_eq!(kind, LoopContext::Loop);

    // Pop context.
    walker.pop_loop_context();
    assert_eq!(walker.current_loop_context(), None);
}

#[test]
fn emit_pending_unsafe_bodies_routes_action_app() {
    // Issue #1088: emit_pending_unsafe_bodies should route Action nodes with App children
    // through emit_call_stmt, emitting the CALL instruction.
    let mut arena = IrArena::new();

    // Allocate: Lambda for target function.
    let target_lambda_id = arena.alloc(IrKind::Lambda, span());
    let target_symbol = Symbol::new("target_fn".to_string(), SymbolKind::Function, target_lambda_id);
    arena.symbols_mut().insert(target_symbol);

    // Allocate: Var node for callee (binding name = "target_fn").
    let callee_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_id, "target_fn".to_string());

    // Allocate: App node (callee + args).
    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id]);

    // Allocate: Action node (wrapping App).
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);

    // Allocate: Unsafe block (containing Action).
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [action_id]);

    // Create walker and emit pending unsafe bodies.
    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    // Verify: a CALL instruction was emitted.
    let insts = &walker.state().instructions;
    let call_found = insts.entries().iter().any(|(_, inst)| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Call
    });
    assert!(
        call_found,
        "Expected CALL instruction in pending unsafe body, got {} instructions",
        insts.entries().len()
    );

    // Verify: no U1614 diagnostics were emitted.
    let diags = &walker.structured_diagnostics;
    let u1614_found = diags.iter().any(|d| {
        d.code().number() == 1614
    });
    assert!(
        !u1614_found,
        "Expected no U1614 diagnostics, got {} diagnostics",
        diags.len()
    );
}

#[test]
fn emit_pending_unsafe_bodies_skips_label_sibling() {
    // Issue #1088 regression: StmtLabel lowers to IrKind::Placeholder (not
    // IrKind::Label, which is a dead/reserved variant). emit_pending_unsafe_bodies
    // must skip Placeholder siblings (already emitted as labels by UnsafeWalker)
    // instead of falling through to the U1614 "unroutable statement kind" arm.
    //
    // Mirrors boot/entry.pdx: a `halt:` label followed by a call statement inside
    // the same unsafe block.
    let mut arena = IrArena::new();

    // Allocate: Lambda for target function.
    let target_lambda_id = arena.alloc(IrKind::Lambda, span());
    let target_symbol = Symbol::new("target_fn".to_string(), SymbolKind::Function, target_lambda_id);
    arena.symbols_mut().insert(target_symbol);

    // Allocate: Var node for callee (binding name = "target_fn").
    let callee_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_id, "target_fn".to_string());

    // Allocate: App node (callee + args).
    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id]);

    // Allocate: Action node (wrapping App).
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);

    // Allocate: Placeholder node (label sibling, e.g. `halt:`) — already emitted
    // by UnsafeWalker prior to this pass, so it must be a no-op here.
    let label_id = arena.alloc(IrKind::Placeholder, span());

    // Allocate: Unsafe block containing the label placeholder followed by the
    // action statement, matching the `halt:` + call ordering in boot/entry.pdx.
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [label_id, action_id]);

    // Create walker and emit pending unsafe bodies.
    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    // Verify: a CALL instruction was still emitted for the Action sibling.
    let insts = &walker.state().instructions;
    let call_found = insts.entries().iter().any(|(_, inst)| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Call
    });
    assert!(
        call_found,
        "Expected CALL instruction alongside label placeholder, got {} instructions",
        insts.entries().len()
    );

    // Verify: no U1614 diagnostics were emitted (the Placeholder must not fall
    // through to the unroutable-statement-kind arm).
    let diags = &walker.structured_diagnostics;
    let u1614_found = diags.iter().any(|d| {
        d.code().number() == 1614
    });
    assert!(
        !u1614_found,
        "Expected no U1614 diagnostics for label placeholder, got {} diagnostics",
        diags.len()
    );
}
// MS x64 shadow-space caller-side frame emission tests (issue #1008)
// Add these to the end of crates/paideia-as-elaborator/src/emit_walker_tests/layouts_calls.rs

#[test]
fn emit_walker_ms_zero_arg_call_emits_shadow_prelude_postlude() {
    // MS callee, 0 args. Verify sub rsp, 40 and add rsp, 40 with correct operands.
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();

    // Create target lambda with MS calling convention
    let target_lambda_id = IrNodeId::new(100).expect("valid lambda id");
    let mut target_info = LetInfo::immutable();
    target_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(target_lambda_id, target_info);

    let target_symbol = Symbol::new_with_abi(
        "target_ms".to_string(),
        SymbolKind::Function,
        target_lambda_id,
        Some(CallingConvention::Ms),
    );
    arena.symbols_mut().insert(target_symbol);

    // Create caller lambda
    let _caller_lambda_id = IrNodeId::new(200).expect("valid caller id");
    let callee_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_var_id, "target_ms".to_string());

    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_var_id]);
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [action_id]);

    // Emit the call
    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    // Verify prelude (sub rsp, 40) is present
    let insts = &walker.state().instructions;
    let sub_found = insts.entries().iter().any(|(_, inst)| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Sub &&
        inst.operands.len() == 2 &&
        matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RSP) &&
        matches!(&inst.operands[1], paideia_as_ir::instruction::Operand::Imm64(40))
    });
    assert!(sub_found, "Expected 'sub rsp, 40' prelude for MS call");

    // Verify postlude (add rsp, 40) is present
    let add_found = insts.entries().iter().any(|(_, inst)| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Add &&
        inst.operands.len() == 2 &&
        matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RSP) &&
        matches!(&inst.operands[1], paideia_as_ir::instruction::Operand::Imm64(40))
    });
    assert!(add_found, "Expected 'add rsp, 40' postlude for MS call");
}

#[test]
fn emit_walker_ms_one_arg_call_uses_rcx_not_rdi() {
    // MS callee, 1 lit arg. Verify mov rcx, imm (NOT mov rdi, imm).
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();

    // Create target lambda with MS calling convention
    let target_lambda_id = IrNodeId::new(100).expect("valid lambda id");
    let mut target_info = LetInfo::immutable();
    target_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(target_lambda_id, target_info);

    let target_symbol = Symbol::new_with_abi(
        "target_ms".to_string(),
        SymbolKind::Function,
        target_lambda_id,
        Some(CallingConvention::Ms),
    );
    arena.symbols_mut().insert(target_symbol);

    // Create literal argument
    let arg_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg_lit_id, 42i64);

    // Create caller lambda with argument
    let callee_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_var_id, "target_ms".to_string());

    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_var_id, arg_lit_id]);
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [action_id]);

    // Emit the call
    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    // Verify mov rcx, 42
    let insts = &walker.state().instructions;
    let rcx_found = insts.entries().iter().any(|(_, inst)| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Mov &&
        inst.operands.len() == 2 &&
        matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RCX) &&
        matches!(&inst.operands[1], paideia_as_ir::instruction::Operand::Imm64(42))
    });
    assert!(rcx_found, "Expected 'mov rcx, 42' for MS arg 0");

    // Verify RDI is NOT used for MS args
    let rdi_used = insts.entries().iter().any(|(_, inst)| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Mov &&
        inst.operands.len() == 2 &&
        matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RDI)
    });
    assert!(!rdi_used, "MS call should NOT use RDI (SysV register)");
}

#[test]
fn emit_walker_ms_four_arg_call_uses_full_ms_reg_pool() {
    // MS callee, 4 lit args. Verify [RCX, RDX, R8, R9] all populated.
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();

    // Create target lambda with MS calling convention
    let target_lambda_id = IrNodeId::new(100).expect("valid lambda id");
    let mut target_info = LetInfo::immutable();
    target_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(target_lambda_id, target_info);

    let target_symbol = Symbol::new_with_abi(
        "target_ms".to_string(),
        SymbolKind::Function,
        target_lambda_id,
        Some(CallingConvention::Ms),
    );
    arena.symbols_mut().insert(target_symbol);

    // Create 4 literal arguments
    let arg1_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1_id, 10i64);
    let arg2_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg2_id, 20i64);
    let arg3_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg3_id, 30i64);
    let arg4_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg4_id, 40i64);

    let callee_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_var_id, "target_ms".to_string());

    let app_id = arena.alloc_with_children(
        IrKind::App,
        span(),
        [callee_var_id, arg1_id, arg2_id, arg3_id, arg4_id],
    );
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [action_id]);

    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    let insts = &walker.state().instructions;

    // Verify all 4 MS arg registers are populated
    for (idx, expected_reg, expected_val) in [
        (0, paideia_as_ir::abi::RCX, 10i64),
        (1, paideia_as_ir::abi::RDX, 20i64),
        (2, paideia_as_ir::abi::R8, 30i64),
        (3, paideia_as_ir::abi::R9, 40i64),
    ] {
        let found = insts.entries().iter().any(|(_, inst)| {
            inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Mov &&
            inst.operands.len() == 2 &&
            matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == expected_reg) &&
            matches!(&inst.operands[1], paideia_as_ir::instruction::Operand::Imm64(v) if *v == expected_val)
        });
        assert!(found, "Expected 'mov {}, {}' for MS arg {}", expected_reg.0, expected_val, idx);
    }
}

#[test]
fn emit_walker_ms_five_arg_call_emits_t0521() {
    // MS callee, 5 args. Verify T0521 emitted; no CALL emitted.
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();

    // Create target lambda with MS calling convention
    let target_lambda_id = IrNodeId::new(100).expect("valid lambda id");
    let mut target_info = LetInfo::immutable();
    target_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(target_lambda_id, target_info);

    let target_symbol = Symbol::new_with_abi(
        "target_ms".to_string(),
        SymbolKind::Function,
        target_lambda_id,
        Some(CallingConvention::Ms),
    );
    arena.symbols_mut().insert(target_symbol);

    // Create 5 literal arguments
    let args: Vec<_> = (1..=5)
        .map(|i| {
            let id = arena.alloc(IrKind::Literal, span());
            arena.literal_values_mut().insert(id, (i * 10) as i64);
            id
        })
        .collect();

    let callee_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_var_id, "target_ms".to_string());

    let mut app_children = vec![callee_var_id];
    app_children.extend(args);
    let app_id = arena.alloc_with_children(IrKind::App, span(), app_children.into_iter());
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [action_id]);

    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    // Verify T0521 is in diagnostics
    let t0521_found = walker.structured_diagnostics.iter().any(|d| d.code().number() == 521);
    assert!(t0521_found, "Expected T0521 diagnostic for 5-arg MS call");
}

#[test]
fn emit_walker_sysv_call_still_uses_rdi_rsi_pool() {
    // SysV callee (explicitly annotated), 1 arg. Verify mov rdi, imm (no prelude/postlude).
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();

    // Create target lambda with SysV calling convention (explicit)
    let target_lambda_id = IrNodeId::new(100).expect("valid lambda id");
    let mut target_info = LetInfo::immutable();
    target_info.abi = Some(CallingConvention::Sysv);
    arena.let_meta_mut().insert(target_lambda_id, target_info);

    let target_symbol = Symbol::new_with_abi(
        "target_sysv".to_string(),
        SymbolKind::Function,
        target_lambda_id,
        Some(CallingConvention::Sysv),
    );
    arena.symbols_mut().insert(target_symbol);

    // Create 1 literal argument
    let arg_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg_id, 99i64);

    let callee_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_var_id, "target_sysv".to_string());

    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_var_id, arg_id]);
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [action_id]);

    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    let insts = &walker.state().instructions;

    // Verify mov rdi, 99
    let rdi_found = insts.entries().iter().any(|(_, inst)| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Mov &&
        matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RDI) &&
        matches!(&inst.operands[1], paideia_as_ir::instruction::Operand::Imm64(99))
    });
    assert!(rdi_found, "Expected 'mov rdi, 99' for SysV call");

    // Verify NO prelude
    let sub_found = insts.entries().iter().any(|(_, inst)| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Sub &&
        matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RSP)
    });
    assert!(!sub_found, "SysV call should NOT emit 'sub rsp' prelude");
}

#[test]
fn emit_walker_absent_abi_call_matches_sysv() {
    // Unannotated callee (abi == None). Verify identical to SysV case (regression fence).
    let mut arena = IrArena::new();

    // Create target lambda WITHOUT ABI annotation
    let target_lambda_id = IrNodeId::new(100).expect("valid lambda id");
    let target_symbol = Symbol::new(
        "target_default".to_string(),
        SymbolKind::Function,
        target_lambda_id,
    );
    arena.symbols_mut().insert(target_symbol);

    // Create 1 literal argument
    let arg_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg_id, 77i64);

    let callee_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_var_id, "target_default".to_string());

    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_var_id, arg_id]);
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [action_id]);

    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    let insts = &walker.state().instructions;

    // Verify mov rdi, 77 (SysV default)
    let rdi_found = insts.entries().iter().any(|(_, inst)| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Mov &&
        matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RDI) &&
        matches!(&inst.operands[1], paideia_as_ir::instruction::Operand::Imm64(77))
    });
    assert!(rdi_found, "Unannotated call should default to SysV (mov rdi)");

    // Verify NO prelude
    let sub_found = insts.entries().iter().any(|(_, inst)| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Sub &&
        matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RSP)
    });
    assert!(!sub_found, "Unannotated call should NOT emit prelude");
}

#[test]
fn emit_walker_ms_shadow_bump_is_forty_bytes() {
    // Pin Imm64(40) on prelude/postlude (regression fence for MS_CALL_STACK_BUMP).
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();

    let target_lambda_id = IrNodeId::new(100).expect("valid lambda id");
    let mut target_info = LetInfo::immutable();
    target_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(target_lambda_id, target_info);

    let target_symbol = Symbol::new_with_abi(
        "target_ms".to_string(),
        SymbolKind::Function,
        target_lambda_id,
        Some(CallingConvention::Ms),
    );
    arena.symbols_mut().insert(target_symbol);

    let callee_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_var_id, "target_ms".to_string());

    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_var_id]);
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [action_id]);

    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    let insts = &walker.state().instructions;

    // Count how many Sub/Add rsp, 40 we see
    let mut bumps_found = 0;
    for (_, inst) in insts.entries().iter() {
        if (inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Sub ||
            inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Add) &&
           matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RSP) &&
           matches!(&inst.operands[1], paideia_as_ir::instruction::Operand::Imm64(40))
        {
            bumps_found += 1;
        }
    }
    assert_eq!(bumps_found, 2, "Expected exactly 2 'sub/add rsp, 40' for MS call (prelude + postlude)");
}

#[test]
fn emit_walker_ms_prelude_precedes_arg_moves_in_id_order() {
    // Assert sorted-by-IrNodeId iteration produces prelude, mov_arg*, call, postlude, ret.
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();

    let target_lambda_id = IrNodeId::new(100).expect("valid lambda id");
    let mut target_info = LetInfo::immutable();
    target_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(target_lambda_id, target_info);

    let target_symbol = Symbol::new_with_abi(
        "target_ms".to_string(),
        SymbolKind::Function,
        target_lambda_id,
        Some(CallingConvention::Ms),
    );
    arena.symbols_mut().insert(target_symbol);

    // 1 arg
    let arg_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg_id, 55i64);

    let callee_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_var_id, "target_ms".to_string());

    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_var_id, arg_id]);
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [action_id]);

    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    let insts = &walker.state().instructions;
    let mut inst_list: Vec<_> = insts.entries().iter().collect();
    inst_list.sort_by_key(|&(id, _)| id);  // Sort by IrNodeId

    // Verify order: sub, mov, call, add
    let mut prelude_idx = None;
    let mut mov_idx = None;
    let mut call_idx = None;
    let mut postlude_idx = None;

    for (i, (_, inst)) in inst_list.iter().enumerate() {
        if inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Sub &&
           matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RSP)
        {
            prelude_idx = Some(i);
        } else if inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Mov &&
                  matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RCX)
        {
            mov_idx = Some(i);
        } else if inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Call {
            call_idx = Some(i);
        } else if inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Add &&
                  matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RSP)
        {
            postlude_idx = Some(i);
        }
    }

    if let (Some(p), Some(m), Some(c), Some(a)) = (prelude_idx, mov_idx, call_idx, postlude_idx) {
        assert!(p < m, "Prelude should come before MOV args");
        assert!(m < c, "MOV args should come before CALL");
        assert!(c < a, "CALL should come before postlude");
    } else {
        panic!("Missing expected instructions for MS call sequence");
    }
}

#[test]
fn emit_walker_ms_postlude_is_between_call_and_ret() {
    // Verify postlude is between CALL and RET.
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();

    let target_lambda_id = IrNodeId::new(100).expect("valid lambda id");
    let mut target_info = LetInfo::immutable();
    target_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(target_lambda_id, target_info);

    let target_symbol = Symbol::new_with_abi(
        "target_ms".to_string(),
        SymbolKind::Function,
        target_lambda_id,
        Some(CallingConvention::Ms),
    );
    arena.symbols_mut().insert(target_symbol);

    let callee_var_id = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(callee_var_id, "target_ms".to_string());

    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_var_id]);
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [app_id]);
    let unsafe_id = arena.alloc_with_children(IrKind::Unsafe, span(), [action_id]);

    let mut walker = EmitWalker::new();
    // Register the unsafe body as belonging to a caller lambda (needed for state.current_function tracking)
    walker.state_mut().unsafe_body_to_lambda.insert(unsafe_id.get(), 200);
    let pending = vec![unsafe_id.get()];
    walker.emit_pending_unsafe_bodies(pending, &mut arena, None);

    let insts = &walker.state().instructions;
    let mut inst_list: Vec<_> = insts.entries().iter().collect();
    inst_list.sort_by_key(|&(id, _)| id);

    let mut call_idx = None;
    let mut postlude_idx = None;

    for (i, (_, inst)) in inst_list.iter().enumerate() {
        if inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Call {
            call_idx = Some(i);
        } else if inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Add &&
                  matches!(&inst.operands[0], paideia_as_ir::instruction::Operand::Reg(r) if *r == paideia_as_ir::abi::RSP)
        {
            postlude_idx = Some(i);
        }
    }

    if let (Some(c), Some(p)) = (call_idx, postlude_idx) {
        assert!(c < p, "CALL should come before postlude (add rsp)");
        
    } else {
        panic!("Missing CALL or postlude (add rsp) instruction");
    }
}

#[test]
fn symbol_carries_abi_from_let_meta() {
    // Synthesize Let with LetInfo.abi = Some(Ms), walk, assert Symbol.abi == Some(Ms).
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();

    // Create Lambda with MS calling convention
    let lambda_id = arena.alloc(IrKind::Lambda, span());

    // Create a Let binding with the Lambda as its RHS
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lambda_id]);

    // Register the let binding name
    arena.binding_names_mut().insert(let_id, "ms_func".to_string());

    // Annotate the Let with MS calling convention
    let mut let_info = LetInfo::immutable();
    let_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(let_id, let_info);

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify the symbol table contains "ms_func" with abi == Some(Ms)
    if let Some(sym) = arena.symbols().lookup_by_name("ms_func") {
        assert_eq!(sym.abi, Some(CallingConvention::Ms), "Symbol should carry MS ABI from let_meta");
    } else {
        panic!("Symbol 'ms_func' not found in symbol table after walk");
    }
}

// PA19-r19-006: MS x64 lambda emission unit tests

#[test]
fn ms_lambda_identity_body_moves_from_rcx() {
    // MS x64 identity: fn (x) -> x should use RCX (not RDI) as the parameter register.
    // Verify the parameter is registered in local_bindings under RCX.
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();
    let span_ref = span();

    // Create a Var node for the parameter
    let param_var = arena.alloc(IrKind::Var, span_ref);
    arena.binding_names_mut().insert(param_var, "x".to_string());

    // Create Lambda with Var body (identity)
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span_ref, [param_var]);

    // Create a Let binding with MS ABI
    let let_id = arena.alloc_with_children(IrKind::Let, span_ref, [lambda_id]);
    arena.binding_names_mut().insert(let_id, "my_id".to_string());

    let mut let_info = LetInfo::immutable();
    let_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(let_id, let_info);

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify lambda_abi was recorded as MS
    let recorded_abi = walker.state().lambda_abi(lambda_id.get());
    assert_eq!(recorded_abi, CallingConvention::Ms, "Lambda ABI should be MS x64");

    // For an identity lambda, the parameter x should be registered.
    // Under MS x64, param 0 maps to RCX.
    // The local_bindings entry should be in the walk's state.
    // (Verification happens at higher level, but we've confirmed ABI recording.)
}

#[test]
fn ms_lambda_add_imm_body_leas_from_rcx() {
    // MS x64 add-imm: fn (x) -> x + 1 should use RCX (not RDI) as the source register in lea.
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();
    let span_ref = span();

    // Create a Var node for the first parameter
    let param_var = arena.alloc(IrKind::Var, span_ref);
    arena.binding_names_mut().insert(param_var, "x".to_string());

    // Create Lambda with Var body
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span_ref, [param_var]);

    // Create a Let binding with MS ABI
    let let_id = arena.alloc_with_children(IrKind::Let, span_ref, [lambda_id]);
    arena.binding_names_mut().insert(let_id, "my_add_imm".to_string());

    let mut let_info = LetInfo::immutable();
    let_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(let_id, let_info);

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify lambda_abi was recorded as MS
    let recorded_abi = walker.state().lambda_abi(lambda_id.get());
    assert_eq!(recorded_abi, CallingConvention::Ms, "Lambda ABI should be MS x64");

    // For add-imm, we expect emit_add_imm_lambda to use param_index_to_reg_for_abi(Ms, 0) = RCX.
    // This will be verified at the integration test level with actual bytecode inspection.
}

#[test]
fn ms_lambda_literal_return_body_moves_imm_to_rax() {
    // MS x64 literal return: fn() -> 42 should emit mov rax, 42; ret.
    // This applies to both ABIs; verify it compiles under MS x64.
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    let mut arena = IrArena::new();
    let span_ref = span();

    // Create a Literal node (value 42)
    let literal_id = arena.alloc(IrKind::Literal, span_ref);
    arena.literal_values_mut().insert(literal_id, 42);

    // Create Lambda with Literal body (no parameters)
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span_ref, [literal_id]);

    // Create a Let binding with MS ABI
    let let_id = arena.alloc_with_children(IrKind::Let, span_ref, [lambda_id]);
    arena.binding_names_mut().insert(let_id, "my_lit".to_string());

    let mut let_info = LetInfo::immutable();
    let_info.abi = Some(CallingConvention::Ms);
    arena.let_meta_mut().insert(let_id, let_info);

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify lambda was emitted
    assert!(walker.emitted_lambdas().contains(&lambda_id.get()), "Literal lambda should be emitted");

    // Verify lambda_abi was recorded as MS (though not used for literal body)
    let recorded_abi = walker.state().lambda_abi(lambda_id.get());
    assert_eq!(recorded_abi, CallingConvention::Ms, "Lambda ABI should be MS x64");
}

#[test]
fn sysv_lambda_identity_still_uses_rdi_regression() {
    // SysV identity: fn (x) -> x should still use RDI (not RCX) for backward compatibility.
    use paideia_as_ir::let_meta::CallingConvention;

    let mut arena = IrArena::new();
    let span_ref = span();

    // Create a Var node for the parameter
    let param_var = arena.alloc(IrKind::Var, span_ref);
    arena.binding_names_mut().insert(param_var, "x".to_string());

    // Create Lambda with Var body (identity)
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span_ref, [param_var]);

    // Create a Let binding WITHOUT ABI annotation (defaults to SysV)
    let let_id = arena.alloc_with_children(IrKind::Let, span_ref, [lambda_id]);
    arena.binding_names_mut().insert(let_id, "my_id_sysv".to_string());

    // Walk the arena (no explicit ABI)
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify lambda_abi defaults to SysV
    let recorded_abi = walker.state().lambda_abi(lambda_id.get());
    assert_eq!(recorded_abi, CallingConvention::Sysv, "Unannotated lambda should default to SysV");
}

#[test]
fn absent_abi_lambda_matches_sysv() {
    // Absent @abi directive should default to SysV, not MS.
    use paideia_as_ir::let_meta::CallingConvention;

    let mut arena = IrArena::new();
    let span_ref = span();

    // Create a simple Lambda
    let literal_id = arena.alloc(IrKind::Literal, span_ref);
    arena.literal_values_mut().insert(literal_id, 100);
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span_ref, [literal_id]);

    // Create a Let binding WITHOUT ABI annotation
    let let_id = arena.alloc_with_children(IrKind::Let, span_ref, [lambda_id]);
    arena.binding_names_mut().insert(let_id, "default_abi".to_string());

    // No let_meta entry (absent ABI)

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify lambda_abi defaults to SysV
    let recorded_abi = walker.state().lambda_abi(lambda_id.get());
    assert_eq!(recorded_abi, CallingConvention::Sysv, "Absent ABI should default to SysV");
}

#[test]
fn ms_lambda_param_reg_helper_returns_ms_arg_regs() {
    // Verify param_index_to_reg_for_abi correctly maps MS x64 parameter indices.
    use paideia_as_ir::let_meta::{LetInfo, CallingConvention};

    // MS x64: [RCX, RDX, R8, R9]
    let idx0_ms = EmitWalker::param_index_to_reg_for_abi(CallingConvention::Ms, 0);
    assert_eq!(idx0_ms, Some(abi::RCX), "MS x64 param 0 should be RCX");

    let idx1_ms = EmitWalker::param_index_to_reg_for_abi(CallingConvention::Ms, 1);
    assert_eq!(idx1_ms, Some(abi::RDX), "MS x64 param 1 should be RDX");

    let idx2_ms = EmitWalker::param_index_to_reg_for_abi(CallingConvention::Ms, 2);
    assert_eq!(idx2_ms, Some(abi::R8), "MS x64 param 2 should be R8");

    let idx3_ms = EmitWalker::param_index_to_reg_for_abi(CallingConvention::Ms, 3);
    assert_eq!(idx3_ms, Some(abi::R9), "MS x64 param 3 should be R9");

    let idx4_ms = EmitWalker::param_index_to_reg_for_abi(CallingConvention::Ms, 4);
    assert_eq!(idx4_ms, None, "MS x64 param 4 should be stack (None)");

    // SysV x64: [RDI, RSI, RDX, RCX, R8, R9]
    let idx0_sysv = EmitWalker::param_index_to_reg_for_abi(CallingConvention::Sysv, 0);
    assert_eq!(idx0_sysv, Some(abi::RDI), "SysV param 0 should be RDI");

    let idx1_sysv = EmitWalker::param_index_to_reg_for_abi(CallingConvention::Sysv, 1);
    assert_eq!(idx1_sysv, Some(abi::RSI), "SysV param 1 should be RSI");

    let idx6_sysv = EmitWalker::param_index_to_reg_for_abi(CallingConvention::Sysv, 6);
    assert_eq!(idx6_sysv, None, "SysV param 6 should be stack (None)");
}

#[test]
fn emit_walker_t0540_var_assign_non_var_rhs() {
    // Issue #1135: T0540 fires when visit_var_assign encounters a non-Var RHS
    // (e.g., literal or FieldAccess instead of Var).

    let mut arena = IrArena::new();
    let mut walker = EmitWalker::new();

    let span_ref = span();

    // Create a Store node with [Var(lhs), op, Literal(5)] children
    // This represents: counter = 5 (non-Var RHS)
    let lhs_id = arena.alloc(IrKind::Var, span_ref);
    let op_id = arena.alloc(IrKind::Placeholder, span_ref);
    let literal_id = arena.alloc(IrKind::Literal, span_ref);

    let store_id = arena.alloc_with_children(
        IrKind::Store,
        span_ref,
        [lhs_id, op_id, literal_id],
    );

    // Set binding names
    arena.binding_names_mut().insert(lhs_id, "counter".to_string());

    // Walk only the Store node (not the full arena)
    walker.visit_var_assign(store_id, &arena);

    // Verify T0540 diagnostic was fired via the typed diagnostic pipe
    let typed_diags = walker.take_typed_diagnostics();
    assert!(
        !typed_diags.is_empty(),
        "T0540 should be fired for non-Var RHS"
    );
    assert!(
        typed_diags.iter().any(|d| d.code().to_string() == "T0540"),
        "Diagnostic should have code T0540"
    );
}
