//! paideia-as#1276 phase 3: unit tests for the default SysV frame-pointer
//! prologue/epilogue emission around `fn` lambdas — the elaborator side of
//! the wire-up that unblocks paideia-os#716 (`klog_walk_rbp` walking real
//! frame chains).
//!
//! Coverage:
//!
//! - `emit_prologue_by_default` — a function without `@no_frame` emits
//!   `push rbp; mov rbp, rsp` as its first two instructions.
//! - `no_prologue_with_no_frame` — the same function shape marked
//!   `LetInfo::no_frame = true` emits NO prologue.
//! - `epilogue_leave_ret` — the default-frame function emits
//!   `mov rsp, rbp; pop rbp` immediately before the terminal `ret`.
//! - `no_epilogue_with_no_frame` (defensive) — the annotated function emits
//!   NO epilogue either.
//! - `lambda_first_instr_points_at_prologue` — the ELF-symbol-aligning
//!   `lambda_first_instr` entry names `push rbp`, not the body's first
//!   instruction. This is the invariant that prevents control from
//!   entering after the prologue and blowing the stack at ret.
//!
//! paideia-as#1314: also pins the unsafe-bodied lambda combination that
//! motivated this issue:
//!
//! - `no_prologue_for_unsafe_body_regardless_of_no_frame` — an
//!   unsafe-bodied lambda never gets a frame prologue, whether or not it
//!   carries `@no_frame`. This is the invariant 825+ existing `@no_frame`
//!   sites (and every unannotated unsafe lambda) are written against; it
//!   must not flip silently if `visit_lambda`'s condition is ever
//!   restructured again.
//! - `no_frame_on_unsafe_body_emits_b1707` — `@no_frame` combined with an
//!   unsafe body now produces a B1707 diagnostic (redundant annotation)
//!   instead of being silently accepted and ignored.
//! - `no_frame_absent_on_unsafe_body_emits_no_b1707` — an unannotated
//!   unsafe-bodied lambda emits no B1707 (nothing redundant was written).

use super::super::*;
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::let_meta::LetInfo;

/// Build a minimal `let f = fn(x) -> unsafe {}` arena and drive the walker.
/// Mirrors `build_and_walk_identity` but with an (empty) `Unsafe` body
/// instead of the identity `Var` body, so `visit_lambda`'s
/// `body_is_unsafe` arm fires.
fn build_and_walk_unsafe_body(no_frame: bool) -> (IrArena, EmitWalker, IrNodeId) {
    let mut arena = IrArena::new();
    let unsafe_body = arena.alloc(IrKind::Unsafe, span());
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [unsafe_body]);
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lambda_id]);
    arena.binding_names_mut().insert(let_id, "f".to_string());

    let mut let_info = LetInfo::immutable();
    let_info.no_frame = no_frame;
    arena.let_meta_mut().insert(let_id, let_info);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);
    (arena, walker, lambda_id)
}

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Build a minimal `let f = fn(x) -> x` arena and drive the walker.
/// If `no_frame` is `Some(true)`, the wrapping `Let` carries
/// `@no_frame`; otherwise the default (walkable) emission fires.
fn build_and_walk_identity(no_frame: bool) -> (IrArena, EmitWalker, IrNodeId) {
    let mut arena = IrArena::new();
    let param_var = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(param_var, "x".to_string());
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [param_var]);
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lambda_id]);
    arena.binding_names_mut().insert(let_id, "f".to_string());

    let mut let_info = LetInfo::immutable();
    let_info.no_frame = no_frame;
    arena.let_meta_mut().insert(let_id, let_info);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);
    (arena, walker, lambda_id)
}

/// Collect the emitted instructions for `lambda_id`'s function in
/// `(emission_order, node_id)` order — the same key `.text` layout uses.
fn function_instructions(walker: &EmitWalker, lambda_id: IrNodeId)
    -> Vec<paideia_as_ir::instruction::Instruction>
{
    let target = lambda_id.get();
    let mut entries: Vec<(u32, IrNodeId, paideia_as_ir::instruction::Instruction)> = walker
        .state()
        .instructions()
        .entries()
        .iter()
        .filter_map(|(id, inst)| {
            walker
                .state()
                .instr_to_lambda()
                .get(id)
                .copied()
                .filter(|owner| *owner == target)
                .map(|_| (inst.emission_order, *id, inst.clone()))
        })
        .collect();
    entries.sort_by_key(|(order, id, _)| (*order, id.get()));
    entries.into_iter().map(|(_, _, inst)| inst).collect()
}

/// paideia-as#1276 phase 3: default identity function `let f = fn(x) -> x`
/// (no `@no_frame`) begins with `push rbp; mov rbp, rsp`.
#[test]
fn emit_prologue_by_default() {
    let (_arena, walker, lambda_id) = build_and_walk_identity(false);
    let insts = function_instructions(&walker, lambda_id);
    assert!(
        insts.len() >= 2,
        "expected at least the 2 prologue instructions, got: {:?}",
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );

    assert_eq!(
        insts[0].mnemonic,
        paideia_as_ir::instruction::Mnemonic::Push,
        "first instruction must be `push rbp`, got {:?}",
        insts[0].mnemonic
    );
    assert_eq!(
        insts[0].operands.as_slice(),
        &[paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RBP)],
        "`push` must target rbp"
    );

    assert_eq!(
        insts[1].mnemonic,
        paideia_as_ir::instruction::Mnemonic::Mov,
        "second instruction must be `mov rbp, rsp`, got {:?}",
        insts[1].mnemonic
    );
    assert_eq!(
        insts[1].operands.as_slice(),
        &[
            paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RBP),
            paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RSP),
        ],
        "second operand list must be (RBP, RSP) — mov rbp, rsp"
    );

    // `was_frame_prologue_emitted` must match — that's the flag `emit_ret`
    // reads to decide whether to emit the matching epilogue.
    assert!(
        walker.state().was_frame_prologue_emitted(lambda_id.get()),
        "state must record the prologue as emitted"
    );
}

/// paideia-as#1276 phase 3: identity function marked `@no_frame` emits NO
/// prologue — the first instruction is the body itself (`mov rax, rdi`).
#[test]
fn no_prologue_with_no_frame() {
    let (_arena, walker, lambda_id) = build_and_walk_identity(true);
    let insts = function_instructions(&walker, lambda_id);

    // Not a single `push rbp` should appear inside this function.
    let has_push_rbp = insts.iter().any(|inst| {
        inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Push
            && inst.operands.as_slice()
                == &[paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RBP)]
    });
    assert!(
        !has_push_rbp,
        "@no_frame function must not emit `push rbp`; got {:?}",
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );

    // First instruction is the identity body: `mov rax, rdi`.
    assert_eq!(
        insts[0].mnemonic,
        paideia_as_ir::instruction::Mnemonic::Mov
    );
    assert_eq!(
        insts[0].operands.as_slice(),
        &[
            paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RAX),
            paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RDI),
        ],
        "@no_frame identity must start with `mov rax, rdi`"
    );

    // And the walker's frame-prologue-emitted flag must be false.
    assert!(
        !walker.state().was_frame_prologue_emitted(lambda_id.get()),
        "@no_frame lambda must NOT be recorded as prologue-emitted"
    );
}

/// paideia-as#1276 phase 3: default identity function ends with
/// `mov rsp, rbp; pop rbp; ret` — the frame-pointer epilogue precedes ret.
#[test]
fn epilogue_leave_ret() {
    let (_arena, walker, lambda_id) = build_and_walk_identity(false);
    let insts = function_instructions(&walker, lambda_id);
    let n = insts.len();
    assert!(
        n >= 3,
        "expected at least 3 instructions for the epilogue+ret tail, got {}",
        n
    );

    // Last three instructions must be exactly the epilogue+ret.
    assert_eq!(
        insts[n - 3].mnemonic,
        paideia_as_ir::instruction::Mnemonic::Mov,
        "third-to-last must be `mov rsp, rbp`, got {:?}",
        insts[n - 3].mnemonic
    );
    assert_eq!(
        insts[n - 3].operands.as_slice(),
        &[
            paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RSP),
            paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RBP),
        ],
        "`mov rsp, rbp` must actually be (RSP, RBP), not the other order"
    );

    assert_eq!(
        insts[n - 2].mnemonic,
        paideia_as_ir::instruction::Mnemonic::Pop,
        "second-to-last must be `pop rbp`"
    );
    assert_eq!(
        insts[n - 2].operands.as_slice(),
        &[paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RBP)],
        "`pop` must target rbp"
    );

    assert_eq!(
        insts[n - 1].mnemonic,
        paideia_as_ir::instruction::Mnemonic::Ret,
        "last instruction must be `ret`"
    );
}

/// paideia-as#1276 phase 3 (defensive): identity function marked `@no_frame`
/// must emit NO frame epilogue — the last three instructions must NOT be
/// `mov rsp, rbp; pop rbp; ret`; the last must be `ret` alone (the body's
/// third instruction is directly `mov rax, rdi` for identity).
#[test]
fn no_epilogue_with_no_frame() {
    let (_arena, walker, lambda_id) = build_and_walk_identity(true);
    let insts = function_instructions(&walker, lambda_id);
    let n = insts.len();
    assert_eq!(
        n, 2,
        "@no_frame identity must emit exactly `mov rax, rdi; ret` (2 insts), got {}: {:?}",
        n,
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );
    assert_eq!(insts[0].mnemonic, paideia_as_ir::instruction::Mnemonic::Mov);
    assert_eq!(insts[1].mnemonic, paideia_as_ir::instruction::Mnemonic::Ret);
}

/// paideia-as#1276 phase 3: the emitted prologue's `push rbp` must claim
/// `lambda_first_instr[L]` so the ELF symbol for this function starts AT
/// the frame prologue, not after it. If we left `lambda_first_instr`
/// pointing at the body's first instruction (which `record_lambda_entry`
/// installs via `.or_insert` before lazy prologue injection), callers would
/// jump INTO the function past `push rbp` — `mov rsp, rbp` on entry would
/// then restore rsp to the CALLER's frame and `pop rbp; ret` would corrupt
/// both rbp and the return address.
#[test]
fn lambda_first_instr_points_at_prologue() {
    let (_arena, walker, lambda_id) = build_and_walk_identity(false);
    let first_instr_id = walker
        .state()
        .lambda_first_instr()
        .get(&lambda_id.get())
        .copied()
        .expect("lambda_first_instr must be recorded for the identity function");

    let first_inst = walker
        .state()
        .instructions()
        .get(first_instr_id)
        .expect("the recorded first-instruction id must resolve to an actual instruction");

    assert_eq!(
        first_inst.mnemonic,
        paideia_as_ir::instruction::Mnemonic::Push,
        "lambda_first_instr must name `push rbp`, got {:?}",
        first_inst.mnemonic
    );
    assert_eq!(
        first_inst.operands.as_slice(),
        &[paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RBP)],
        "the recorded first instruction must be exactly `push rbp`"
    );
}

/// paideia-as#1314 regression: an unsafe-bodied lambda must NEVER get a
/// frame-pointer prologue, whether or not it carries `@no_frame`. This is
/// the invariant `visit_lambda`'s `body_is_unsafe` check exists to
/// preserve; the fix for #1314 only stopped `is_lambda_no_frame` from
/// being dead code for unsafe bodies, it must not change which lambdas
/// get a prologue. If this test ever fails after touching
/// `visit_lambda`'s frame-arming condition, that condition has been
/// folded incorrectly — see the warning comment at that call site.
#[test]
fn no_prologue_for_unsafe_body_regardless_of_no_frame() {
    for no_frame in [false, true] {
        let (_arena, walker, lambda_id) = build_and_walk_unsafe_body(no_frame);
        assert!(
            !walker.state().was_frame_prologue_emitted(lambda_id.get()),
            "unsafe-bodied lambda (no_frame={no_frame}) must never be recorded as \
             prologue-emitted"
        );
        let insts = function_instructions(&walker, lambda_id);
        let has_push_rbp = insts.iter().any(|inst| {
            inst.mnemonic == paideia_as_ir::instruction::Mnemonic::Push
                && inst.operands.as_slice()
                    == &[paideia_as_ir::instruction::Operand::Reg(paideia_as_ir::abi::RBP)]
        });
        assert!(
            !has_push_rbp,
            "unsafe-bodied lambda (no_frame={no_frame}) must not emit `push rbp`; got {:?}",
            insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
        );
    }
}

/// paideia-as#1314: `@no_frame` on an unsafe-bodied lambda is redundant
/// (see `no_prologue_for_unsafe_body_regardless_of_no_frame`) and must now
/// produce a B1707 diagnostic instead of being silently accepted.
#[test]
fn no_frame_on_unsafe_body_emits_b1707() {
    let (_arena, walker, _lambda_id) = build_and_walk_unsafe_body(true);
    let has_b1707 = walker
        .structured_diagnostics
        .iter()
        .any(|d| d.code().number() == 1707);
    assert!(
        has_b1707,
        "expected a B1707 diagnostic for `@no_frame` on an unsafe body, got: {:?}",
        walker
            .structured_diagnostics
            .iter()
            .map(|d| d.code().number())
            .collect::<Vec<_>>()
    );
}

/// paideia-as#1314 (defensive): an unsafe-bodied lambda with NO `@no_frame`
/// annotation has nothing redundant to flag, so it must not emit B1707.
#[test]
fn no_frame_absent_on_unsafe_body_emits_no_b1707() {
    let (_arena, walker, _lambda_id) = build_and_walk_unsafe_body(false);
    let has_b1707 = walker
        .structured_diagnostics
        .iter()
        .any(|d| d.code().number() == 1707);
    assert!(
        !has_b1707,
        "unannotated unsafe-bodied lambda must not emit B1707, got: {:?}",
        walker
            .structured_diagnostics
            .iter()
            .map(|d| d.code().number())
            .collect::<Vec<_>>()
    );
}
