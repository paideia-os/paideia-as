use super::super::*;
use crate::emit_fixture::EmitFixture;
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::CallMeta;

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

#[test]
fn emit_walker_new_starts_empty() {
    let walker = EmitWalker::new();
    assert!(walker.state().instructions.is_empty());
    assert_eq!(walker.state().current_function, 0);
    assert_eq!(walker.state().estimated_offset, 0);
    assert!(walker.state().lambda_first_instr().is_empty());
}

#[test]
fn emit_walker_walk_on_empty_arena_emits_zero_diagnostics() {
    let mut f = EmitFixture::new();
    f.walk();
    f.assert_no_diagnostics();
}

#[test]
fn emit_pass_state_default_is_clean() {
    let state = EmitPassState::default();
    assert!(state.instructions.is_empty());
    assert_eq!(state.current_function, 0);
    assert_eq!(state.estimated_offset, 0);
    assert!(state.lambda_first_instr.is_empty());
}

#[test]
fn emit_walker_lets_literal_42_emits_7_byte_mov() {
    let mut f = EmitFixture::new();
    let lit_id = f.literal(42);
    let let_id = f.let_binding(lit_id);
    f.walk();

    let inst = f.instruction(let_id);
    assert_eq!(inst.mnemonic, Mnemonic::Mov);
    assert_eq!(inst.operands.len(), 2);
    assert_eq!(inst.operands[0], Operand::Reg(abi::RAX));
    assert_eq!(inst.operands[1], Operand::Imm64(42));

    // 32-bit immediate encoding = 7 bytes.
    assert_eq!(f.estimated_offset(), 7);
}

/// Phase 7 m4-003: `let x : u32 = 42` (typed) emits the narrow MovSized
/// form (5-byte `B8 imm32`), not the generic 64-bit move.
#[test]
fn emit_walker_typed_u32_let_emits_mov_sized_w32() {
    use paideia_as_ir::{IntWidth, LetInfo, TypeId as IrTypeId};
    use paideia_as_types::TypeInterner;

    let mut arena = IrArena::new();
    let lit_id = arena.alloc(IrKind::Literal, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);
    arena.literal_values_mut().insert(lit_id, 42);

    // Build a type interner with a u32 type and record it on the binding.
    let mut typer = TypeInterner::new();
    let u32_id = typer.uint(32);
    arena.let_meta_mut().insert(
        let_id,
        LetInfo::with_type(false, Some(IrTypeId(u32_id.get()))),
    );

    let mut walker = EmitWalker::new();
    walker.walk_with_typer(&mut arena, &typer);

    let inst = walker
        .state()
        .instructions
        .get(let_id)
        .expect("instruction should be emitted");
    assert_eq!(
        inst.mnemonic,
        Mnemonic::MovSized {
            width: IntWidth::W32
        }
    );
    assert_eq!(inst.operands[1], Operand::Imm64(42));
    // 5-byte narrow form (B8 imm32), not the 7-byte 64-bit form.
    assert_eq!(walker.state().estimated_offset, 5);
}

/// Phase 7 m4-003: a `u64`-typed binding keeps the generic 64-bit Mov path.
#[test]
fn emit_walker_typed_u64_let_keeps_generic_mov() {
    use paideia_as_ir::{LetInfo, TypeId as IrTypeId};
    use paideia_as_types::TypeInterner;

    let mut arena = IrArena::new();
    let lit_id = arena.alloc(IrKind::Literal, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);
    arena.literal_values_mut().insert(lit_id, 42);

    let mut typer = TypeInterner::new();
    let u64_id = typer.uint(64);
    arena.let_meta_mut().insert(
        let_id,
        LetInfo::with_type(false, Some(IrTypeId(u64_id.get()))),
    );

    let mut walker = EmitWalker::new();
    walker.walk_with_typer(&mut arena, &typer);

    let inst = walker.state().instructions.get(let_id).unwrap();
    // W64 falls through to the generic Mov path (7 bytes for imm32-range 42).
    assert_eq!(inst.mnemonic, Mnemonic::Mov);
    assert_eq!(walker.state().estimated_offset, 7);
}

/// Phase 7 m4-003: untyped bindings (no LetInfo.ty) keep the generic path,
/// even when a typer is supplied — preserving backward compatibility.
#[test]
fn emit_walker_untyped_let_with_typer_keeps_generic_mov() {
    use paideia_as_types::TypeInterner;

    let mut arena = IrArena::new();
    let lit_id = arena.alloc(IrKind::Literal, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);
    arena.literal_values_mut().insert(lit_id, 42);

    let typer = TypeInterner::new();
    let mut walker = EmitWalker::new();
    walker.walk_with_typer(&mut arena, &typer);

    let inst = walker.state().instructions.get(let_id).unwrap();
    assert_eq!(inst.mnemonic, Mnemonic::Mov);
    assert_eq!(walker.state().estimated_offset, 7);
}

#[test]
fn emit_walker_lets_literal_64bit_emits_10_byte_mov() {
    let value = 0xCAFE_F00D_DEAD_BEEFu64 as i64;
    let mut f = EmitFixture::new();
    let lit_id = f.literal(value);
    let let_id = f.let_binding(lit_id);
    f.walk();

    let inst = f.instruction(let_id);
    assert_eq!(inst.mnemonic, Mnemonic::Mov);
    assert_eq!(inst.operands.len(), 2);
    assert_eq!(inst.operands[0], Operand::Reg(abi::RAX));
    assert_eq!(inst.operands[1], Operand::Imm64(value));

    // 64-bit immediate encoding = 10 bytes.
    assert_eq!(f.estimated_offset(), 10);
}

// ── Lambda lowering tests (m1-003) ──────────────────────────────────

#[test]
fn emit_walker_lambda_identity_emits_mov_rax_rdi_ret() {
    let mut f = EmitFixture::new();
    let var_id = f.var();
    let lambda_id = f.lambda(var_id);
    f.walk();

    // Phase-5-m1-003: instructions are stored at virtual node IDs
    // (lambda_id*2, lambda_id*2+1) to ensure proper sorting.
    let main_id = IrNodeId::new(lambda_id.get() * 2).expect("main instr id");
    let ret_id = IrNodeId::new(lambda_id.get() * 2 + 1).expect("ret instr id");

    let inst = f.instruction(main_id);
    assert_eq!(inst.mnemonic, Mnemonic::Mov);
    assert_eq!(inst.operands.len(), 2);
    assert_eq!(inst.operands[0], Operand::Reg(abi::RAX));
    assert_eq!(inst.operands[1], Operand::Reg(abi::RDI));

    assert_eq!(f.instruction(ret_id).mnemonic, Mnemonic::Ret);

    // 3 bytes for mov + 1 byte for ret = 4 bytes.
    assert_eq!(f.estimated_offset(), 4);

    // Lambda offset recorded.
    assert!(
        f.walker
            .state()
            .lambda_first_instr()
            .contains_key(&lambda_id.get())
    );
}

#[test]
fn emit_walker_lambda_bitnot_emits_mov_rax_rdi_not_rax_ret() {
    // Phase 7 m4-001: `fn (x) -> ~x` lowers to a Lambda whose body is a
    // BitNot over the parameter. Expect `mov rax, rdi; not rax; ret`.
    let mut arena = IrArena::new();

    // Body: BitNot with the parameter Var as its single child.
    let var_id = arena.alloc(IrKind::Var, span());
    let bitnot_id = arena.alloc_with_children(IrKind::BitNot, span(), [var_id]);
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [bitnot_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // The 3-instruction bitnot emitter keys on lambda*3 + {0,1,2}.
    let mov_id = IrNodeId::new(lambda_id.get() * 3).expect("mov instr id");
    let not_id = IrNodeId::new(lambda_id.get() * 3 + 1).expect("not instr id");
    let ret_id = IrNodeId::new(lambda_id.get() * 3 + 2).expect("ret instr id");

    // mov rax, rdi
    let mov_inst = walker
        .state()
        .instructions
        .get(mov_id)
        .expect("mov instruction should be emitted");
    assert_eq!(mov_inst.mnemonic, Mnemonic::Mov);
    assert_eq!(mov_inst.operands.len(), 2);
    assert_eq!(mov_inst.operands[0], Operand::Reg(abi::RAX)); // rax
    assert_eq!(mov_inst.operands[1], Operand::Reg(abi::RDI)); // rdi

    // not rax
    let not_inst = walker
        .state()
        .instructions
        .get(not_id)
        .expect("not instruction should be emitted");
    assert_eq!(not_inst.mnemonic, Mnemonic::Not);
    assert_eq!(not_inst.operands.len(), 1);
    assert_eq!(not_inst.operands[0], Operand::Reg(abi::RAX)); // rax

    // ret
    let ret_inst = walker
        .state()
        .instructions
        .get(ret_id)
        .expect("ret instruction should be emitted");
    assert_eq!(ret_inst.mnemonic, Mnemonic::Ret);

    // Offset: 3 (mov) + 3 (not) + 1 (ret) = 7 bytes.
    assert_eq!(walker.state().estimated_offset, 7);

    // Lambda offset recorded.
    assert!(
        walker
            .state()
            .lambda_first_instr()
            .contains_key(&lambda_id.get())
    );
}

#[test]
fn emit_walker_lambda_cast_emits_movsx_rax_edi_ret() {
    // Phase 7 m4-002: `fn (x) -> x as i64` lowers to a Lambda whose body is
    // a Cast over the parameter. Expect `movsx rax, edi; ret`.
    let mut arena = IrArena::new();

    // Body: Cast with the parameter Var as its single child.
    let var_id = arena.alloc(IrKind::Var, span());
    let cast_id = arena.alloc_with_children(IrKind::Cast, span(), [var_id]);
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [cast_id]);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // The 2-instruction cast emitter keys on lambda*2 + {0,1}.
    let movsx_id = IrNodeId::new(lambda_id.get() * 2).expect("movsx instr id");
    let ret_id = IrNodeId::new(lambda_id.get() * 2 + 1).expect("ret instr id");

    // movsx rax, edi
    let movsx_inst = walker
        .state()
        .instructions
        .get(movsx_id)
        .expect("movsx instruction should be emitted");
    assert_eq!(movsx_inst.mnemonic, Mnemonic::Movsx);
    assert_eq!(movsx_inst.operands.len(), 2);
    assert_eq!(movsx_inst.operands[0], Operand::Reg(abi::RAX)); // rax
    assert_eq!(movsx_inst.operands[1], Operand::Reg(abi::RDI)); // rdi/edi
    assert_eq!(
        movsx_inst.encoding_hint.map(|h| h.operand_size),
        Some(4),
        "canonical i32 as i64 widening reads a 4-byte source"
    );

    // ret
    let ret_inst = walker
        .state()
        .instructions
        .get(ret_id)
        .expect("ret instruction should be emitted");
    assert_eq!(ret_inst.mnemonic, Mnemonic::Ret);

    // Offset: 3 (movsx) + 1 (ret) = 4 bytes.
    assert_eq!(walker.state().estimated_offset, 4);

    // Lambda offset recorded.
    assert!(
        walker
            .state()
            .lambda_first_instr()
            .contains_key(&lambda_id.get())
    );
}

// ---- PA8 m3-002 (#826): cast dispatch table ----

fn shape(src_width: u8, dst_width: u8, src_signed: bool, dst_signed: bool) -> CastShape {
    CastShape {
        src_width,
        dst_width,
        src_signed,
        dst_signed,
    }
}

#[test]
fn cast_plan_widening_signed_dispatches_movsx() {
    // i8/i16 → i64 use the 0F BE / 0F BF movsx forms; i32 → i64 uses MOVSXD.
    assert_eq!(cast_plan(shape(1, 8, true, true)), CastPlan::SignExtend(1));
    assert_eq!(cast_plan(shape(2, 8, true, true)), CastPlan::SignExtend(2));
    assert_eq!(cast_plan(shape(4, 8, true, true)), CastPlan::SignExtend(4));

    // movsxd (4-byte src) lowers to Movsx/opcode 0x63, 3 bytes.
    let (m, hint, size) = cast_plan(shape(4, 8, true, true)).instruction().unwrap();
    assert_eq!(m, Mnemonic::Movsx);
    assert_eq!(hint.unwrap().opcode, 0x63);
    assert_eq!(hint.unwrap().operand_size, 4);
    assert_eq!(size, 3);

    // movsxbq (1-byte src) lowers to Movsx/opcode 0x0F, 4 bytes.
    let (m, hint, size) = cast_plan(shape(1, 8, true, true)).instruction().unwrap();
    assert_eq!(m, Mnemonic::Movsx);
    assert_eq!(hint.unwrap().opcode, 0x0F);
    assert_eq!(hint.unwrap().operand_size, 1);
    assert_eq!(size, 4);
}

#[test]
fn cast_plan_widening_unsigned_dispatches_movzx_or_mov32() {
    // u8/u16 → u64 use movzx (0F B6 / 0F B7); u32 → u64 uses a 32-bit mov.
    assert_eq!(
        cast_plan(shape(1, 8, false, false)),
        CastPlan::ZeroExtend(1)
    );
    assert_eq!(
        cast_plan(shape(2, 8, false, false)),
        CastPlan::ZeroExtend(2)
    );
    assert_eq!(cast_plan(shape(4, 8, false, false)), CastPlan::Mov32);

    // movzx u8 → Movzx/opcode 0xB6, 4 bytes.
    let (m, hint, size) = cast_plan(shape(1, 8, false, false)).instruction().unwrap();
    assert_eq!(m, Mnemonic::Movzx);
    assert_eq!(hint.unwrap().opcode, 0xB6);
    assert_eq!(size, 4);

    // 32-bit mov implicitly zero-extends → Mov, operand_size 4, 2 bytes.
    let (m, hint, size) = cast_plan(shape(4, 8, false, false)).instruction().unwrap();
    assert_eq!(m, Mnemonic::Mov);
    assert_eq!(hint.unwrap().operand_size, 4);
    assert_eq!(size, 2);
}

#[test]
fn cast_plan_narrowing_dispatches_mov_dest_width() {
    // Any → smaller width truncates via a destination-sized mov, regardless
    // of signedness.
    assert_eq!(cast_plan(shape(8, 4, true, false)), CastPlan::Narrow(4));
    assert_eq!(cast_plan(shape(8, 2, false, false)), CastPlan::Narrow(2));
    assert_eq!(cast_plan(shape(4, 1, true, true)), CastPlan::Narrow(1));

    let (m, hint, size) = cast_plan(shape(8, 1, true, true)).instruction().unwrap();
    assert_eq!(m, Mnemonic::Mov);
    assert_eq!(hint.unwrap().operand_size, 1);
    assert_eq!(size, 2);
}

#[test]
fn cast_plan_same_width_is_nop() {
    // Same-width reinterpret (incl. signed<->unsigned of equal width) emits
    // no conversion instruction.
    for w in [1u8, 2, 4, 8] {
        assert_eq!(cast_plan(shape(w, w, true, true)), CastPlan::Nop);
        assert_eq!(cast_plan(shape(w, w, true, false)), CastPlan::Nop);
        assert_eq!(cast_plan(shape(w, w, false, true)), CastPlan::Nop);
    }
    assert!(CastPlan::Nop.instruction().is_none());
}

#[test]
fn emit_cast_lambda_with_shape_narrowing_emits_single_mov_then_ret() {
    // Narrowing emits exactly one conversion mov (2 bytes) + ret (1 byte).
    let mut arena = IrArena::new();
    let var_id = arena.alloc(IrKind::Var, span());
    let cast_id = arena.alloc_with_children(IrKind::Cast, span(), [var_id]);
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [cast_id]);

    let mut walker = EmitWalker::new();
    walker.emit_cast_lambda_with_shape(lambda_id, shape(8, 4, true, false));

    let mov_id = IrNodeId::new(lambda_id.get() * 2).expect("mov instr id");
    let mov = walker
        .state()
        .instructions
        .get(mov_id)
        .expect("mov emitted");
    assert_eq!(mov.mnemonic, Mnemonic::Mov);
    assert_eq!(mov.encoding_hint.map(|h| h.operand_size), Some(4));

    // Encoder emits mov (3 bytes: 48 8B FA-family) + ret (1) = 4 bytes.
    // Previously this test asserted 3, matching a hardcoded `+= 2` in
    // emit_cast_lambda that was drifting from encoder truth — same class
    // of bug as #985/#986. Step 5 (emit_inst) surfaces the correct value.
    assert_eq!(walker.state().estimated_offset, 4);
}

#[test]
fn emit_cast_lambda_with_shape_same_width_emits_only_ret() {
    // A same-width reinterpret emits no conversion instruction, only ret.
    let mut arena = IrArena::new();
    let var_id = arena.alloc(IrKind::Var, span());
    let cast_id = arena.alloc_with_children(IrKind::Cast, span(), [var_id]);
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [cast_id]);

    let mut walker = EmitWalker::new();
    walker.emit_cast_lambda_with_shape(lambda_id, shape(8, 8, true, false));

    // No conversion instruction at node*2.
    let conv_id = IrNodeId::new(lambda_id.get() * 2).expect("conv id");
    assert!(walker.state().instructions.get(conv_id).is_none());

    // ret present at node*2+1; offset is just 1 byte.
    let ret_id = IrNodeId::new(lambda_id.get() * 2 + 1).expect("ret id");
    assert_eq!(
        walker.state().instructions.get(ret_id).map(|i| i.mnemonic),
        Some(Mnemonic::Ret)
    );
    assert_eq!(walker.state().estimated_offset, 1);
}

#[test]
fn emit_walker_lambda_double_emits_lea_rdi_rdi_ret() {
    let mut arena = IrArena::new();

    // Allocate: Var nodes for both operands, then App with [callee, arg0, arg1].
    // Assume callee is +.
    let callee_id = arena.alloc(IrKind::Var, span());
    let arg0_id = arena.alloc(IrKind::Var, span());
    let arg1_id = arena.alloc(IrKind::Var, span());
    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id, arg0_id, arg1_id]);

    // Populate call_sites so operator_lexeme_of can find the "+" operator.
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "+".to_string(),
        arg_count: 2,
        is_intrinsic: true,
    });

    // Allocate Lambda with App as body.
    // Note: Lambda IDs are small in unit tests. For the (Var, Var) case to emit, we need lambda_id > 50.
    // We'll manually craft the test to have lambda_id in the right range, or we'll use a large ID.
    // For now, let's allocate more nodes first to push lambda_id > 50.
    for _ in 0..50 {
        arena.alloc(IrKind::Literal, span());
    }
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify instructions were emitted for the lambda (lea + ret).
    // Phase-5-m1-003: instructions are now stored at virtual node IDs (lambda_id*2, lambda_id*2+1)
    let main_id = IrNodeId::new(lambda_id.get() * 2).expect("main instr id");
    let ret_id = IrNodeId::new(lambda_id.get() * 2 + 1).expect("ret instr id");

    let inst = walker
        .state()
        .instructions
        .get(main_id)
        .expect("instruction should be emitted");
    assert_eq!(inst.mnemonic, Mnemonic::Lea);
    assert_eq!(inst.operands.len(), 2);
    assert_eq!(inst.operands[0], Operand::Reg(abi::RAX)); // rax

    // Check MemSib: [rdi + rdi]
    match inst.operands[1] {
        Operand::MemSib {
            base,
            index,
            scale,
            disp,
        } => {
            assert_eq!(base, abi::RDI); // rdi
            assert_eq!(index, Some(abi::RDI)); // rdi
            assert_eq!(scale, paideia_as_ir::instruction::Scale::X1);
            assert_eq!(disp, 0);
        }
        _ => panic!("Expected MemSib operand"),
    }

    let ret_inst = walker
        .state()
        .instructions
        .get(ret_id)
        .expect("ret instruction should be emitted");
    assert_eq!(ret_inst.mnemonic, Mnemonic::Ret);

    // Verify offset: 4 bytes for lea + 1 byte for ret = 5 bytes.
    assert_eq!(walker.state().estimated_offset, 5);

    // Verify lambda offset recorded.
    assert!(
        walker
            .state()
            .lambda_first_instr()
            .contains_key(&lambda_id.get())
    );
}

#[test]
fn emit_walker_lambda_add_one_emits_lea_rdi_1_ret() {
    let mut arena = IrArena::new();

    // Allocate: Var (arg0), Literal (1), and App with [callee, arg0, lit].
    let callee_id = arena.alloc(IrKind::Var, span());
    let arg0_id = arena.alloc(IrKind::Var, span());
    let lit_id = arena.alloc(IrKind::Literal, span());
    let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id, arg0_id, lit_id]);

    // Register the literal value 1.
    arena.literal_values_mut().insert(lit_id, 1);

    // Populate call_sites so operator_lexeme_of can find the "+" operator.
    arena.call_sites_mut().insert(app_id, CallMeta {
        callee_name: "+".to_string(),
        arg_count: 2,
        is_intrinsic: true,
    });

    // Allocate Lambda with App as body.
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify instructions were emitted for the lambda (lea + ret).
    // Phase-5-m1-003: instructions are now stored at virtual node IDs (lambda_id*2, lambda_id*2+1)
    let main_id = IrNodeId::new(lambda_id.get() * 2).expect("main instr id");
    let ret_id = IrNodeId::new(lambda_id.get() * 2 + 1).expect("ret instr id");

    let inst = walker
        .state()
        .instructions
        .get(main_id)
        .expect("instruction should be emitted");
    assert_eq!(inst.mnemonic, Mnemonic::Lea);
    assert_eq!(inst.operands.len(), 2);
    assert_eq!(inst.operands[0], Operand::Reg(abi::RAX)); // rax

    // Check MemSib: [rdi + 1]
    match inst.operands[1] {
        Operand::MemSib {
            base,
            index,
            scale,
            disp,
        } => {
            assert_eq!(base, abi::RDI); // rdi
            assert_eq!(index, None);
            assert_eq!(scale, paideia_as_ir::instruction::Scale::X1);
            assert_eq!(disp, 1);
        }
        _ => panic!("Expected MemSib operand"),
    }

    let ret_inst = walker
        .state()
        .instructions
        .get(ret_id)
        .expect("ret instruction should be emitted");
    assert_eq!(ret_inst.mnemonic, Mnemonic::Ret);

    // Verify offset: 4 bytes for lea + 1 byte for ret = 5 bytes.
    assert_eq!(walker.state().estimated_offset, 5);

    // Verify lambda offset recorded.
    assert!(
        walker
            .state()
            .lambda_first_instr()
            .contains_key(&lambda_id.get())
    );
}

// ── Unsafe block recording tests (m1-004) ──────────────────────────────────

#[test]
fn emit_walker_unsafe_node_recorded_in_pending() {
    let mut arena = IrArena::new();

    // Allocate a single Unsafe node with an empty body (no children).
    let unsafe_id = arena.alloc(IrKind::Unsafe, span());

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify the unsafe node was recorded in pending_unsafe_blocks.
    assert_eq!(walker.state().pending_unsafe_count(), 1);
    assert_eq!(walker.state().pending_unsafe_blocks[0], unsafe_id.get());
}

#[test]
fn emit_walker_two_unsafe_nodes_recorded_in_order() {
    let mut arena = IrArena::new();

    // Allocate two Unsafe nodes.
    let unsafe_id_1 = arena.alloc(IrKind::Unsafe, span());
    let unsafe_id_2 = arena.alloc(IrKind::Unsafe, span());

    // Walk the arena.
    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    // Verify both unsafe nodes were recorded in order.
    assert_eq!(walker.state().pending_unsafe_count(), 2);
    assert_eq!(walker.state().pending_unsafe_blocks[0], unsafe_id_1.get());
    assert_eq!(walker.state().pending_unsafe_blocks[1], unsafe_id_2.get());
}

#[test]
fn emit_pass_state_take_pending_drains() {
    let mut state = EmitPassState::default();

    // Add some pending unsafe blocks.
    state.pending_unsafe_blocks.push(1);
    state.pending_unsafe_blocks.push(2);
    state.pending_unsafe_blocks.push(3);

    // Take the pending unsafe blocks.
    let taken = state.take_pending_unsafe();

    // Verify the taken vector has the expected contents.
    assert_eq!(taken.len(), 3);
    assert_eq!(taken[0], 1);
    assert_eq!(taken[1], 2);
    assert_eq!(taken[2], 3);

    // Verify the state's pending list is now empty.
    assert!(state.pending_unsafe_blocks.is_empty());
}

// ── Data table population tests (m4-003) ──────────────────────────────────

use paideia_as_ir::SectionKind;

#[test]
fn emit_walker_populate_data_table_empty_arena() {
    let arena = IrArena::new();
    let mut data_table = DataSideTable::new();

    EmitWalker::populate_data_table(&arena, &mut data_table);
    assert!(data_table.is_empty());
}

#[test]
fn emit_walker_populate_data_table_let_literal_value() {
    let mut arena = IrArena::new();

    // Allocate: Literal node with value 0x0011223344556677, then Let with Literal as child.
    let lit_id = arena.alloc(IrKind::Literal, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);

    // Register the literal value.
    arena
        .literal_values_mut()
        .insert(lit_id, 0x0011223344556677i64);

    // Populate the data table.
    let mut data_table = DataSideTable::new();
    EmitWalker::populate_data_table(&arena, &mut data_table);

    // Verify the entry was created.
    let entry = data_table.get(let_id).expect("data entry should exist");
    assert_eq!(entry.section, SectionKind::Rodata);
    assert_eq!(entry.align, 8);
    assert_eq!(entry.bytes.len(), 8);
    // Little-endian: 77 66 55 44 33 22 11 00
    assert_eq!(entry.bytes[0], 0x77);
    assert_eq!(entry.bytes[7], 0x00);
}

#[test]
fn emit_walker_populate_data_table_multiple_entries() {
    let mut arena = IrArena::new();

    // Allocate first Let-Literal.
    let lit1_id = arena.alloc(IrKind::Literal, span());
    let let1_id = arena.alloc_with_children(IrKind::Let, span(), [lit1_id]);
    arena
        .literal_values_mut()
        .insert(lit1_id, 0x0102030405060708i64);

    // Allocate second Let-Literal.
    let lit2_id = arena.alloc(IrKind::Literal, span());
    let let2_id = arena.alloc_with_children(IrKind::Let, span(), [lit2_id]);
    arena
        .literal_values_mut()
        .insert(lit2_id, 0x0807060504030201i64);

    // Populate the data table.
    let mut data_table = DataSideTable::new();
    EmitWalker::populate_data_table(&arena, &mut data_table);

    // Verify both entries were created.
    assert_eq!(data_table.len(), 2);
    assert!(data_table.get(let1_id).is_some());
    assert!(data_table.get(let2_id).is_some());
}

#[test]
fn emit_walker_populate_data_table_symbol_name_generation() {
    let mut arena = IrArena::new();

    let lit_id = arena.alloc(IrKind::Literal, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);
    arena.literal_values_mut().insert(lit_id, 42i64);

    let mut data_table = DataSideTable::new();
    EmitWalker::populate_data_table(&arena, &mut data_table);

    let entry = data_table.get(let_id).expect("data entry should exist");
    // Symbol name should be generated as data_<node_id>
    assert!(entry.symbol_name.starts_with("data_"));
    assert!(entry.symbol_name.contains(&let_id.get().to_string()));
}

// ── Phase 6 m5-002 Data table routing tests (uninit + immutable/mutable) ──────────────────────────

#[test]
fn emit_walker_populate_data_table_immutable_literal_routes_to_rodata() {
    let mut arena = IrArena::new();

    // Allocate: immutable Let with Literal RHS
    let lit_id = arena.alloc(IrKind::Literal, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);
    arena
        .literal_values_mut()
        .insert(lit_id, 0x1234567890ABCDEF);

    // Do NOT register as mutable (defaults to false).

    let mut data_table = DataSideTable::new();
    EmitWalker::populate_data_table(&arena, &mut data_table);

    let entry = data_table.get(let_id).expect("data entry should exist");
    assert_eq!(entry.section, SectionKind::Rodata);
    assert_eq!(entry.size_hint, 8);
    assert!(!entry.bytes.is_empty());
}

#[test]
fn emit_walker_populate_data_table_mutable_literal_routes_to_data() {
    let mut arena = IrArena::new();

    // Allocate: mutable Let with Literal RHS
    let lit_id = arena.alloc(IrKind::Literal, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lit_id]);
    arena
        .literal_values_mut()
        .insert(lit_id, 0xFEDCBA0987654321u64 as i64);

    // Register as mutable
    arena
        .let_meta_mut()
        .insert(let_id, paideia_as_ir::LetInfo::mutable());

    let mut data_table = DataSideTable::new();
    EmitWalker::populate_data_table(&arena, &mut data_table);

    let entry = data_table.get(let_id).expect("data entry should exist");
    assert_eq!(entry.section, SectionKind::Data);
    assert_eq!(entry.size_hint, 8);
    assert!(!entry.bytes.is_empty());
}

#[test]
fn emit_walker_populate_data_table_mutable_uninit_routes_to_bss() {
    let mut arena = IrArena::new();

    // Allocate: mutable Let with Placeholder RHS (uninit marker)
    let uninit_id = arena.alloc(IrKind::Placeholder, span());
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [uninit_id]);

    // Register as mutable
    arena
        .let_meta_mut()
        .insert(let_id, paideia_as_ir::LetInfo::mutable());

    let mut data_table = DataSideTable::new();
    EmitWalker::populate_data_table(&arena, &mut data_table);

    let entry = data_table.get(let_id).expect("data entry should exist");
    assert_eq!(entry.section, SectionKind::Bss);
    assert_eq!(entry.size_hint, 8);
    assert!(entry.bytes.is_empty());
}

#[test]
fn emit_walker_populate_data_table_immutable_placeholder_routed_to_bss() {
    let mut arena = IrArena::new();

    // Allocate: immutable Let with Placeholder RHS
    let uninit_id = arena.alloc(IrKind::Placeholder, span());
    let _let_id = arena.alloc_with_children(IrKind::Let, span(), [uninit_id]);

    // Do NOT register as mutable (defaults to false).

    let mut data_table = DataSideTable::new();
    EmitWalker::populate_data_table(&arena, &mut data_table);

    // Phase 6 m5-004: Immutable + Placeholder is now routed to .bss
    // (supports `let x = uninit` at module level, even though module-level doesn't support `let mut`)
    assert_eq!(data_table.len(), 1);
    let entry = data_table.iter().next().expect("should have one entry");
    assert_eq!(entry.1.section, SectionKind::Bss);
}

#[test]
fn emit_walker_populate_data_table_rodata_bss_coexist() {
    let mut arena = IrArena::new();

    // Allocate: immutable Let-Literal (→ Rodata)
    let lit1_id = arena.alloc(IrKind::Literal, span());
    let let1_id = arena.alloc_with_children(IrKind::Let, span(), [lit1_id]);
    arena
        .literal_values_mut()
        .insert(lit1_id, 0x0011223344556677);

    // Allocate: mutable Let-Uninit (→ Bss)
    let uninit_id = arena.alloc(IrKind::Placeholder, span());
    let let2_id = arena.alloc_with_children(IrKind::Let, span(), [uninit_id]);
    arena
        .let_meta_mut()
        .insert(let2_id, paideia_as_ir::LetInfo::mutable());

    let mut data_table = DataSideTable::new();
    EmitWalker::populate_data_table(&arena, &mut data_table);

    assert_eq!(data_table.len(), 2);
    let rodata_entry = data_table.get(let1_id).expect("rodata entry should exist");
    let bss_entry = data_table.get(let2_id).expect("bss entry should exist");

    assert_eq!(rodata_entry.section, SectionKind::Rodata);
    assert_eq!(bss_entry.section, SectionKind::Bss);
    assert!(!rodata_entry.bytes.is_empty());
    assert!(bss_entry.bytes.is_empty());
}

#[test]
fn emit_walker_populate_data_table_mutable_data_rodata_coexist() {
    let mut arena = IrArena::new();

    // Allocate: immutable Let-Literal (→ Rodata)
    let lit1_id = arena.alloc(IrKind::Literal, span());
    let let1_id = arena.alloc_with_children(IrKind::Let, span(), [lit1_id]);
    arena
        .literal_values_mut()
        .insert(lit1_id, 0xAAAAAAAAAAAAAAAAu64 as i64);

    // Allocate: mutable Let-Literal (→ Data)
    let lit2_id = arena.alloc(IrKind::Literal, span());
    let let2_id = arena.alloc_with_children(IrKind::Let, span(), [lit2_id]);
    arena
        .literal_values_mut()
        .insert(lit2_id, 0xBBBBBBBBBBBBBBBBu64 as i64);
    arena
        .let_meta_mut()
        .insert(let2_id, paideia_as_ir::LetInfo::mutable());

    let mut data_table = DataSideTable::new();
    EmitWalker::populate_data_table(&arena, &mut data_table);

    assert_eq!(data_table.len(), 2);
    let rodata_entry = data_table.get(let1_id).expect("rodata entry should exist");
    let data_entry = data_table.get(let2_id).expect("data entry should exist");

    assert_eq!(rodata_entry.section, SectionKind::Rodata);
    assert_eq!(data_entry.section, SectionKind::Data);
    assert_eq!(rodata_entry.size_hint, 8);
    assert_eq!(data_entry.size_hint, 8);
}
