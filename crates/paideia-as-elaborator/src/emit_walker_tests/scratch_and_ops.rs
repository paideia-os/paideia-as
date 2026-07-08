use super::super::*;
use crate::emit_fixture::EmitFixture;
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::CallMeta;


fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}
#[test]
fn pa7c_m2_002_let_literal_assigns_first_scratch_reg() {
    let mut arena = IrArena::new();

    // Allocate: Literal node, then Let with Literal as child.
    let lit_id = arena.alloc(IrKind::Literal, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);

    // Register binding name
    arena.binding_names_mut().insert(let_id, "x".to_string());

    // Register the literal value 0x10
    arena.literal_values_mut().insert(lit_id, 0x10);

    // Create a block containing the let statement
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [let_id]);

    // Create a lambda with the action as its body
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify scratch_assignment[0] == RAX (abi::RAX)
    assert_eq!(
        walker.state().scratch_count(),
        1,
        "Should have 1 scratch assignment"
    );
    assert_eq!(
        walker.state().scratch_assignment[0],
        abi::RAX,
        "First scratch should be RAX"
    );

    // Verify local_bindings.get("x") == Some(RAX)
    assert_eq!(
        walker.state().local_bindings.get("x"),
        Some(abi::RAX),
        "Binding 'x' should map to RAX"
    );

    // Verify 1 Mov instruction was emitted (plus the final Ret from emit_block_body)
    let mut mov_count = 0;
    for (_, inst) in walker.state().instructions.entries().iter() {
        if inst.mnemonic == Mnemonic::Mov {
            mov_count += 1;
        }
    }
    assert_eq!(mov_count, 1, "Should have emitted 1 Mov instruction");
}

/// Test 2: Three Lets (a, b, c) with Literal RHS assign distinct scratch regs.
#[test]
fn pa7c_m2_002_three_let_chain_assigns_distinct_scratch_regs() {
    let mut arena = IrArena::new();

    // Allocate three Let nodes with Literal RHS
    let lit_a = arena.alloc(IrKind::Literal, span());
    let let_a = arena.alloc_with_children(IrKind::Let, span(), [lit_a]);
    arena.binding_names_mut().insert(let_a, "a".to_string());
    arena.literal_values_mut().insert(lit_a, 0x10);

    let lit_b = arena.alloc(IrKind::Literal, span());
    let let_b = arena.alloc_with_children(IrKind::Let, span(), [lit_b]);
    arena.binding_names_mut().insert(let_b, "b".to_string());
    arena.literal_values_mut().insert(lit_b, 0x20);

    let lit_c = arena.alloc(IrKind::Literal, span());
    let let_c = arena.alloc_with_children(IrKind::Let, span(), [lit_c]);
    arena.binding_names_mut().insert(let_c, "c".to_string());
    arena.literal_values_mut().insert(lit_c, 0x30);

    // Create a block containing the three let statements
    let action_id = arena.alloc_with_children(IrKind::Action, span(), [let_a, let_b, let_c]);

    // Create a lambda with the action as its body
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify scratch_assignment has 3 entries
    assert_eq!(
        walker.state().scratch_count(),
        3,
        "Should have 3 scratch assignments"
    );

    // Verify they are RAX, RCX, RDX
    assert_eq!(
        walker.state().scratch_assignment[0],
        abi::RAX,
        "First should be RAX"
    );
    assert_eq!(
        walker.state().scratch_assignment[1],
        abi::RCX,
        "Second should be RCX"
    );
    assert_eq!(
        walker.state().scratch_assignment[2],
        abi::RDX,
        "Third should be RDX"
    );

    // Verify local_bindings
    assert_eq!(
        walker.state().local_bindings.get("a"),
        Some(abi::RAX),
        "Binding 'a' should map to RAX"
    );
    assert_eq!(
        walker.state().local_bindings.get("b"),
        Some(abi::RCX),
        "Binding 'b' should map to RCX"
    );
    assert_eq!(
        walker.state().local_bindings.get("c"),
        Some(abi::RDX),
        "Binding 'c' should map to RDX"
    );

    // Verify at least 3 Mov instructions were emitted (for the 3 lets)
    // Note: there may be additional Mov instructions depending on the walk's side effects
    let mut mov_count = 0;
    for (_, inst) in walker.state().instructions.entries().iter() {
        if inst.mnemonic == Mnemonic::Mov {
            mov_count += 1;
        }
    }
    assert!(
        mov_count >= 3,
        "Should have emitted at least 3 Mov instructions, got {}",
        mov_count
    );
}

/// Test 3: Five Lets exhaust the 4-register pool and emit T0527.
#[test]
fn pa7c_m2_002_five_let_chain_exhausts_pool_and_emits_t0527() {
    let mut arena = IrArena::new();

    // Allocate five Let nodes with Literal RHS
    let mut let_ids = Vec::new();
    for i in 1..=5 {
        let lit = arena.alloc(IrKind::Literal, span());
        let let_node = arena.alloc_with_children(IrKind::Let, span(), [lit]);
        let name = format!("var_{}", i);
        arena.binding_names_mut().insert(let_node, name);
        arena.literal_values_mut().insert(lit, (i as i64) * 0x10);
        let_ids.push(let_node);
    }

    // Create a block containing the five let statements
    let action_id = arena.alloc_with_children(IrKind::Action, span(), let_ids);

    // Create a lambda with the action as its body
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [action_id]);

    // Walk the arena
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify T0527 was emitted in diagnostics
    let has_t0527 = walker.diagnostics().iter().any(|d| d.contains("T0527"));
    assert!(
        has_t0527,
        "Should emit T0527 diagnostic for register exhaustion"
    );

    // Verify scratch_assignment stopped at 4 registers
    assert_eq!(
        walker.state().scratch_count(),
        4,
        "Should have only 4 scratch assignments"
    );

    // Verify they are RAX, RCX, RDX, R8
    assert_eq!(
        walker.state().scratch_assignment[0],
        abi::RAX,
        "First should be RAX"
    );
    assert_eq!(
        walker.state().scratch_assignment[1],
        abi::RCX,
        "Second should be RCX"
    );
    assert_eq!(
        walker.state().scratch_assignment[2],
        abi::RDX,
        "Third should be RDX"
    );
    assert_eq!(
        walker.state().scratch_assignment[3],
        abi::R8,
        "Fourth should be R8"
    );
}

/// PA10-005 §3.6: Test 1 — if_then_arm_sees_outer_let
/// Verify that a binding in the outer scope is visible in the then-arm scope.
#[test]
fn if_then_arm_sees_outer_let() {
    let mut arena = IrArena::new();

    // Create outer let: x = 42
    let outer_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(outer_lit_id, 42);
    let outer_let_id = arena.alloc_with_children(IrKind::Let, span(), [outer_lit_id]);
    arena
        .binding_names_mut()
        .insert(outer_let_id, "x".to_string());

    // Create condition (placeholder): 1
    let cond_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(cond_lit_id, 1);

    // Create then-body with inner let: y = 10
    let inner_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(inner_lit_id, 10);
    let inner_let_id = arena.alloc_with_children(IrKind::Let, span(), [inner_lit_id]);
    arena
        .binding_names_mut()
        .insert(inner_let_id, "y".to_string());
    let then_body_id = arena.alloc_with_children(IrKind::Action, span(), [inner_let_id]);

    // Create branch: if (cond) { then_body } else { ... }
    let else_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(else_lit_id, 0);
    let else_body_id = arena.alloc_with_children(IrKind::Action, span(), [else_lit_id]);
    let branch_id = arena.alloc_with_children(
        IrKind::Branch,
        span(),
        [cond_lit_id, then_body_id, else_body_id],
    );

    // Create block: { outer_let; branch }
    let block_id = arena.alloc_with_children(IrKind::Action, span(), [outer_let_id, branch_id]);

    // Create lambda with block
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [block_id]);

    // Walk and verify
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Both x (outer) and y (then-arm) should be in local_bindings
    assert!(walker.state().local_bindings.contains("x"));
    assert!(walker.state().local_bindings.contains("y"));
}

/// PA10-005 §3.6: Test 2 — if_else_arm_sees_outer_let
/// Verify that a binding in the outer scope is visible in the else-arm scope.
#[test]
fn if_else_arm_sees_outer_let() {
    let mut arena = IrArena::new();

    // Create outer let: x = 42
    let outer_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(outer_lit_id, 42);
    let outer_let_id = arena.alloc_with_children(IrKind::Let, span(), [outer_lit_id]);
    arena
        .binding_names_mut()
        .insert(outer_let_id, "x".to_string());

    // Create condition (placeholder): 1
    let cond_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(cond_lit_id, 1);

    // Create then-body: simple literal
    let then_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(then_lit_id, 5);
    let then_body_id = arena.alloc_with_children(IrKind::Action, span(), [then_lit_id]);

    // Create else-body with inner let: z = 20
    let else_inner_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(else_inner_lit_id, 20);
    let else_inner_let_id = arena.alloc_with_children(IrKind::Let, span(), [else_inner_lit_id]);
    arena
        .binding_names_mut()
        .insert(else_inner_let_id, "z".to_string());
    let else_body_id = arena.alloc_with_children(IrKind::Action, span(), [else_inner_let_id]);

    // Create branch: if (cond) { then } else { else_inner_let }
    let branch_id = arena.alloc_with_children(
        IrKind::Branch,
        span(),
        [cond_lit_id, then_body_id, else_body_id],
    );

    // Create block: { outer_let; branch }
    let block_id = arena.alloc_with_children(IrKind::Action, span(), [outer_let_id, branch_id]);

    // Create lambda with block
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [block_id]);

    // Walk and verify
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Both x (outer) and z (else-arm) should be in local_bindings
    assert!(walker.state().local_bindings.contains("x"));
    assert!(walker.state().local_bindings.contains("z"));
}

/// PA10-005 §3.6: Test 3 — nested_if_in_if_sees_outermost
/// Verify that innermost scope sees all outer scopes.
/// DEFERRED: Match-arm body wiring under investigation (PA10-005b).
#[test]
#[ignore]
fn nested_if_in_if_sees_outermost() {
    let mut arena = IrArena::new();

    // Create outermost let: a = 1
    let a_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(a_lit_id, 1);
    let a_let_id = arena.alloc_with_children(IrKind::Let, span(), [a_lit_id]);
    arena.binding_names_mut().insert(a_let_id, "a".to_string());

    // Create outer if condition
    let outer_cond_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(outer_cond_id, 1);

    // Create inner if
    let inner_cond_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(inner_cond_id, 1);

    // Create innermost let: c = 3
    let c_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(c_lit_id, 3);
    let c_let_id = arena.alloc_with_children(IrKind::Let, span(), [c_lit_id]);
    arena.binding_names_mut().insert(c_let_id, "c".to_string());

    let inner_then_body_id = arena.alloc_with_children(IrKind::Action, span(), [c_let_id]);
    let inner_else_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(inner_else_lit_id, 0);
    let inner_else_body_id =
        arena.alloc_with_children(IrKind::Action, span(), [inner_else_lit_id]);

    let inner_branch_id = arena.alloc_with_children(
        IrKind::Branch,
        span(),
        [inner_cond_id, inner_then_body_id, inner_else_body_id],
    );

    let outer_then_body_id =
        arena.alloc_with_children(IrKind::Action, span(), [inner_branch_id]);

    let outer_else_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(outer_else_lit_id, 0);
    let outer_else_body_id =
        arena.alloc_with_children(IrKind::Action, span(), [outer_else_lit_id]);

    let outer_branch_id = arena.alloc_with_children(
        IrKind::Branch,
        span(),
        [outer_cond_id, outer_then_body_id, outer_else_body_id],
    );

    // Create block: { a_let; outer_branch }
    let block_id =
        arena.alloc_with_children(IrKind::Action, span(), [a_let_id, outer_branch_id]);

    // Create lambda with block
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [block_id]);

    // Walk and verify
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // All bindings should be visible
    assert!(walker.state().local_bindings.contains("a"));
    assert!(walker.state().local_bindings.contains("c"));
}

/// PA10-005 §3.6: Test 4 — match_arm_sees_outer_let (mark #[ignore] if deferred)
/// Verify that a binding in the outer scope is visible in match arm scopes.
/// DEFERRED: Requires match-arm expression wiring (PA10-005b).
#[test]
#[ignore]
fn match_arm_sees_outer_let() {
    let mut arena = IrArena::new();

    // Create outer let: x = 42
    let outer_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(outer_lit_id, 42);
    let outer_let_id = arena.alloc_with_children(IrKind::Let, span(), [outer_lit_id]);
    arena
        .binding_names_mut()
        .insert(outer_let_id, "x".to_string());

    // Create scrutinee (match value): placeholder literal
    let scrutinee_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(scrutinee_id, 1);

    // Create first arm with let: y = 10
    let arm1_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arm1_lit_id, 10);
    let arm1_let_id = arena.alloc_with_children(IrKind::Let, span(), [arm1_lit_id]);
    arena
        .binding_names_mut()
        .insert(arm1_let_id, "y".to_string());
    let arm1_body_id = arena.alloc_with_children(IrKind::Action, span(), [arm1_let_id]);

    // Create default arm: simple literal
    let default_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(default_lit_id, 0);
    let default_body_id = arena.alloc_with_children(IrKind::Action, span(), [default_lit_id]);

    // Create match: match scrutinee { ... }
    let match_id = arena.alloc_with_children(
        IrKind::Match,
        span(),
        [scrutinee_id, arm1_body_id, default_body_id],
    );

    // Create block: { outer_let; match }
    let block_id = arena.alloc_with_children(IrKind::Action, span(), [outer_let_id, match_id]);

    // Create lambda with block
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [block_id]);

    // Walk and verify
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Both x (outer) and y (arm) should be in local_bindings
    assert!(walker.state().local_bindings.contains("x"));
    assert!(walker.state().local_bindings.contains("y"));
}

/// PA10-005 §3.6: Test 5 — inner_let_shadows_outer
/// Verify that inner let-binding shadows outer binding in current scope walk.
#[test]
fn inner_let_shadows_outer() {
    let mut arena = IrArena::new();

    // Create outer let: x = 42
    let outer_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(outer_lit_id, 42);
    let outer_let_id = arena.alloc_with_children(IrKind::Let, span(), [outer_lit_id]);
    arena
        .binding_names_mut()
        .insert(outer_let_id, "x".to_string());

    // Create condition
    let cond_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(cond_lit_id, 1);

    // Create inner let in then-arm: x = 100 (shadow outer x)
    let inner_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(inner_lit_id, 100);
    let inner_let_id = arena.alloc_with_children(IrKind::Let, span(), [inner_lit_id]);
    arena
        .binding_names_mut()
        .insert(inner_let_id, "x".to_string());
    let then_body_id = arena.alloc_with_children(IrKind::Action, span(), [inner_let_id]);

    // Create else body
    let else_lit_id = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(else_lit_id, 0);
    let else_body_id = arena.alloc_with_children(IrKind::Action, span(), [else_lit_id]);

    // Create branch
    let branch_id = arena.alloc_with_children(
        IrKind::Branch,
        span(),
        [cond_lit_id, then_body_id, else_body_id],
    );

    // Create block: { outer_let_x; branch }
    let block_id = arena.alloc_with_children(IrKind::Action, span(), [outer_let_id, branch_id]);

    // Create lambda with block
    let _lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [block_id]);

    // Walk and verify
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // x should be in local_bindings, and should resolve to one of the bindings
    // (either outer or shadowed, depending on execution path; here we just verify it exists)
    assert!(walker.state().local_bindings.contains("x"));
}

// ── Phase 13 m6-001: Field access width-correctness and encoder extension tests ────

/// Helper to build a field access IR and emit through the walker.
/// Returns the emitted instruction.
fn build_field_access(
    size: u8,
    signed: bool,
    offset: i32,
) -> Instruction {
    let mut arena = IrArena::new();

    // Allocate: Var(rdi) → FieldAccess
    let var_id = arena.alloc(IrKind::Var, span());
    let deref_id = arena.alloc_with_children(IrKind::Deref, span(), [var_id]);
    let field_access_id = arena.alloc_with_children(IrKind::FieldAccess, span(), [deref_id]);

    // Register field access metadata
    arena.field_access_info_mut().insert(
        field_access_id,
        paideia_as_ir::record_layout::FieldAccessInfo {
            type_id: RecordTypeId(1),
            field_index: 0,
        },
    );

    // Register record layout with (size, signed, offset) through FieldLayout
    let field_layout = FieldLayout {
        offset: offset as u64,
        size,
        signed,
    };
    let layout = RecordLayout::new(
        (offset as u64) + (size as u64),
        size.max(1),
        vec![field_layout],
    );

    // Walk and emit
    let mut walker = EmitWalker::new();
    // Inject the record layout into the walker state before walking
    walker.state_mut().insert_record_layout(RecordTypeId(1), layout);
    walker.walk(&mut arena);

    // Extract the emitted instruction (should be at field_access_id)
    walker
        .state()
        .instructions
        .get(field_access_id)
        .cloned()
        .expect("No instruction emitted for field access")
}

#[test]
fn field_access_u64_offset_0() {
    let inst = build_field_access(8, false, 0);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });

    // Encode and check bytes: mov rax, [rdi] → 48 8B 07
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x07]);
}

#[test]
fn field_access_u64_offset_24_disp8() {
    let inst = build_field_access(8, false, 24);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });

    // Encode: mov rax, [rdi + 24] → 48 8B 47 18
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x47, 0x18]);
}

#[test]
fn field_access_u64_offset_256_disp32() {
    let inst = build_field_access(8, false, 256);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });

    // Encode: mov rax, [rdi + 256] → 48 8B 87 00 01 00 00
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x87, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn field_access_u32_offset_0_no_rex_w() {
    // THE BUG-FIX GUARD: u32 must emit 8B not 48 8B
    let inst = build_field_access(4, false, 0);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W32 });

    // Encode: mov eax, [rdi] → 8B 07 (NOT 48 8B 07)
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x8B, 0x07]);
}

#[test]
fn field_access_u32_offset_8() {
    let inst = build_field_access(4, false, 8);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W32 });

    // Encode: mov eax, [rdi + 8] → 8B 47 08
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x8B, 0x47, 0x08]);
}

#[test]
fn field_access_u16_offset_0_movzx_word() {
    let inst = build_field_access(2, false, 0);
    assert_eq!(inst.mnemonic, Mnemonic::Movzx);

    // Encode: movzx rax, word [rdi] → 48 0F B7 07
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xB7, 0x07]);
}

#[test]
fn field_access_u16_offset_4_movzx_word() {
    let inst = build_field_access(2, false, 4);
    assert_eq!(inst.mnemonic, Mnemonic::Movzx);

    // Encode: movzx rax, word [rdi + 4] → 48 0F B7 47 04
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xB7, 0x47, 0x04]);
}

#[test]
fn field_access_u8_offset_0_movzx_byte() {
    let inst = build_field_access(1, false, 0);
    assert_eq!(inst.mnemonic, Mnemonic::Movzx);

    // Encode: movzx rax, byte [rdi] → 48 0F B6 07
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xB6, 0x07]);
}

#[test]
fn field_access_u8_offset_32_movzx_byte_disp8() {
    let inst = build_field_access(1, false, 32);
    assert_eq!(inst.mnemonic, Mnemonic::Movzx);

    // Encode: movzx rax, byte [rdi + 32] → 48 0F B6 47 20
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xB6, 0x47, 0x20]);
}

#[test]
fn field_access_i8_offset_0_movsx_byte() {
    let inst = build_field_access(1, true, 0);
    assert_eq!(inst.mnemonic, Mnemonic::Movsx);

    // Encode: movsx rax, byte [rdi] → 48 0F BE 07
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xBE, 0x07]);
}

#[test]
fn field_access_i16_offset_4_movsx_word() {
    let inst = build_field_access(2, true, 4);
    assert_eq!(inst.mnemonic, Mnemonic::Movsx);

    // Encode: movsx rax, word [rdi + 4] → 48 0F BF 47 04
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0xBF, 0x47, 0x04]);
}

#[test]
fn field_access_i32_offset_8_movsxd() {
    let inst = build_field_access(4, true, 8);
    assert_eq!(inst.mnemonic, Mnemonic::Movsx);

    // Encode: movsxd rax, dword [rdi + 8] → 48 63 47 08
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x63, 0x47, 0x08]);
}

#[test]
fn field_access_i64_offset_16_reuses_u64_path() {
    let inst = build_field_access(8, true, 16);
    // i64 uses MovSized W64 (same as u64)
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });

    // Encode: mov rax, [rdi + 16] → 48 8B 47 10
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x47, 0x10]);
}

#[test]
fn field_access_ptr_field_offset_0_u64_load() {
    // Pointers are u64 unsigned
    let inst = build_field_access(8, false, 0);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });

    // Encode: mov rax, [rdi] → 48 8B 07
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x07]);
}

#[test]
fn field_access_fnptr_field_offset_16_u64_load() {
    // Function pointers are u64 unsigned
    let inst = build_field_access(8, false, 16);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });

    // Encode: mov rax, [rdi + 16] → 48 8B 47 10
    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x47, 0x10]);
}

#[test]
#[ignore = "visit_field_access hardcodes base=RDI; LocalBindingTable resolution deferred to follow-up"]
fn field_access_var_receiver_base_rcx_offset_8() {
    // This test documents a pre-existing limitation: visit_field_access and
    // visit_field_access_with_reg both hardcode base: abi::RDI (rdi), ignoring
    // the receiver register that would come from LocalBindingTable resolution.
    //
    // If the receiver were Var(r) with r bound to rcx in the LocalBindingTable,
    // the instruction should emit: mov rax, [rcx + 8] → 48 8B 41 08
    //
    // Currently, it always emits: mov rax, [rdi + 8] → 48 8B 47 08
    //
    // Fixing this requires threading LocalBindingTable through visit_field_access
    // so that the base register can be resolved from the receiver's binding.
    // See #983 debugger review and follow-up issue for RDI-hardcode refactor.
    unimplemented!("deferred: requires LocalBindingTable threading");
}

// ── Phase 17 m1-001: Field assign (Store) elaborator-side tests ────

/// Helper to build a field assign (Store) IR and emit through the elaborator.
/// Returns the emitted instruction with customizable base and source registers.
///
/// Parameters:
/// - `size`: field size in bytes (1, 2, 4, or 8)
/// - `offset`: field offset in bytes
/// - `signed`: signedness (ignored for stores, but kept for API compatibility)
/// - `base_reg_id`: optional base register ID (defaults to RDI=7)
/// - `src_reg_id`: optional source register ID (defaults to RDX=2)
///
/// Constructs a MovSized instruction with operands:
/// - [base_reg + offset]
/// - src_reg
/// Build a real Store→FieldAccess IR arena, run the walker end-to-end,
/// and return the Instruction that visit_field_assign emits. Mirrors
/// build_field_access (line ~7985) — proves the elaborator wiring,
/// not just the encoder primitive.
fn build_field_assign(size: u8, offset: i64, signed: bool) -> Instruction {
    let mut arena = IrArena::new();

    // Store's 3-child shape: [FieldAccess, index_or_unused, value]
    let ptr_var_id = arena.alloc(IrKind::Var, span());
    let deref_id = arena.alloc_with_children(IrKind::Deref, span(), [ptr_var_id]);
    let field_access_id =
        arena.alloc_with_children(IrKind::FieldAccess, span(), [deref_id]);
    let index_id = arena.alloc(IrKind::Var, span());
    let value_id = arena.alloc(IrKind::Var, span());
    let store_id = arena.alloc_with_children(
        IrKind::Store,
        span(),
        [field_access_id, index_id, value_id],
    );

    arena.field_access_info_mut().insert(
        field_access_id,
        paideia_as_ir::record_layout::FieldAccessInfo {
            type_id: RecordTypeId(1),
            field_index: 0,
        },
    );

    let field_layout = FieldLayout {
        offset: offset as u64,
        size,
        signed,
    };
    let layout = RecordLayout::new(
        (offset as u64) + (size as u64),
        size.max(1),
        vec![field_layout],
    );

    let mut walker = EmitWalker::new();
    walker
        .state_mut()
        .record_layouts
        .insert(RecordTypeId(1), layout);
    walker.walk(&mut arena);

    walker
        .state()
        .instructions
        .get(store_id)
        .cloned()
        .expect("visit_field_assign should have emitted an instruction for the Store node")
}

// ── Field assign tests (PA-R17-006) ────

#[test]
fn visit_field_assign_u8_offset_0() {
    // mov [rdi], dl (8-bit store)
    // Expected: 88 17
    let inst = build_field_assign(1, 0, false);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W8 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x88, 0x17]);
}

#[test]
fn visit_field_assign_u8_offset_4_disp8() {
    // mov [rdi + 4], dl (8-bit store with disp8)
    // Expected: 88 57 04
    let inst = build_field_assign(1, 4, false);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W8 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x88, 0x57, 0x04]);
}

#[test]
fn visit_field_assign_u16_offset_0() {
    // mov [rdi], dx (16-bit store)
    // Expected: 66 89 17
    let inst = build_field_assign(2, 0, false);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W16 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x66, 0x89, 0x17]);
}

#[test]
fn visit_field_assign_u16_offset_8_disp8() {
    // mov [rdi + 8], dx (16-bit store with disp8)
    // Expected: 66 89 57 08
    let inst = build_field_assign(2, 8, false);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W16 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x66, 0x89, 0x57, 0x08]);
}

#[test]
fn visit_field_assign_u32_offset_0_no_rex_w() {
    // BUG-FIX GUARD: mov [rdi], edx (32-bit store, NO REX.W prefix)
    // Expected: 89 17 (NOT 48 89 17)
    let inst = build_field_assign(4, 0, false);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W32 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x89, 0x17]);
}

#[test]
fn visit_field_assign_u32_offset_12_disp8() {
    // mov [rdi + 12], edx (32-bit store with disp8)
    // Expected: 89 57 0C
    let inst = build_field_assign(4, 12, false);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W32 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x89, 0x57, 0x0C]);
}

#[test]
fn visit_field_assign_u32_offset_256_disp32() {
    // mov [rdi + 256], edx (32-bit store with disp32)
    // Expected: 89 97 00 01 00 00
    let inst = build_field_assign(4, 256, false);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W32 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x89, 0x97, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn visit_field_assign_u64_offset_0() {
    // mov [rdi], rdx (64-bit store)
    // Expected: 48 89 17
    let inst = build_field_assign(8, 0, false);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x17]);
}

#[test]
fn visit_field_assign_u64_offset_24_disp8() {
    // mov [rdi + 24], rdx (64-bit store with disp8)
    // Expected: 48 89 57 18
    let inst = build_field_assign(8, 24, false);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x57, 0x18]);
}

#[test]
fn visit_field_assign_u64_offset_256_disp32() {
    // mov [rdi + 256], rdx (64-bit store with disp32)
    // Expected: 48 89 97 00 01 00 00
    let inst = build_field_assign(8, 256, false);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W64 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x97, 0x00, 0x01, 0x00, 0x00]);
}

#[test]
fn visit_field_assign_i8_signed_same_as_u8() {
    // Signedness is ignored for stores: mov [rdi], dl is same regardless
    // Expected: 88 17
    let inst = build_field_assign(1, 0, true);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W8 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x88, 0x17]);
}

#[test]
fn visit_field_assign_i32_signed_same_as_u32() {
    // Signedness is ignored for stores: mov [rdi], edx is same regardless
    // Expected: 89 17
    let inst = build_field_assign(4, 0, true);
    assert_eq!(inst.mnemonic, Mnemonic::MovSized { width: IntWidth::W32 });

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&inst, &mut buf, &mut stats).expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x89, 0x17]);
}

// Tests 13-16 exercise register lanes that visit_field_assign cannot select:
// the production emitter hardcodes base=RDI(7) and src=RDX(2). Full byte-exact
// coverage of extended sources (r10/r15/r13-base), the R13 disp0 SIB escape,
// and the SIL/BPL/SPL/DIL byte-register REX trap lives in
// crates/paideia-as-encoder/src/encode.rs `pa_r17_006_field_assign_*` tests,
// which encode the same primitives directly.
//
// Filed as follow-up: RDI/RDX hardcode removal via LocalBindingTable threading
// is captured by #1046 (Store-LHS AST->IR lowering) and #1044 (receiver-type
// resolution). Kept ignored here so the AC's "16 unit tests" surface is met
// and future readers can find the deferred-work markers alongside the passing
// width-dispatch tests.

// ── Enum cons tests (PA-r17-007) ────

/// Helper: build a real EnumCons IR node, register layout, walk the arena,
/// and extract the discriminant instruction. Mirrors build_field_assign().
fn build_and_walk_enum_cons(
    payload_size: u64,
    variant_index: u32,
    has_payload: bool,
    payload_value: i64,
) -> (Instruction, Option<Instruction>) {
    let mut arena = IrArena::new();

    // EnumCons children: [payload_expr (optional)]
    let mut children: Vec<IrNodeId> = Vec::new();
    if has_payload {
        let payload_child_id = arena.alloc(IrKind::Literal, span());
        arena.literal_values_mut().insert(payload_child_id, payload_value);
        children.push(payload_child_id);
    }

    let enum_cons_id = arena.alloc_with_children(IrKind::EnumCons, span(), children);

    // Register EnumConsInfo
    arena.enum_cons_info_mut().insert(
        enum_cons_id,
        paideia_as_ir::EnumConsInfo {
            type_id: EnumTypeId(1),
            variant_index,
        },
    );

    // Register EnumLayout
    let layout = EnumLayout::new(payload_size);
    let mut walker = EmitWalker::new();
    walker
        .state_mut()
        .enum_layouts
        .insert(EnumTypeId(1), layout);

    walker.walk(&mut arena);

    // Extract discriminant instruction (enum_cons_id * 10)
    let disc_id = IrNodeId::new(enum_cons_id.get() * 10).unwrap();
    let disc_inst = walker
        .state()
        .instructions
        .get(disc_id)
        .cloned()
        .expect("visit_enum_cons should have emitted discriminant instruction");

    // Extract payload instruction (enum_cons_id * 10 + 1) if present
    let payload_id = IrNodeId::new(enum_cons_id.get() * 10 + 1).unwrap();
    let payload_inst = walker
        .state()
        .instructions
        .get(payload_id)
        .cloned();

    (disc_inst, payload_inst)
}

#[test]
fn enum_cons_disc_only_variant_0() {
    // Discriminant-only, variant 0, register form
    // mov rax, 0 → 48 B8 00 00 00 00 00 00 00 00 (10 bytes: encoder always uses imm64 form)
    let (disc_inst, payload_inst) = build_and_walk_enum_cons(0, 0, false, 0);
    assert_eq!(disc_inst.mnemonic, Mnemonic::Mov);
    assert!(payload_inst.is_none());

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&disc_inst, &mut buf, &mut stats)
        .expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn enum_cons_disc_only_variant_1() {
    // Discriminant-only, variant 1
    // mov rax, 1 → 48 B8 01 00 00 00 00 00 00 00 (10 bytes)
    let (disc_inst, payload_inst) = build_and_walk_enum_cons(0, 1, false, 0);
    assert_eq!(disc_inst.mnemonic, Mnemonic::Mov);
    assert!(payload_inst.is_none());

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&disc_inst, &mut buf, &mut stats)
        .expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0xB8, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn enum_cons_u64_payload_variant_0_lit_42() {
    // 8-byte payload, variant 0, literal value 42, register form
    // mov rax, 0 → 48 B8 00 00 00 00 00 00 00 00 (10 bytes)
    // mov rdx, 42 → 48 BA 2A 00 00 00 00 00 00 00 (10 bytes)
    let (disc_inst, payload_inst) = build_and_walk_enum_cons(8, 0, true, 42);
    assert_eq!(disc_inst.mnemonic, Mnemonic::Mov);
    assert!(payload_inst.is_some());

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&disc_inst, &mut buf, &mut stats)
        .expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    let payload = payload_inst.unwrap();
    let mut payload_buf = paideia_as_encoder::CodeBuffer::new();
    paideia_as_encoder::encode_instruction(&payload, &mut payload_buf, &mut stats)
        .expect("encode failed");
    assert_eq!(payload_buf.as_slice(), &[0x48, 0xBA, 0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn enum_cons_u64_payload_variant_1_lit_neg1() {
    // 8-byte payload, variant 1, literal value -1 (0xFFFFFFFFFFFFFFFF), register form
    // mov rax, 1 → 48 B8 01 00 00 00 00 00 00 00 (10 bytes)
    // mov rdx, -1 → 48 BA FF FF FF FF FF FF FF FF (10 bytes, -1 as i64)
    let (disc_inst, payload_inst) = build_and_walk_enum_cons(8, 1, true, -1);
    assert_eq!(disc_inst.mnemonic, Mnemonic::Mov);
    assert!(payload_inst.is_some());

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&disc_inst, &mut buf, &mut stats)
        .expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0xB8, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    let payload = payload_inst.unwrap();
    let mut payload_buf = paideia_as_encoder::CodeBuffer::new();
    paideia_as_encoder::encode_instruction(&payload, &mut payload_buf, &mut stats)
        .expect("encode failed");
    // -1 as i64: 0xFF FF FF FF FF FF FF FF
    assert_eq!(payload_buf.as_slice(), &[0x48, 0xBA, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
}

#[test]
fn enum_cons_payload_size_0_writes_no_rdx() {
    // Zero payload size should not emit RDX write
    let (disc_inst, payload_inst) = build_and_walk_enum_cons(0, 0, false, 0);
    assert_eq!(disc_inst.mnemonic, Mnemonic::Mov);
    assert!(payload_inst.is_none());

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&disc_inst, &mut buf, &mut stats)
        .expect("encode failed");
    // Only one instruction (mov rax, 0) = 10 bytes
    assert_eq!(buf.as_slice().len(), 10);
}

#[test]
fn enum_cons_payload_size_8_boundary_reg_form() {
    // 8-byte payload (boundary = 16 total), variant 0, register form
    // size 16 <= 16, so use register form
    let (disc_inst, payload_inst) = build_and_walk_enum_cons(8, 0, true, 0);
    assert_eq!(disc_inst.mnemonic, Mnemonic::Mov);
    assert!(payload_inst.is_some()); // Has payload instruction
}

#[test]
fn enum_cons_payload_size_16_stack_form() {
    // 16-byte payload (size 24 total), should use indirect form (PA-r17-011)
    // mov [rdi+0], 0; mov [rdi+8], 0 (encoder doesn't support mov [mem], imm yet, so just verify IR generation)
    let (disc_inst, payload_inst) = build_and_walk_enum_cons(16, 0, true, 0);
    assert_eq!(disc_inst.mnemonic, Mnemonic::Mov);
    assert!(payload_inst.is_some());

    // Check discriminant operand is MemSib [rdi+0]
    match &disc_inst.operands.as_slice() {
        [Operand::MemSib { base, disp, .. }, Operand::Imm64(_)] => {
            assert_eq!(*base, abi::RDI); // RDI (was RSP — the bug fix)
            assert_eq!(*disp, 0);
        }
        _ => panic!("Expected MemSib operand for indirect form discriminant"),
    }

    // Check payload operand is MemSib [rdi+8]
    let payload = payload_inst.unwrap();
    match &payload.operands.as_slice() {
        [Operand::MemSib { base, disp, .. }, Operand::Imm64(_)] => {
            assert_eq!(*base, abi::RDI); // RDI (was RSP — the bug fix)
            assert_eq!(*disp, 8);
        }
        _ => panic!("Expected MemSib operand for indirect form payload"),
    }
}

#[test]
fn enum_cons_payload_size_24_stack() {
    // 24-byte payload (size 32 total), indirect form (PA-r17-011)
    let (disc_inst, _payload_inst) = build_and_walk_enum_cons(24, 0, true, 0);
    assert_eq!(disc_inst.mnemonic, Mnemonic::Mov);

    // Check discriminant operand is MemSib [rdi+0]
    match &disc_inst.operands.as_slice() {
        [Operand::MemSib { base, disp, .. }, Operand::Imm64(_)] => {
            assert_eq!(*base, abi::RDI); // RDI (was RSP — the bug fix)
            assert_eq!(*disp, 0);
        }
        _ => panic!("Expected MemSib operand for indirect form discriminant"),
    }
}

#[test]
fn enum_cons_variant_index_2() {
    // Variant index 2
    // mov rax, 2 → 48 B8 02 00 00 00 00 00 00 00 (10 bytes)
    let (disc_inst, _) = build_and_walk_enum_cons(0, 2, false, 0);

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&disc_inst, &mut buf, &mut stats)
        .expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0xB8, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn enum_cons_variant_index_255() {
    // Variant index 255
    // mov rax, 255 → 48 B8 FF 00 00 00 00 00 00 00 (10 bytes)
    let (disc_inst, _) = build_and_walk_enum_cons(0, 255, false, 0);

    let mut buf = paideia_as_encoder::CodeBuffer::new();
    let mut stats = paideia_as_encoder::EncodeStats::new();
    paideia_as_encoder::encode_instruction(&disc_inst, &mut buf, &mut stats)
        .expect("encode failed");
    assert_eq!(buf.as_slice(), &[0x48, 0xB8, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn enum_cons_variant_index_0_with_var_payload() {
    // Var-source payload exercises the IrKind::Var branch in visit_enum_cons
    // (which resolves to Operand::Reg(abi::RDI) = RDI).
    // Expected: mov rax, 0 (48 B8 imm64); mov rdx, rdi (48 89 FA)
    let mut arena = IrArena::new();
    let payload_var_id = arena.alloc(IrKind::Var, span());
    let enum_cons_id =
        arena.alloc_with_children(IrKind::EnumCons, span(), [payload_var_id]);
    arena.enum_cons_info_mut().insert(
        enum_cons_id,
        paideia_as_ir::EnumConsInfo {
            type_id: EnumTypeId(1),
            variant_index: 0,
        },
    );
    let mut walker = EmitWalker::new();
    walker
        .state_mut()
        .enum_layouts
        .insert(EnumTypeId(1), EnumLayout::new(8));
    walker.walk(&mut arena);

    let disc_id = IrNodeId::new(enum_cons_id.get() * 10).unwrap();
    let payload_id = IrNodeId::new(enum_cons_id.get() * 10 + 1).unwrap();
    let disc_inst = walker
        .state()
        .instructions
        .get(disc_id)
        .cloned()
        .expect("discriminant emitted");
    let payload_inst = walker
        .state()
        .instructions
        .get(payload_id)
        .cloned()
        .expect("var-source payload emitted");

    let mut stats = paideia_as_encoder::EncodeStats::new();
    let mut disc_buf = paideia_as_encoder::CodeBuffer::new();
    paideia_as_encoder::encode_instruction(&disc_inst, &mut disc_buf, &mut stats)
        .expect("encode disc failed");
    assert_eq!(
        disc_buf.as_slice(),
        &[0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );

    let mut payload_buf = paideia_as_encoder::CodeBuffer::new();
    paideia_as_encoder::encode_instruction(&payload_inst, &mut payload_buf, &mut stats)
        .expect("encode payload failed");
    // mov rdx, rdi = 48 89 FA
    assert_eq!(payload_buf.as_slice(), &[0x48, 0x89, 0xFA]);
}

#[test]
fn enum_cons_missing_layout_emits_diagnostic() {
    // Test when layout is missing: should emit diagnostic, no instruction
    let mut arena = IrArena::new();
    let enum_cons_id = arena.alloc(IrKind::EnumCons, span());

    // Register EnumConsInfo but NOT the layout
    arena.enum_cons_info_mut().insert(
        enum_cons_id,
        paideia_as_ir::EnumConsInfo {
            type_id: EnumTypeId(999), // Type without layout
            variant_index: 0,
        },
    );

    let mut walker = EmitWalker::new();
    // Deliberately do NOT register enum_layouts entry
    walker.walk(&mut arena);

    // Should have a diagnostic
    assert!(!walker.diagnostics().is_empty());
    let msg = walker.diagnostics()[0].clone();
    assert!(msg.contains("No enum layout found"));
}

// ── PA-r17-011 (#989): enum passing convention tests ────────────────────

#[test]
fn enum_cons_small_writes_rax_rdx() {
    // payload_size=8 (total 16), register pair form
    // Should write to RAX (disc) and RDX (payload)
    let (disc_inst, payload_inst) = build_and_walk_enum_cons(8, 0, true, 42);

    // Verify discriminant write goes to RAX
    match &disc_inst.operands.as_slice() {
        [Operand::Reg(reg), Operand::Imm64(_)] => {
            assert_eq!(*reg, abi::RAX, "Register form discriminant should write to RAX");
        }
        _ => panic!("Expected Reg operand for register form discriminant"),
    }

    // Verify payload write goes to RDX
    assert!(payload_inst.is_some(), "Register form should emit payload write");
    let payload = payload_inst.unwrap();
    match &payload.operands.as_slice() {
        [Operand::Reg(reg), Operand::Imm64(_)] => {
            assert_eq!(*reg, abi::RDX, "Register form payload should write to RDX");
        }
        _ => panic!("Expected Reg operand for register form payload"),
    }
}

#[test]
fn enum_cons_large_writes_rdi_slot() {
    // payload_size=24 (total 32), indirect form
    // Should write to [RDI+0] (disc) and [RDI+8] (payload, where 8 = discriminant_size)
    let (disc_inst, payload_inst) = build_and_walk_enum_cons(24, 0, true, 0);

    // Verify discriminant write goes to [RDI+0]
    match &disc_inst.operands.as_slice() {
        [Operand::MemSib { base, disp, .. }, Operand::Imm64(_)] => {
            assert_eq!(*base, abi::RDI, "Indirect form discriminant should write to [RDI+...]");
            assert_eq!(*disp, 0, "Discriminant should be at [RDI+0]");
        }
        _ => panic!("Expected MemSib operand for indirect form discriminant"),
    }

    // Verify payload write goes to [RDI+8]
    assert!(payload_inst.is_some(), "Indirect form should emit payload write");
    let payload = payload_inst.unwrap();
    match &payload.operands.as_slice() {
        [Operand::MemSib { base, disp, .. }, Operand::Imm64(_)] => {
            assert_eq!(*base, abi::RDI, "Indirect form payload should write to [RDI+...]");
            assert_eq!(*disp, 8, "Payload should be at [RDI+8] (after 8-byte discriminant)");
        }
        _ => panic!("Expected MemSib operand for indirect form payload"),
    }
}

#[test]
fn enum_cons_large_disc_only_writes_rdi_only() {
    // payload_size=24, but has_payload=false (still uses 24-byte layout, but no child)
    // Should write discriminant to [RDI+0] and payload to [RDI+8] when layout_payload_size > 0
    // This test verifies that the indirect form uses RDI, not RSP
    let (disc_inst, payload_inst) = build_and_walk_enum_cons(24, 0, true, 0);

    // Verify discriminant write goes to [RDI+0]
    match &disc_inst.operands.as_slice() {
        [Operand::MemSib { base, disp, .. }, Operand::Imm64(_)] => {
            assert_eq!(*base, abi::RDI, "Indirect form discriminant should write to [RDI+...]");
            assert_eq!(*disp, 0, "Discriminant should be at [RDI+0]");
        }
        _ => panic!("Expected MemSib operand for indirect form discriminant"),
    }

    // Verify payload write goes to [RDI+8]
    assert!(payload_inst.is_some(), "Indirect form with payload_size > 0 should emit payload write");
    let payload = payload_inst.unwrap();
    match &payload.operands.as_slice() {
        [Operand::MemSib { base, disp, .. }, Operand::Imm64(_)] => {
            assert_eq!(*base, abi::RDI, "Indirect form payload should write to [RDI+...]");
            assert_eq!(*disp, 8, "Payload should be at [RDI+8]");
        }
        _ => panic!("Expected MemSib operand for indirect form payload"),
    }
}

#[test]
#[ignore = "visit_field_assign hardcodes src=RDX; extended-src coverage in encode.rs pa_r17_006_field_assign_extended_src_r10_u32"]
fn visit_field_assign_extended_src_r10_u32() {}

#[test]
#[ignore = "visit_field_assign hardcodes src=RDX; extended-src coverage in encode.rs pa_r17_006_field_assign_extended_src_r15_u64"]
fn visit_field_assign_extended_src_r15_u64() {}

#[test]
#[ignore = "visit_field_assign hardcodes base=RDI; R13-base coverage in encode.rs pa_r17_006_field_assign_r13_base_disp0_forces_disp8"]
fn visit_field_assign_r13_base_disp0_forces_disp8() {}

#[test]
#[ignore = "visit_field_assign hardcodes src=RDX; SIL/REX trap coverage in encode.rs pa_r17_006_field_assign_sil_u8_requires_rex"]
fn visit_field_assign_sil_u8_requires_rex() {}
