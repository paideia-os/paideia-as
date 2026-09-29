//! PAS-DEBT-B4-002 Slice C (paideia-as#1554): byte-shape tests for
//! the callee-side sret splice and the caller-side persistent-slot
//! path.
//!
//! These tests exercise:
//!
//!   * `emit_ret` splices `sub rsp, N` + the aggregate-return helper's
//!     instruction stream when the current function's Symbol has a
//!     `return_record_layout` (Memory + register-return SysV, Memory
//!     MS).
//!   * `emit_call.rs` swaps the Slice B transient `sub/lea/add`
//!     triplet for a persistent `lea rdi, [rbp - disp]` (SysV) /
//!     `lea rcx, [rbp - disp]` (MS) when the App node has a
//!     `CallerSretSlot` entry — and the caller's frame prologue
//!     carries the matching `sub rsp, bump`.
//!   * Register-return callers with no persistent slot allocated
//!     stay byte-identical to the Slice B / scalar path (regression
//!     guard).
//!
//! Byte-exactness is asserted at the instruction-shape level
//! (mnemonic + operand pattern), mirroring `sret_call_wiring.rs`'s
//! approach. The `aggregate_return.rs` tests already pin the
//! encoded bytes for each helper's output.

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

fn ms_memory_16b_layout() -> RecordLayout {
    RecordLayout::new(
        16,
        8,
        vec![
            FieldLayout { offset: 0, size: 8, signed: false, is_float: false },
            FieldLayout { offset: 8, size: 8, signed: false, is_float: false },
        ],
    )
}

/// Extract instructions owned by a specific lambda, in emission order.
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

// ── Callee-side sret splice (SysV Memory) ─────────────────────────────

/// A callee whose Symbol carries a 24 B Memory-classified
/// `return_record_layout` must emit, before its frame teardown:
///   sub rsp, 32
///   mov r10, [rsp+0]; mov [rdi+0],  r10
///   mov r10, [rsp+8]; mov [rdi+8],  r10
///   mov r10, [rsp+16]; mov [rdi+16], r10
///   mov rax, rdi
/// then the standard `mov rsp, rbp; pop rbp; ret` tail.
#[test]
fn sysv_memory_callee_emit_ret_splices_sret_store() {
    let mut arena = IrArena::new();

    // Callee body: bare literal 0 (matches Slice B fixture bodies).
    let body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(body, 0);
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [body]);

    // Stamp a Symbol with `return_record_layout = Some(memory_24b)`.
    let mut sym = Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    sym.return_record_layout = Some(memory_24b_layout());
    arena.symbols_mut().insert(sym);

    let mut walker = EmitWalker::new();
    // Keep the default frame prologue on — Slice C requires the
    // frame pointer for the callee-side splice (RBP anchors both
    // the source buffer allocation and the teardown that releases
    // it). Do NOT mark `no_frame`.
    walker.walk(&mut arena);

    let insts = insts_for(&walker, callee_lambda);
    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();

    // Frame prologue: push rbp; mov rbp, rsp.
    assert_eq!(mnems[0], Mnemonic::Push);
    assert_eq!(mnems[1], Mnemonic::Mov);

    // Body-shape emission for the Literal arm: mov rax, 0 (single
    // instruction). Then the sret splice: sub rsp, 32; 3× (mov, mov);
    // mov rax, rdi. Then teardown: mov rsp, rbp; pop rbp; ret.
    //
    // Sanity: RET is present exactly once at the tail.
    assert_eq!(
        insts.last().unwrap().mnemonic,
        Mnemonic::Ret,
        "callee tail must be RET"
    );
    let ret_count = mnems.iter().filter(|m| **m == Mnemonic::Ret).count();
    assert_eq!(ret_count, 1);

    // Locate the sret `sub rsp, 32` and assert its operands.
    let sret_sub_idx = insts
        .iter()
        .enumerate()
        .find(|(_, i)| {
            i.mnemonic == Mnemonic::Sub
                && matches!(i.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
                && matches!(i.operands.get(1), Some(Operand::Imm64(32)))
        })
        .map(|(idx, _)| idx)
        .expect("sret source-buffer sub rsp, 32 must be present");

    // After the sub, expect 6 MOV instructions (3× load+store pairs)
    // and then `mov rax, rdi`.
    for offset in 1..=6 {
        assert_eq!(insts[sret_sub_idx + offset].mnemonic, Mnemonic::Mov);
    }
    // 7th after sub: mov rax, rdi
    let rax_rdi = &insts[sret_sub_idx + 7];
    assert_eq!(rax_rdi.mnemonic, Mnemonic::Mov);
    match (&rax_rdi.operands[0], &rax_rdi.operands[1]) {
        (Operand::Reg(rax), Operand::Reg(rdi)) => {
            assert_eq!(*rax, abi::RAX);
            assert_eq!(*rdi, abi::RDI);
        }
        other => panic!("expected `mov rax, rdi`, got {:?}", other),
    }
}

// ── Callee-side sret splice (SysV IntPair register-return) ────────────

/// A callee returning a 16 B `{ u64, u64 }` classifies as IntPair.
/// `emit_ret` splices `sub rsp, 16` + `mov rax, [rsp+0]; mov rdx, [rsp+8]`
/// before the frame teardown. RAX/RDX carry the return pair per SysV.
#[test]
fn sysv_intpair_callee_emit_ret_splices_pair_load() {
    let mut arena = IrArena::new();

    let body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(body, 0);
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [body]);

    let mut sym = Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    sym.return_record_layout = Some(int_pair_16b_layout());
    arena.symbols_mut().insert(sym);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let insts = insts_for(&walker, callee_lambda);
    // Find the sret `sub rsp, 16`.
    let sret_sub_idx = insts
        .iter()
        .enumerate()
        .find(|(_, i)| {
            i.mnemonic == Mnemonic::Sub
                && matches!(i.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
                && matches!(i.operands.get(1), Some(Operand::Imm64(16)))
        })
        .map(|(idx, _)| idx)
        .expect("sret source-buffer sub rsp, 16 must be present for IntPair");

    // Next two insts: `mov rax, [rsp+0]` and `mov rdx, [rsp+8]`.
    let mov_rax = &insts[sret_sub_idx + 1];
    let mov_rdx = &insts[sret_sub_idx + 2];
    assert_eq!(mov_rax.mnemonic, Mnemonic::Mov);
    match (&mov_rax.operands[0], &mov_rax.operands[1]) {
        (Operand::Reg(r), Operand::MemSib { base, disp, .. }) => {
            assert_eq!(*r, abi::RAX);
            assert_eq!(*base, abi::RSP);
            assert_eq!(*disp, 0);
        }
        other => panic!("expected `mov rax, [rsp+0]`, got {:?}", other),
    }
    assert_eq!(mov_rdx.mnemonic, Mnemonic::Mov);
    match (&mov_rdx.operands[0], &mov_rdx.operands[1]) {
        (Operand::Reg(r), Operand::MemSib { base, disp, .. }) => {
            assert_eq!(*r, abi::RDX);
            assert_eq!(*base, abi::RSP);
            assert_eq!(*disp, 8);
        }
        other => panic!("expected `mov rdx, [rsp+8]`, got {:?}", other),
    }
}

// ── Callee-side sret splice (MS Memory) ───────────────────────────────

/// MS callee returning a 16 B aggregate → Memory placement. The sret
/// store copies from `[rsp+0]` into `[rcx+0]`, `[rcx+8]`, then
/// `mov rax, rcx`.
#[test]
fn ms_memory_callee_emit_ret_splices_sret_store_via_rcx() {
    let mut arena = IrArena::new();

    let body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(body, 0);
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [body]);

    let mut sym = Symbol::new_with_abi(
        "callee".to_string(),
        SymbolKind::Function,
        callee_lambda,
        Some(CallingConvention::Ms),
    );
    sym.return_record_layout = Some(ms_memory_16b_layout());
    arena.symbols_mut().insert(sym);

    let mut walker = EmitWalker::new();
    // MS callee needs its ABI known so emit_ret classifies via MS
    // placement, not the SysV default.
    walker
        .state_mut()
        .insert_lambda_abi(callee_lambda.get(), CallingConvention::Ms);
    walker.walk(&mut arena);

    let insts = insts_for(&walker, callee_lambda);
    // Locate `sub rsp, 16` (16 B aggregate, aligned to 16 already).
    let sret_sub_idx = insts
        .iter()
        .enumerate()
        .find(|(_, i)| {
            i.mnemonic == Mnemonic::Sub
                && matches!(i.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
                && matches!(i.operands.get(1), Some(Operand::Imm64(16)))
        })
        .map(|(idx, _)| idx)
        .expect("sret source-buffer sub rsp, 16 must be present for MS Memory");

    // After the sub: 4 MOVs (2 × load+store) + `mov rax, rcx`.
    for offset in 1..=4 {
        assert_eq!(insts[sret_sub_idx + offset].mnemonic, Mnemonic::Mov);
    }
    let rax_rcx = &insts[sret_sub_idx + 5];
    assert_eq!(rax_rcx.mnemonic, Mnemonic::Mov);
    match (&rax_rcx.operands[0], &rax_rcx.operands[1]) {
        (Operand::Reg(rax), Operand::Reg(rcx)) => {
            assert_eq!(*rax, abi::RAX);
            assert_eq!(*rcx, abi::RCX);
        }
        other => panic!("expected `mov rax, rcx`, got {:?}", other),
    }
}

// ── Scalar-return regression: no callee sret splice ───────────────────

/// A callee whose Symbol has `return_record_layout = None` must
/// emit NO sret splice — the tail is just the frame teardown plus
/// RET. Pins that the pre-Slice-C behaviour is preserved for every
/// non-record-returning function in the corpus.
#[test]
fn scalar_return_callee_no_sret_splice() {
    let mut arena = IrArena::new();

    let body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(body, 42);
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [body]);

    let sym = Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    // NO return_record_layout stamped.
    arena.symbols_mut().insert(sym);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let insts = insts_for(&walker, callee_lambda);

    // Assert the tail is exactly: mov rsp, rbp; pop rbp; ret. No sret
    // `sub rsp, N` or `mov rax, rdi/rcx` between the body's `mov rax, 42`
    // and the teardown.
    let ret_idx = insts
        .iter()
        .position(|i| i.mnemonic == Mnemonic::Ret)
        .expect("callee must terminate with RET");
    // The three insts immediately before RET must be the body's
    // `mov rax, 42` (or its position depends on body arm) and the
    // teardown pair. Assert: no `sub rsp` between the body and RET
    // that could be an sret allocation.
    for inst in &insts[..ret_idx] {
        if inst.mnemonic == Mnemonic::Sub {
            match inst.operands.first() {
                Some(Operand::Reg(r)) if *r == abi::RSP => {
                    panic!(
                        "scalar-return callee must NOT emit an rsp sub (sret allocation): {:?}",
                        inst
                    );
                }
                _ => {}
            }
        }
    }
}

// ── Caller-side persistent-slot path (SysV Memory) ────────────────────

/// A caller with a `CallerSretSlotTable` entry for its App emits a
/// single LEA at `[rbp - disp]` in place of Slice B's transient
/// `sub/lea/add` triplet. The caller's frame prologue carries the
/// matching `sub rsp, bump`; `mov rsp, rbp` at RET releases the
/// area (no post-CALL `add rsp`).
#[test]
fn caller_persistent_slot_emits_lea_rbp_relative_no_transient_sub_add() {
    let mut arena = IrArena::new();

    // Callee body: bare literal 0.
    let callee_body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(callee_body, 0);
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [callee_body]);
    let mut callee_sym =
        Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    callee_sym.return_record_layout = Some(memory_24b_layout());
    arena.symbols_mut().insert(callee_sym);

    // Caller body: App(callee_var, 11, 22). Two literal args → 2
    // register moves.
    let callee_var = arena.alloc(IrKind::Var, span());
    let arg0 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0, 11);
    let arg1 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1, 22);
    let app = arena.alloc_with_children(IrKind::App, span(), [callee_var, arg0, arg1]);
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

    // Simulate the pass output: install the caller-side slot and
    // per-caller bump for this App / caller Lambda.
    arena
        .caller_sret_slot_table_mut()
        .insert(app, CallerSretSlot::new(-32, 32));
    arena
        .caller_sret_frame_bump_table_mut()
        .insert(caller_lambda, 32);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let insts = insts_for(&walker, caller_lambda);

    // Prologue: push rbp; mov rbp, rsp; sub rsp, 32 (Slice C caller
    // sret bump). Assert the bump is present.
    let mut saw_prologue_bump = false;
    for (idx, inst) in insts.iter().enumerate() {
        if inst.mnemonic == Mnemonic::Sub
            && matches!(inst.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
            && matches!(inst.operands.get(1), Some(Operand::Imm64(32)))
        {
            // Confirm it appears BEFORE any Lea (the sret LEA fires
            // between the bump and the CALL).
            let lea_before = insts[..idx].iter().any(|i| i.mnemonic == Mnemonic::Lea);
            if !lea_before {
                saw_prologue_bump = true;
                break;
            }
        }
    }
    assert!(
        saw_prologue_bump,
        "expected `sub rsp, 32` in caller prologue (before the sret LEA); insts: {:#?}",
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );

    // sret LEA: `lea rdi, [rbp - 32]`.
    let lea_idx = insts
        .iter()
        .position(|i| {
            i.mnemonic == Mnemonic::Lea
                && matches!(i.operands.first(), Some(Operand::Reg(r)) if *r == abi::RDI)
                && matches!(
                    i.operands.get(1),
                    Some(Operand::MemSib { base, disp, .. }) if *base == abi::RBP && *disp == -32
                )
        })
        .expect("expected `lea rdi, [rbp-32]` (persistent slot LEA)");

    // No transient sret allocation: assert there is NO
    // `sub rsp, 32` between the LEA and the CALL (would indicate a
    // Slice B triplet leak).
    let call_idx = insts
        .iter()
        .position(|i| i.mnemonic == Mnemonic::Call)
        .expect("CALL must be present");
    for inst in &insts[lea_idx + 1..call_idx] {
        if inst.mnemonic == Mnemonic::Sub {
            if let Some(Operand::Reg(r)) = inst.operands.first() {
                assert_ne!(
                    *r, abi::RSP,
                    "no rsp Sub between persistent-slot LEA and CALL: {:?}",
                    inst
                );
            }
        }
    }

    // No post-CALL `add rsp, 32` (Slice B release skipped when
    // persistent slot is in play).
    let ret_idx = insts.iter().position(|i| i.mnemonic == Mnemonic::Ret).unwrap();
    for inst in &insts[call_idx + 1..ret_idx] {
        if inst.mnemonic == Mnemonic::Add {
            if let Some(Operand::Reg(r)) = inst.operands.first() {
                if let Some(Operand::Imm64(v)) = inst.operands.get(1) {
                    // The `add rsp, 32` release we're guarding against.
                    // Other `add rsp, N` (MS shadow-space etc.) don't
                    // apply here — this test is SysV with no ABI
                    // annotation. Still, be strict.
                    assert!(
                        !(*r == abi::RSP && *v == 32),
                        "no post-CALL `add rsp, 32` when persistent slot is used: {:?}",
                        inst
                    );
                }
            }
        }
    }
}

// ── Slice B fallback preserved when no persistent slot ────────────────

/// A record-returning caller whose App has NO entry in
/// `CallerSretSlotTable` (the pre-Slice-C corpus) must fall back to
/// Slice B behavior byte-for-byte: transient `sub rsp, N`,
/// `lea rdi, [rsp]`, `add rsp, N` after CALL. This test is the
/// belt-and-suspenders pin that keeps Slice B's fixture tests
/// (`sysv_memory_24b_emits_sret_sub_lea_arg_shift_add` and
/// friends) green.
#[test]
fn slice_b_fallback_when_no_caller_slot() {
    let mut arena = IrArena::new();

    let callee_body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(callee_body, 0);
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [callee_body]);
    let mut callee_sym =
        Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    callee_sym.return_record_layout = Some(memory_24b_layout());
    arena.symbols_mut().insert(callee_sym);

    let callee_var = arena.alloc(IrKind::Var, span());
    let arg0 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0, 11);
    let arg1 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1, 22);
    let app = arena.alloc_with_children(IrKind::App, span(), [callee_var, arg0, arg1]);
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

    // Deliberately do NOT insert into caller_sret_slot_table /
    // caller_sret_frame_bump_table (simulates the pre-Slice-C
    // corpus).
    //
    // Also mark @no_frame so we get a clean transient-slot sequence
    // without prologue/epilogue noise — matching Slice B tests.
    let mut walker = EmitWalker::new();
    walker.state_mut().mark_lambda_no_frame(callee_lambda.get());
    walker.state_mut().mark_lambda_no_frame(caller_lambda.get());
    walker.walk(&mut arena);

    let insts = insts_for(&walker, caller_lambda);

    // Slice B transient sequence: sub rsp, 32; lea rdi, [rsp+0]; mov rsi, 11; mov rdx, 22; call; add rsp, 32; ret.
    let mnems: Vec<Mnemonic> = insts.iter().map(|i| i.mnemonic).collect();
    assert_eq!(
        mnems,
        vec![
            Mnemonic::Sub,
            Mnemonic::Lea,
            Mnemonic::Mov,
            Mnemonic::Mov,
            Mnemonic::Call,
            Mnemonic::Add,
            Mnemonic::Ret,
        ],
        "Slice B fallback must emit transient sub/lea/mov/mov/call/add/ret"
    );

    // sret LEA must reference RSP (not RBP) in the fallback path.
    match (&insts[1].operands[0], &insts[1].operands[1]) {
        (Operand::Reg(r), Operand::MemSib { base, disp, .. }) => {
            assert_eq!(*r, abi::RDI, "Slice B fallback LEA target must be RDI");
            assert_eq!(
                *base,
                abi::RSP,
                "Slice B fallback LEA base must be RSP (transient slot)"
            );
            assert_eq!(*disp, 0);
        }
        other => panic!("unexpected Slice B LEA operands: {:?}", other),
    }
}

// ── paideia-as#1559 Gap B: skip_sret_splice gate ──────────────────────

/// A callee whose Symbol carries a record-return layout AND
/// `skip_sret_splice = true` must NOT emit the callee-side sret
/// splice — no `sub rsp, padded_size`, no per-eightbyte load/store,
/// no `mov rax, rdi`. The Lambda's body is expected to have already
/// placed the return value into the ABI-appropriate registers or
/// sret buffer.
///
/// Pins the Wave 51 gate: when the flag is set the historical Slice C
/// splice is suppressed. Complements the two sibling assertions
/// above (splice fires when the flag is default-false on Memory /
/// IntPair placements).
#[test]
fn sysv_memory_callee_with_skip_sret_splice_omits_slice_c_splice() {
    let mut arena = IrArena::new();

    let body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(body, 0);
    let callee_lambda = arena.alloc_with_children(IrKind::Lambda, span(), [body]);

    // Stamp the layout AND the skip flag.
    let mut sym = Symbol::new("callee".to_string(), SymbolKind::Function, callee_lambda);
    sym.return_record_layout = Some(memory_24b_layout());
    sym.skip_sret_splice = true;
    arena.symbols_mut().insert(sym);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let insts = insts_for(&walker, callee_lambda);

    // No `sub rsp, 32` (the sret source-buffer allocation the splice
    // would have emitted for a 24 B Memory-placed layout, padded to
    // 32 B for SysV mod-16). Any other `sub rsp, …` in this test
    // fixture is out of scope — the body is a bare Literal, so the
    // only rsp-touching sub would be the sret allocation.
    for inst in &insts {
        if inst.mnemonic == Mnemonic::Sub {
            match inst.operands.first() {
                Some(Operand::Reg(r)) if *r == abi::RSP => {
                    panic!(
                        "skip_sret_splice=true must suppress Slice C's sret \
                         sub rsp allocation, got: {:?}",
                        inst
                    );
                }
                _ => {}
            }
        }
    }

    // Also assert the fingerprint `mov rax, rdi` of the sret store
    // tail is absent — the body is `Literal 0` which would emit
    // `mov rax, 0`, distinguishable from `mov rax, rdi`.
    for inst in &insts {
        if inst.mnemonic == Mnemonic::Mov && inst.operands.len() == 2 {
            if let (Operand::Reg(r0), Operand::Reg(r1)) =
                (&inst.operands[0], &inst.operands[1])
            {
                assert!(
                    !(*r0 == abi::RAX && *r1 == abi::RDI),
                    "skip_sret_splice=true must suppress the `mov rax, rdi` \
                     that tails Slice C's Memory-placement splice"
                );
            }
        }
    }
}

/// The Wave-51 gate is *per-Symbol*: a distinct callee with
/// `skip_sret_splice = false` still receives the splice, even in the
/// same walker session. Guards against the gate being globally hoisted
/// (a state-bag misplacement that would silently disable the splice
/// for every callee once any one asks for it).
#[test]
fn skip_sret_splice_is_per_callee_not_global() {
    let mut arena = IrArena::new();

    // Callee A: skip_sret_splice = true (splice suppressed).
    let body_a = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(body_a, 0);
    let callee_a = arena.alloc_with_children(IrKind::Lambda, span(), [body_a]);
    let mut sym_a = Symbol::new("callee_a".to_string(), SymbolKind::Function, callee_a);
    sym_a.return_record_layout = Some(memory_24b_layout());
    sym_a.skip_sret_splice = true;
    arena.symbols_mut().insert(sym_a);

    // Callee B: skip_sret_splice = false (splice fires).
    let body_b = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(body_b, 0);
    let callee_b = arena.alloc_with_children(IrKind::Lambda, span(), [body_b]);
    let mut sym_b = Symbol::new("callee_b".to_string(), SymbolKind::Function, callee_b);
    sym_b.return_record_layout = Some(memory_24b_layout());
    // skip_sret_splice defaults to false — leave it.
    arena.symbols_mut().insert(sym_b);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);

    let insts_a = insts_for(&walker, callee_a);
    let insts_b = insts_for(&walker, callee_b);

    // Callee A must NOT contain an rsp-directed sub with imm 32.
    for inst in &insts_a {
        if inst.mnemonic == Mnemonic::Sub
            && matches!(inst.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
            && matches!(inst.operands.get(1), Some(Operand::Imm64(32)))
        {
            panic!(
                "callee_a with skip_sret_splice=true must not carry the sret sub rsp,32"
            );
        }
    }

    // Callee B must contain exactly one `sub rsp, 32` (the sret
    // source-buffer allocation).
    let b_sub_count = insts_b
        .iter()
        .filter(|i| {
            i.mnemonic == Mnemonic::Sub
                && matches!(i.operands.first(), Some(Operand::Reg(r)) if *r == abi::RSP)
                && matches!(i.operands.get(1), Some(Operand::Imm64(32)))
        })
        .count();
    assert_eq!(
        b_sub_count, 1,
        "callee_b with skip_sret_splice=false must still receive the sret splice \
         even when a sibling callee opted out"
    );
}
