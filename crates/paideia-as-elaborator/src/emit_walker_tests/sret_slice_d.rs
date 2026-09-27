//! PAS-DEBT-B4-002 Slice D (paideia-as#1554): tests for the
//! return-position record-cons materialisation (Piece 1) and the
//! lifted caller-side pair-unpack gate (Piece 2).
//!
//! Piece 1 tests exercise `emit_callee_sret_splice`'s new inline
//! buffer population: when the current Lambda's body is
//! `IrKind::RecordCons`, the splice emits `mov [rsp + offset],
//! value` for each field between its `sub rsp, padded_size` and the
//! aggregate-return helper's copy sequence.
//!
//! Piece 2 tests exercise the pair-unpack fire path in
//! `emit_call.rs`: with the pass gate lifted, a register-return
//! callee whose caller has a real frame prologue now spills
//! RAX/RDX into a persistent `[RBP + disp]` slot after CALL. The
//! `@no_frame` fallback preserves Slice B byte identity for the
//! IntPair regression test (pinned separately in
//! `sret_call_wiring.rs`).

use super::super::*;
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::let_meta::CallingConvention;
use paideia_as_ir::record_layout::{FieldLayout, RecordLayout};
use paideia_as_ir::{CallMeta, CallerSretSlot, Symbol, SymbolKind};

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

fn memory_24b_layout() -> RecordLayout {
    RecordLayout::new(
        24,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
            FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
            FieldLayout { offset: 16, size: 8, signed: false, is_float: false },
        ],
    )
}

fn int_pair_16b_layout() -> RecordLayout {
    RecordLayout::new(
        16,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
            FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
        ],
    )
}

/// Extract the instructions owned by a specific lambda in emission
/// order — mirrors the helper in `sret_slice_c.rs` verbatim.
fn insts_for(walker: &EmitWalker, lambda_id: IrNodeId) -> Vec<Instruction> {
    let target = lambda_id.get();
    let mut owned: Vec<(u32, Instruction)> = walker
        .state()
        .instructions()
        .entries()
        .iter()
        .filter_map(|(nid, inst)| {
            walker
                .state()
                .instr_to_lambda()
                .get(nid)
                .copied()
                .filter(|owner| *owner == target)
                .map(|_| (inst.emission_order, inst.clone()))
        })
        .collect();
    owned.sort_by_key(|(order, _)| *order);
    owned.into_iter().map(|(_, i)| i).collect()
}

// ── Piece 1: RecordCons body populates the sret source buffer ─────────

/// SysV Memory-classified callee whose body is
/// `Pair { a: 11, b: 22, c: 33 }`. After `sub rsp, 32` (source
/// buffer), the splice must emit three literal stores at offsets 0,
/// 8, 16 BEFORE the sret store copies from `[rsp+*]` into `[rdi+*]`.
#[test]
fn sysv_memory_record_cons_body_populates_source_buffer_from_literals() {
    let mut arena = IrArena::new();

    // Build body: RecordCons("Pair", [Literal(11), Literal(22), Literal(33)]).
    // Per `lower/record_cons.rs`'s canonicalisation, children are
    // `[type_name, values...]`; use a Var placeholder as the type
    // name (its kind is inspected only if we recurse into it, which
    // the splice never does — it iterates `children[1..]`).
    let type_name = arena.alloc(IrKind::Var, span());
    let v0 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(v0, 11);
    let v1 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(v1, 22);
    let v2 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(v2, 33);
    let record_cons = arena.alloc_with_children(
        IrKind::RecordCons,
        span(),
        [type_name, v0, v1, v2],
    );
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [record_cons]);

    // Stamp the callee symbol with the 24 B Memory layout.
    let mut sym = Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    sym.return_record_layout = Some(memory_24b_layout());
    arena.symbols_mut().insert(sym);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let insts = insts_for(&walker, callee_lambda);

    // Locate the sret source-buffer allocation `sub rsp, 32`.
    let sret_sub_idx = insts
        .iter()
        .position(|i| {
            i.mnemonic == Mnemonic::Sub
                && matches!(i.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
                && matches!(i.operands.get(1), Some(Operand::Imm64(32)))
        })
        .expect("sret source-buffer `sub rsp, 32` must be present");

    // The three field stores must fire immediately after the sub,
    // BEFORE the sret copy pairs. Assert their operand shapes at
    // sret_sub_idx + 1, +2, +3.
    let expected = [(0i32, 11i64), (8, 22), (16, 33)];
    for (i, (want_disp, want_val)) in expected.iter().enumerate() {
        let inst = &insts[sret_sub_idx + 1 + i];
        assert_eq!(inst.mnemonic, Mnemonic::Mov, "field store {i} must be Mov");
        match (&inst.operands[0], &inst.operands[1]) {
            (Operand::MemSib { base, disp, .. }, Operand::Imm64(v)) => {
                assert_eq!(*base, abi::RSP, "field store {i} base must be RSP");
                assert_eq!(*disp, *want_disp, "field store {i} disp");
                assert_eq!(*v, *want_val, "field store {i} value");
            }
            other => panic!("field store {i} unexpected operands: {:?}", other),
        }
    }
}

/// SysV IntPair (16 B, register return) callee whose body is
/// `Pair { a: 111, b: 222 }`. The splice's `sub rsp, 16` is
/// followed by two literal stores at offsets 0/8, then the
/// pair-load `mov rax, [rsp+0]; mov rdx, [rsp+8]`.
#[test]
fn sysv_intpair_record_cons_body_populates_before_pair_load() {
    let mut arena = IrArena::new();

    let type_name = arena.alloc(IrKind::Var, span());
    let v0 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(v0, 111);
    let v1 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(v1, 222);
    let record_cons = arena.alloc_with_children(
        IrKind::RecordCons,
        span(),
        [type_name, v0, v1],
    );
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [record_cons]);

    let mut sym = Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    sym.return_record_layout = Some(int_pair_16b_layout());
    arena.symbols_mut().insert(sym);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let insts = insts_for(&walker, callee_lambda);

    // `sub rsp, 16` — allocate the source buffer.
    let sret_sub_idx = insts
        .iter()
        .position(|i| {
            i.mnemonic == Mnemonic::Sub
                && matches!(i.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
                && matches!(i.operands.get(1), Some(Operand::Imm64(16)))
        })
        .expect("`sub rsp, 16` must precede the pair-load");

    // +1: `mov [rsp+0], 111` — field a.
    let s0 = &insts[sret_sub_idx + 1];
    assert_eq!(s0.mnemonic, Mnemonic::Mov);
    match (&s0.operands[0], &s0.operands[1]) {
        (Operand::MemSib { base, disp, .. }, Operand::Imm64(v)) => {
            assert_eq!(*base, abi::RSP);
            assert_eq!(*disp, 0);
            assert_eq!(*v, 111);
        }
        other => panic!("field a store unexpected: {:?}", other),
    }

    // +2: `mov [rsp+8], 222` — field b.
    let s1 = &insts[sret_sub_idx + 2];
    assert_eq!(s1.mnemonic, Mnemonic::Mov);
    match (&s1.operands[0], &s1.operands[1]) {
        (Operand::MemSib { base, disp, .. }, Operand::Imm64(v)) => {
            assert_eq!(*base, abi::RSP);
            assert_eq!(*disp, 8);
            assert_eq!(*v, 222);
        }
        other => panic!("field b store unexpected: {:?}", other),
    }

    // +3: `mov rax, [rsp+0]` — low eightbyte to RAX.
    let l0 = &insts[sret_sub_idx + 3];
    assert_eq!(l0.mnemonic, Mnemonic::Mov);
    match (&l0.operands[0], &l0.operands[1]) {
        (Operand::Reg(r), Operand::MemSib { base, disp, .. }) => {
            assert_eq!(*r, abi::RAX);
            assert_eq!(*base, abi::RSP);
            assert_eq!(*disp, 0);
        }
        other => panic!("RAX load unexpected: {:?}", other),
    }

    // +4: `mov rdx, [rsp+8]` — high eightbyte to RDX.
    let l1 = &insts[sret_sub_idx + 4];
    assert_eq!(l1.mnemonic, Mnemonic::Mov);
    match (&l1.operands[0], &l1.operands[1]) {
        (Operand::Reg(r), Operand::MemSib { base, disp, .. }) => {
            assert_eq!(*r, abi::RDX);
            assert_eq!(*base, abi::RSP);
            assert_eq!(*disp, 8);
        }
        other => panic!("RDX load unexpected: {:?}", other),
    }
}

/// Non-RecordCons body (`-> 0` Literal) — Slice C scaffolding
/// remains intact: the sret splice fires with `sub rsp, N` + the
/// helper's copy pairs, but NO field stores land between them.
/// Pins that Slice D's Piece 1 is opt-in on the body shape.
#[test]
fn literal_body_leaves_sret_source_buffer_unpopulated() {
    let mut arena = IrArena::new();

    let body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(body, 0);
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [body]);

    let mut sym = Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    sym.return_record_layout = Some(memory_24b_layout());
    arena.symbols_mut().insert(sym);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let insts = insts_for(&walker, callee_lambda);

    // Locate `sub rsp, 32`. The next instruction MUST be the sret
    // copy's `mov r10, [rsp+0]` (the first eightbyte load), NOT a
    // literal-immediate store into `[rsp + 0]`.
    let sret_sub_idx = insts
        .iter()
        .position(|i| {
            i.mnemonic == Mnemonic::Sub
                && matches!(i.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
                && matches!(i.operands.get(1), Some(Operand::Imm64(32)))
        })
        .expect("sret `sub rsp, 32` must be present");

    let next = &insts[sret_sub_idx + 1];
    // Assert the next instruction is a REG-destined MOV (the sret
    // load), not a MEM-destined MOV (which would be a Piece 1 field
    // store).
    match &next.operands[0] {
        Operand::Reg(_) => {
            // Good — sret load, buffer left unpopulated as Slice C
            // intended for non-RecordCons bodies.
        }
        Operand::MemSib { .. } => {
            panic!(
                "literal-body callee must NOT emit field stores into the sret \
                 buffer (Slice D Piece 1 is RecordCons-opt-in). Got: {:?}",
                next
            );
        }
        other => panic!("unexpected next-operand kind: {:?}", other),
    }
}

// ── Piece 2: lifted caller-side pair-unpack gate ──────────────────────

/// A caller with a frame prologue calling an IntPair register-
/// return callee now:
///   1. Gets a persistent `sub rsp, 16` in the prologue (via
///      `caller_sret_frame_bump_table`).
///   2. Emits `mov [rbp-16], rax; mov [rbp-8], rdx` after the CALL
///      (via `sysv_caller_read_return_pair`).
/// Neither happened in Slice C — the pass gate on Memory-only
/// suppressed both.
#[test]
fn sysv_intpair_caller_with_frame_emits_persistent_slot_and_pair_unpack() {
    let mut arena = IrArena::new();

    // Callee: IntPair register-return, bare literal body (splice
    // still fires but the source buffer is uninitialised — the
    // caller-side test doesn't depend on the callee's byte shape).
    let callee_body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(callee_body, 0);
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [callee_body]);
    let mut callee_sym =
        Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    callee_sym.return_record_layout = Some(int_pair_16b_layout());
    arena.symbols_mut().insert(callee_sym);

    // Caller with a real frame — do NOT `mark_lambda_no_frame`.
    // Two literal args to keep the body simple.
    let callee_var = arena.alloc(IrKind::Var, span());
    let a0 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(a0, 11);
    let a1 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(a1, 22);
    let app = arena.alloc_with_children(IrKind::App, span(), [callee_var, a0, a1]);
    arena.call_sites_mut().insert(
        app,
        CallMeta {
            callee_name: "callee".to_string(),
            arg_count: 2,
            is_intrinsic: false,
        },
    );
    let caller_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [app]);
    let caller_sym =
        Symbol::new("caller".to_string(), SymbolKind::Function, caller_lambda);
    arena.symbols_mut().insert(caller_sym);

    // Simulate the pass's Slice D output: slot + bump for the
    // register-return call site. `caller_sret_slot_table` is the
    // sret dest buffer at RBP-16.
    arena
        .caller_sret_slot_table_mut()
        .insert(app, CallerSretSlot::new(-16, 16));
    arena
        .caller_sret_frame_bump_table_mut()
        .insert(caller_lambda, 16);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let insts = insts_for(&walker, caller_lambda);

    // Assert a `sub rsp, 16` fires BEFORE the CALL (the caller
    // prologue's sret bump). Must NOT be flanked by an `add rsp, 16`
    // (that's the transient Slice-B release; persistent-slot path
    // skips it).
    let call_idx = insts
        .iter()
        .position(|i| i.mnemonic == Mnemonic::Call)
        .expect("CALL must be present");

    let mut saw_prologue_bump = false;
    for inst in &insts[..call_idx] {
        if inst.mnemonic == Mnemonic::Sub
            && matches!(inst.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
            && matches!(inst.operands.get(1), Some(Operand::Imm64(16)))
        {
            saw_prologue_bump = true;
        }
    }
    assert!(
        saw_prologue_bump,
        "Slice D IntPair caller must emit `sub rsp, 16` in its prologue"
    );

    // Post-CALL pair-unpack: `mov [rbp-16], rax` and
    // `mov [rbp-8], rdx`. Assert both are present in the post-CALL
    // range (before RET) with the exact RBP-relative displacements.
    let ret_idx = insts.iter().position(|i| i.mnemonic == Mnemonic::Ret).unwrap();
    let post_call = &insts[call_idx + 1..ret_idx];

    let has_rax_store = post_call.iter().any(|i| {
        i.mnemonic == Mnemonic::Mov
            && matches!(
                (&i.operands.get(0), &i.operands.get(1)),
                (Some(Operand::MemSib { base, disp, .. }), Some(Operand::Reg(r)))
                    if *base == abi::RBP && *disp == -16 && *r == abi::RAX
            )
    });
    let has_rdx_store = post_call.iter().any(|i| {
        i.mnemonic == Mnemonic::Mov
            && matches!(
                (&i.operands.get(0), &i.operands.get(1)),
                (Some(Operand::MemSib { base, disp, .. }), Some(Operand::Reg(r)))
                    if *base == abi::RBP && *disp == -8 && *r == abi::RDX
            )
    });
    assert!(
        has_rax_store,
        "Slice D IntPair caller must emit `mov [rbp-16], rax` after CALL. Post-call insts: {:#?}",
        post_call.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );
    assert!(
        has_rdx_store,
        "Slice D IntPair caller must emit `mov [rbp-8], rdx` after CALL"
    );

    // The persistent-slot release skip must hold: no `add rsp, 16`
    // between the CALL and the RET (the Slice B post-CALL release).
    for inst in post_call {
        if inst.mnemonic == Mnemonic::Add
            && matches!(inst.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
            && matches!(inst.operands.get(1), Some(Operand::Imm64(16)))
        {
            panic!(
                "persistent slot must not fire the Slice B `add rsp, 16` release: {:?}",
                inst
            );
        }
    }
}

/// An `@no_frame` caller of an IntPair callee falls back to Slice
/// B byte-identity: no persistent slot, no pair-unpack, no
/// prologue sret bump. Complements the pin in
/// `sret_call_wiring.rs::sysv_intpair_16b_leaves_call_emission_byte_identical_to_scalar`
/// with the explicit "slot allocated at pass time, discarded at
/// emit time" invariant.
#[test]
fn intpair_no_frame_caller_falls_back_to_slice_b_byte_identity() {
    let mut arena = IrArena::new();

    let callee_body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(callee_body, 0);
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [callee_body]);
    let mut callee_sym =
        Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    callee_sym.return_record_layout = Some(int_pair_16b_layout());
    arena.symbols_mut().insert(callee_sym);

    let callee_var = arena.alloc(IrKind::Var, span());
    let a0 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(a0, 11);
    let a1 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(a1, 22);
    let app = arena.alloc_with_children(IrKind::App, span(), [callee_var, a0, a1]);
    arena.call_sites_mut().insert(
        app,
        CallMeta {
            callee_name: "callee".to_string(),
            arg_count: 2,
            is_intrinsic: false,
        },
    );
    let caller_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [app]);
    let caller_sym =
        Symbol::new("caller".to_string(), SymbolKind::Function, caller_lambda);
    arena.symbols_mut().insert(caller_sym);

    // Slice D allocates the slot — simulate that pass output.
    arena
        .caller_sret_slot_table_mut()
        .insert(app, CallerSretSlot::new(-16, 16));
    arena
        .caller_sret_frame_bump_table_mut()
        .insert(caller_lambda, 16);

    let mut walker = EmitWalker::new();
    walker.state_mut().mark_lambda_no_frame(callee_lambda.get());
    walker.state_mut().mark_lambda_no_frame(caller_lambda.get());
    walker.walk(&mut arena);

    let insts = insts_for(&walker, caller_lambda);

    // Byte-identity with Slice B / scalar path: 4 insts total (mov
    // rdi, 11; mov rsi, 22; call; ret) — no rsp sub/add, no LEA,
    // no post-CALL mem stores.
    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![Mnemonic::Mov, Mnemonic::Mov, Mnemonic::Call, Mnemonic::Ret],
        "@no_frame caller of IntPair callee must be scalar-shape byte-identical"
    );

    for inst in &insts {
        if matches!(inst.mnemonic, Mnemonic::Sub | Mnemonic::Add) {
            if let Some(Operand::Reg(r)) = inst.operands.first() {
                assert_ne!(
                    *r, abi::RSP,
                    "@no_frame fallback must NOT emit rsp sub/add: {:?}",
                    inst
                );
            }
        }
        assert_ne!(
            inst.mnemonic, Mnemonic::Lea,
            "@no_frame fallback must NOT emit an sret LEA"
        );
    }
}
