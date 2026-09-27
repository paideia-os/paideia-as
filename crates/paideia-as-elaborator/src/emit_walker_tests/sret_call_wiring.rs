//! PAS-DEBT-B4-002 Slice B (paideia-as#1554): byte-shape tests for the
//! caller-side sret wiring at `emit_call.rs`.
//!
//! These tests exercise the branch introduced by Slice B — probing the
//! callee's `Symbol::return_record_layout`, deciding an aggregate-return
//! shape via `abi::sysv_return_placement_from_layout` /
//! `abi::ms_return_placement_from_layout`, and emitting the caller-side
//! sret prelude/postlude when the placement `needs_hidden_sret`.
//!
//! Byte-exactness is asserted at the instruction-shape layer rather than
//! encoded bytes: the aggregate-return helper's own tests
//! (`aggregate_return.rs`) already pin the encoded bytes for each
//! helper's output. What these tests verify is:
//!
//!   1. Whether the sret sub/lea/add triplet actually got spliced into
//!      the emission stream for a Memory-classified aggregate return.
//!   2. Whether real arg registers shift right by one (RDI→RSI, or
//!      RCX→RDX for MS).
//!   3. Whether register-return placements leave the CALL emission
//!      byte-identical to the scalar path (no sret triplet).
//!
//! The three fixtures under `tests/data/sret_slice_b/` mirror the ABI
//! shapes named in the Slice B scope: 16-byte pair (SysV IntPair
//! register-return), 24-byte Memory (SysV sret), and 16-byte MS Memory
//! (RCX sret path).

use super::super::*;
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::let_meta::CallingConvention;
use paideia_as_ir::record_layout::{FieldLayout, RecordLayout};
use paideia_as_ir::{CallMeta, Symbol, SymbolKind};

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Build a `RecordLayout` for `{ u64 a; u64 b }` — SysV classifies as
/// `[Integer, Integer]` → `IntPair` (RAX+RDX register-return).
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

/// Build a `RecordLayout` for `{ u64 a; u64 b; u64 c }` — 24 bytes,
/// SysV classifies as Memory (>16 B).
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

/// Build a `RecordLayout` for an aggregate MS x64 will classify as
/// Memory: 16 bytes (not a single-register size 1/2/4/8, and not a
/// single-float wrapper).
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

/// Assemble a minimal two-lambda arena: `callee(a, b) -> record { ... }`
/// (RHS body = literal 0 — the point is the symbol carrying
/// `return_record_layout`, not the actual body), and
/// `caller() -> callee(11, 22)`.
///
/// `layout_opt` — pass Some(layout) to stamp
/// `Symbol::return_record_layout`; None models the scalar-return
/// regression case.
///
/// `callee_abi_opt` — pass Some(cc) to annotate the callee's ABI (drives
/// `arg_regs` pool selection on the caller side). When Some, the caller
/// is also stamped with the matching ABI to keep the bridge-save set
/// empty — tests here focus on the sret shape, not the paideia→ABI
/// bridge R15/R14 push/pop pair. See `abi::bridge_save_set` for the
/// bridge crossing rules.
///
/// Returns (walker, caller_lambda_id).
fn build_caller_callee(
    layout_opt: Option<RecordLayout>,
    callee_abi_opt: Option<CallingConvention>,
) -> (EmitWalker, IrNodeId) {
    let mut arena = IrArena::new();

    // Callee body: a bare literal so the callee lambda emits without
    // depending on any richer visit path.
    let callee_body = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(callee_body, 0);
    let callee_lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [callee_body]);

    // Register the callee symbol, optionally with a return_record_layout
    // and an ABI annotation.
    let mut callee_sym = Symbol::new_with_abi(
        "callee".to_string(),
        SymbolKind::Function,
        callee_lambda_id,
        callee_abi_opt,
    );
    callee_sym.return_record_layout = layout_opt;
    arena.symbols_mut().insert(callee_sym);

    // Caller: two literal args, App(callee, 11, 22).
    let arg0 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg0, 11);
    let arg1 = arena.alloc(IrKind::Literal, span());
    arena.literal_values_mut().insert(arg1, 22);
    let callee_var = arena.alloc(IrKind::Var, span());
    let app_id = arena.alloc_with_children(
        IrKind::App,
        span(),
        [callee_var, arg0, arg1],
    );
    let caller_lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [app_id]);

    arena.call_sites_mut().insert(
        app_id,
        CallMeta {
            callee_name: "callee".to_string(),
            arg_count: 2,
            is_intrinsic: false,
        },
    );

    let mut walker = EmitWalker::new();
    // Bare Lambda → suppress default frame-pointer emission so tests
    // read a clean prelude/postlude sequence without the surrounding
    // push rbp / mov rbp, rsp / pop rbp / mov rsp, rbp noise.
    walker.state_mut().mark_lambda_no_frame(callee_lambda_id.get());
    walker.state_mut().mark_lambda_no_frame(caller_lambda_id.get());
    // If the callee is ABI-annotated, plumb its ABI into the walker
    // state — `emit_call_args_and_call` reads `arena.symbols()` for
    // the callee's ABI, but the caller's own ABI comes from
    // `state.lambda_abi_option`. Stamp the caller with the SAME
    // ABI so `bridge_save_set` returns empty (avoids the paideia→ABI
    // R15/R14 push/pop pair that would otherwise clutter the emission
    // stream and force the tests to reason about a bridge crossing
    // orthogonal to the sret shape under test).
    if let Some(cc) = callee_abi_opt {
        walker.state_mut().insert_lambda_abi(callee_lambda_id.get(), cc);
        walker.state_mut().insert_lambda_abi(caller_lambda_id.get(), cc);
    }
    walker.walk(&mut arena);

    (walker, caller_lambda_id)
}

/// Extract the instructions emitted for `lambda_id`, in emission order.
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

// ── SysV Memory (sret) — 24 B `{ u64, u64, u64 }` ──────────────────────

#[test]
fn sysv_memory_24b_emits_sret_sub_lea_arg_shift_add() {
    // Caller: `callee(11, 22)` where callee returns a 24 B record.
    // Expected caller-side sequence (bare, no frame):
    //   sub rsp, 32          ; sret slot (24 padded to 16-multiple = 32)
    //   lea rdi, [rsp]       ; sret pointer
    //   mov rsi, 11          ; real arg 0 shifted to RSI
    //   mov rdx, 22          ; real arg 1 shifted to RDX
    //   call callee
    //   add rsp, 32          ; release sret slot
    //   ret
    let (walker, caller_id) = build_caller_callee(Some(memory_24b_layout()), None);

    let insts = insts_for(&walker, caller_id);
    // Slice B baseline for this shape: sub, lea, mov, mov, call, add, ret = 7 insts.
    assert_eq!(
        insts.len(),
        7,
        "expected 7 caller insts for SysV 24B sret call, got {}: {:#?}",
        insts.len(),
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );

    // [0] sub rsp, 32
    assert_eq!(insts[0].mnemonic, Mnemonic::Sub);
    match (&insts[0].operands[0], &insts[0].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RSP);
            assert_eq!(*v, 32);
        }
        other => panic!("unexpected sret-sub operands: {:?}", other),
    }

    // [1] lea rdi, [rsp]
    assert_eq!(insts[1].mnemonic, Mnemonic::Lea);
    match (&insts[1].operands[0], &insts[1].operands[1]) {
        (Operand::Reg(r), Operand::MemSib { base, disp, .. }) => {
            assert_eq!(*r, abi::RDI);
            assert_eq!(*base, abi::RSP);
            assert_eq!(*disp, 0);
        }
        other => panic!("unexpected sret-lea operands: {:?}", other),
    }

    // [2] mov rsi, 11  (real arg 0, shifted from RDI to RSI)
    assert_eq!(insts[2].mnemonic, Mnemonic::Mov);
    match (&insts[2].operands[0], &insts[2].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RSI, "SysV arg 0 must shift to RSI when sret occupies RDI");
            assert_eq!(*v, 11);
        }
        other => panic!("unexpected arg0 operands: {:?}", other),
    }

    // [3] mov rdx, 22  (real arg 1, shifted from RSI to RDX)
    assert_eq!(insts[3].mnemonic, Mnemonic::Mov);
    match (&insts[3].operands[0], &insts[3].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RDX, "SysV arg 1 must shift to RDX when sret occupies RDI");
            assert_eq!(*v, 22);
        }
        other => panic!("unexpected arg1 operands: {:?}", other),
    }

    // [4] call callee
    assert_eq!(insts[4].mnemonic, Mnemonic::Call);

    // [5] add rsp, 32  (release sret slot)
    assert_eq!(insts[5].mnemonic, Mnemonic::Add);
    match (&insts[5].operands[0], &insts[5].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RSP);
            assert_eq!(*v, 32);
        }
        other => panic!("unexpected sret-add operands: {:?}", other),
    }

    // [6] ret
    assert_eq!(insts[6].mnemonic, Mnemonic::Ret);
}

// ── MS Memory (sret) — 16 B ────────────────────────────────────────────

#[test]
fn ms_memory_16b_emits_sret_via_rcx_arg_shift_to_rdx_r8() {
    // MS callee: `callee(11, 22)` where callee returns a 16 B record.
    // MS classifies size=16 as Memory (only 1/2/4/8 or scalar float
    // land in a register). Expected caller-side sequence:
    //   sub rsp, N_ms        ; MS shadow-space (32 bytes minimum, plus alignment pad)
    //   sub rsp, 16          ; sret slot (16 aligned to 16 = 16)
    //   lea rcx, [rsp]       ; sret pointer
    //   mov rdx, 11          ; real arg 0 shifted from RCX to RDX
    //   mov r8, 22           ; real arg 1 shifted from RDX to R8
    //   call callee
    //   add rsp, 16          ; release sret slot
    //   add rsp, N_ms        ; release MS shadow-space
    //   ret
    let (walker, caller_id) =
        build_caller_callee(Some(ms_memory_16b_layout()), Some(CallingConvention::Ms));

    let insts = insts_for(&walker, caller_id);
    // Expect 9 insts as enumerated above.
    assert_eq!(
        insts.len(),
        9,
        "expected 9 caller insts for MS 16B sret call, got {}: {:#?}",
        insts.len(),
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );

    // [0] sub rsp, <ms_bump> — MS shadow-space; value comes from
    // `abi::MS_CALL_STACK_BUMP` (+ potential pad based on scratch
    // parity). Zero-scratch case → base MS_CALL_STACK_BUMP.
    assert_eq!(insts[0].mnemonic, Mnemonic::Sub);
    match (&insts[0].operands[0], &insts[0].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(_)) => {
            assert_eq!(*r, abi::RSP);
        }
        other => panic!("unexpected ms-prelude operands: {:?}", other),
    }

    // [1] sub rsp, 16 (sret slot)
    assert_eq!(insts[1].mnemonic, Mnemonic::Sub);
    match (&insts[1].operands[0], &insts[1].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RSP);
            assert_eq!(*v, 16);
        }
        other => panic!("unexpected sret-sub operands: {:?}", other),
    }

    // [2] lea rcx, [rsp]
    assert_eq!(insts[2].mnemonic, Mnemonic::Lea);
    match (&insts[2].operands[0], &insts[2].operands[1]) {
        (Operand::Reg(r), Operand::MemSib { base, disp, .. }) => {
            assert_eq!(*r, abi::RCX, "MS sret pointer must land in RCX");
            assert_eq!(*base, abi::RSP);
            assert_eq!(*disp, 0);
        }
        other => panic!("unexpected sret-lea operands: {:?}", other),
    }

    // [3] mov rdx, 11 (real arg 0 shifted from RCX to RDX)
    assert_eq!(insts[3].mnemonic, Mnemonic::Mov);
    match (&insts[3].operands[0], &insts[3].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RDX, "MS arg 0 must shift to RDX when sret occupies RCX");
            assert_eq!(*v, 11);
        }
        other => panic!("unexpected arg0 operands: {:?}", other),
    }

    // [4] mov r8, 22 (real arg 1 shifted from RDX to R8)
    assert_eq!(insts[4].mnemonic, Mnemonic::Mov);
    match (&insts[4].operands[0], &insts[4].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::R8, "MS arg 1 must shift to R8 when sret occupies RCX");
            assert_eq!(*v, 22);
        }
        other => panic!("unexpected arg1 operands: {:?}", other),
    }

    // [5] call callee
    assert_eq!(insts[5].mnemonic, Mnemonic::Call);

    // [6] add rsp, 16 (release sret slot)
    assert_eq!(insts[6].mnemonic, Mnemonic::Add);
    match (&insts[6].operands[0], &insts[6].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RSP);
            assert_eq!(*v, 16);
        }
        other => panic!("unexpected sret-add operands: {:?}", other),
    }

    // [7] add rsp, <ms_bump> (release MS shadow-space)
    assert_eq!(insts[7].mnemonic, Mnemonic::Add);
    match (&insts[7].operands[0], &insts[7].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(_)) => {
            assert_eq!(*r, abi::RSP);
        }
        other => panic!("unexpected ms-postlude operands: {:?}", other),
    }

    // [8] ret
    assert_eq!(insts[8].mnemonic, Mnemonic::Ret);
}

// ── SysV IntPair (register-return, 16 B) — no sret dance ───────────────

#[test]
fn sysv_intpair_16b_leaves_call_emission_byte_identical_to_scalar() {
    // A 16 B `{ u64, u64 }` classifies as `[Integer, Integer]` →
    // `IntPair` (RAX+RDX). Slice B leaves the caller-side CALL
    // emission byte-identical to the scalar path here — no sret sub /
    // lea / add spliced in. Slice C wires the register-return
    // pair-unpack when the caller has a destination buffer.
    let (walker, caller_id) = build_caller_callee(Some(int_pair_16b_layout()), None);

    let insts = insts_for(&walker, caller_id);
    // Expect the scalar-shape sequence: mov rdi, 11; mov rsi, 22; call callee; ret.
    assert_eq!(
        insts.len(),
        4,
        "expected 4 caller insts (scalar-shape) for SysV IntPair \
         register-return, got {}: {:#?}",
        insts.len(),
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );

    // [0] mov rdi, 11 — real arg 0 still in RDI (no sret shift)
    assert_eq!(insts[0].mnemonic, Mnemonic::Mov);
    match (&insts[0].operands[0], &insts[0].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RDI, "register-return placement must NOT shift arg 0 off RDI");
            assert_eq!(*v, 11);
        }
        other => panic!("unexpected arg0 operands: {:?}", other),
    }

    // [1] mov rsi, 22 — real arg 1 still in RSI
    assert_eq!(insts[1].mnemonic, Mnemonic::Mov);
    match (&insts[1].operands[0], &insts[1].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RSI);
            assert_eq!(*v, 22);
        }
        other => panic!("unexpected arg1 operands: {:?}", other),
    }

    // [2] call callee — no sret sub/add wrappers
    assert_eq!(insts[2].mnemonic, Mnemonic::Call);

    // [3] ret
    assert_eq!(insts[3].mnemonic, Mnemonic::Ret);

    // Explicit regression guard: no Sub/Add on RSP anywhere.
    for inst in &insts {
        if matches!(inst.mnemonic, Mnemonic::Sub | Mnemonic::Add) {
            if let Some(Operand::Reg(r)) = inst.operands.first() {
                assert_ne!(
                    *r, abi::RSP,
                    "register-return placement must NOT emit rsp sub/add: {:?}",
                    inst
                );
            }
        }
    }
}

// ── Scalar-return regression — no return_record_layout ─────────────────

#[test]
fn scalar_return_regression_no_sret_wiring_when_layout_absent() {
    // No `return_record_layout` on the callee's symbol at all.
    // Slice B must leave the CALL emission byte-identical to the
    // pre-Slice-B scalar path: mov rdi, 11; mov rsi, 22; call; ret.
    let (walker, caller_id) = build_caller_callee(None, None);

    let insts = insts_for(&walker, caller_id);
    assert_eq!(
        insts.len(),
        4,
        "expected 4 caller insts for scalar-return regression, got {}: {:#?}",
        insts.len(),
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );

    // Explicit regression guard: no Lea, no rsp Sub/Add anywhere.
    for inst in &insts {
        assert_ne!(
            inst.mnemonic, Mnemonic::Lea,
            "scalar-return path must NOT emit an sret LEA"
        );
        if matches!(inst.mnemonic, Mnemonic::Sub | Mnemonic::Add) {
            if let Some(Operand::Reg(r)) = inst.operands.first() {
                assert_ne!(
                    *r, abi::RSP,
                    "scalar-return path must NOT emit rsp sub/add: {:?}",
                    inst
                );
            }
        }
    }

    // Confirm arg 0 is still in RDI (unshifted).
    match (&insts[0].operands[0], &insts[0].operands[1]) {
        (Operand::Reg(r), Operand::Imm64(v)) => {
            assert_eq!(*r, abi::RDI);
            assert_eq!(*v, 11);
        }
        other => panic!("unexpected arg0 operands: {:?}", other),
    }
}

// ── Layout-driven placement composition ────────────────────────────────

#[test]
fn sysv_placement_from_layout_composes_classifier_and_reducer() {
    // Direct unit test for the new
    // `sysv_return_placement_from_layout` helper — pins that the
    // three-layer chain (RecordLayout → classifier → placement)
    // resolves the expected shapes for the two size regimes at
    // stake in Slice B (≤16 register-return, >16 Memory).
    use paideia_as_ir::abi::{sysv_return_placement_from_layout, SysvReturnPlacement};

    let pair = int_pair_16b_layout();
    assert_eq!(
        sysv_return_placement_from_layout(&pair),
        SysvReturnPlacement::IntPair,
        "{{ u64, u64 }} must resolve to IntPair"
    );

    let mem = memory_24b_layout();
    assert_eq!(
        sysv_return_placement_from_layout(&mem),
        SysvReturnPlacement::Memory,
        "24 B aggregate must resolve to Memory"
    );

    // Zero-size aggregate → None.
    let empty = RecordLayout::new(0, 1, vec![]);
    assert_eq!(
        sysv_return_placement_from_layout(&empty),
        SysvReturnPlacement::None,
        "zero-size aggregate must resolve to None"
    );
}
