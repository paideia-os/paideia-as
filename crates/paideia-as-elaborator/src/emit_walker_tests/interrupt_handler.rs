//! paideia-as#1278 phase 2: unit tests for the @interrupt / @interrupt_error
//! ISR entry-stub synthesis around `fn` lambdas — the elaborator side of
//! the wire-up that unblocks paideia-os R18 IPI vector handlers migrating
//! off `src/kernel/core/int/isr_trampoline.pdx`.
//!
//! Coverage:
//!
//! - `interrupt_prologue_pushes_13_gprs_and_clds` — a lambda marked as an
//!   ISR entry emits `push rax; push rcx; ...; push r15; cld` as its
//!   leading instructions, regardless of body shape.
//! - `interrupt_epilogue_pops_13_gprs_and_iretq_no_errcode` — the same
//!   lambda ends with `pop r15; ...; pop rax; iretq` when the vector does
//!   NOT carry a CPU-pushed error code (`has_error_code = false`).
//! - `interrupt_error_epilogue_adds_rsp_before_iretq` — a lambda whose
//!   attr carries `has_error_code = true` emits `pop r15; ...; pop rax;
//!   add rsp, 8; iretq` — the 8-byte errcode-skip before iretq is
//!   required (Intel SDM Vol. 3A §6.15) for vectors 8/10/11/12/13/14/17/21/29/30.
//! - `interrupt_replaces_ret_with_iretq_on_non_unsafe_body` — a non-unsafe
//!   ISR body (e.g. identity) emits `iretq` at the tail, never `ret`.
//! - `interrupt_no_b1707_on_implicit_no_frame` — the implicit `@no_frame`
//!   set by phase-1 lower.rs for interrupt lambdas must NOT trip B1707
//!   (the redundant-`@no_frame` warning) — the ISR sugar owns the flag.
//! - `interrupt_epilogue_sorts_after_unsafe_body` — for an unsafe-bodied
//!   ISR, the pop chain + iretq must sort after any body instructions in
//!   the (emission_order, node_id) key that drives .text layout.

use super::super::*;
use paideia_as_diagnostics::{FileId, Span};
use paideia_as_ir::instruction::{Mnemonic, Operand};
use paideia_as_ir::let_meta::{InterruptAttr, LetInfo};

fn span() -> Span {
    Span::new(FileId::new(1).unwrap(), 0, 1)
}

/// Build a `let f = fn(x) -> unsafe {}` arena carrying an `@interrupt(...)` /
/// `@interrupt_error(...)` marker via `LetInfo::interrupt` (and its implicit
/// `no_frame = true` companion, matching phase-1 lower.rs). Drive the walker
/// AND `emit_interrupt_epilogues` so the resulting instruction stream mirrors
/// what cmd_build produces for a compiled ISR.
fn build_and_walk_interrupt_unsafe(has_error_code: bool) -> (IrArena, EmitWalker, IrNodeId) {
    let mut arena = IrArena::new();
    let unsafe_body = arena.alloc(IrKind::Unsafe, span());
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [unsafe_body]);
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lambda_id]);
    arena.binding_names_mut().insert(let_id, "isr".to_string());

    let mut let_info = LetInfo::immutable();
    // ISR sugar implies @no_frame (matches phase-1 lower.rs).
    let_info.no_frame = true;
    let_info.interrupt = Some(InterruptAttr {
        has_error_code,
        vector: if has_error_code { 13 } else { 0x20 },
        name: if has_error_code {
            "general_protection".to_string()
        } else {
            "test_vec".to_string()
        },
    });
    arena.let_meta_mut().insert(let_id, let_info);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);
    // Post-pass — matches cmd_build's wiring after emit_pending_unsafe_bodies.
    walker.emit_interrupt_epilogues(&mut arena);
    (arena, walker, lambda_id)
}

/// Build a `let f = fn(x) -> x @interrupt(...)` arena — the (rare) non-unsafe
/// body case. Confirms that the interrupt sugar composes with an
/// elaborator-lowered body and that emit_ret suppresses the normal `ret` in
/// favour of the post-pass's `iretq`.
fn build_and_walk_interrupt_identity() -> (IrArena, EmitWalker, IrNodeId) {
    let mut arena = IrArena::new();
    let param_var = arena.alloc(IrKind::Var, span());
    arena.binding_names_mut().insert(param_var, "x".to_string());
    let lambda_id = arena.alloc_with_children(IrKind::Lambda, span(), [param_var]);
    let let_id = arena.alloc_with_children(IrKind::Let, span(), [lambda_id]);
    arena.binding_names_mut().insert(let_id, "isr_id".to_string());

    let mut let_info = LetInfo::immutable();
    let_info.no_frame = true;
    let_info.interrupt = Some(InterruptAttr {
        has_error_code: false,
        vector: 0x21,
        name: "test_id".to_string(),
    });
    arena.let_meta_mut().insert(let_id, let_info);

    let mut walker = EmitWalker::new();
    walker.walk(&mut arena);
    walker.emit_interrupt_epilogues(&mut arena);
    (arena, walker, lambda_id)
}

/// Collect the emitted instructions for `lambda_id`'s function in
/// `(emission_order, node_id)` order — the same key `.text` layout uses.
fn function_instructions(
    walker: &EmitWalker,
    lambda_id: IrNodeId,
) -> Vec<paideia_as_ir::instruction::Instruction> {
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

/// The 13-register spill list, in the exact push order the ISR prologue
/// must emit. Mirrored in `emit_interrupt_prologue`; if that list changes,
/// this constant is what pins the ABI contract for the test.
const EXPECTED_SPILL_ORDER: [paideia_as_ir::instruction::RegId; 13] = [
    paideia_as_ir::abi::RAX,
    paideia_as_ir::abi::RCX,
    paideia_as_ir::abi::RDX,
    paideia_as_ir::abi::RSI,
    paideia_as_ir::abi::RDI,
    paideia_as_ir::abi::R8,
    paideia_as_ir::abi::R9,
    paideia_as_ir::abi::R10,
    paideia_as_ir::abi::R11,
    paideia_as_ir::abi::R12,
    paideia_as_ir::abi::R13,
    paideia_as_ir::abi::R14,
    paideia_as_ir::abi::R15,
];

#[test]
fn interrupt_prologue_pushes_13_gprs_and_clds() {
    let (_arena, walker, lambda_id) = build_and_walk_interrupt_unsafe(false);
    let insts = function_instructions(&walker, lambda_id);

    assert!(
        insts.len() >= 14,
        "expected 13-push spill + cld at prologue; got {} insts: {:?}",
        insts.len(),
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );

    for (i, expected_reg) in EXPECTED_SPILL_ORDER.iter().enumerate() {
        assert_eq!(
            insts[i].mnemonic,
            Mnemonic::Push,
            "prologue inst[{}] must be Push, got {:?}",
            i,
            insts[i].mnemonic
        );
        assert_eq!(
            insts[i].operands.as_slice(),
            &[Operand::Reg(*expected_reg)],
            "prologue inst[{}] must push reg {} (position {} in spill list)",
            i,
            expected_reg.0,
            i
        );
    }

    // Immediately after the 13 pushes: `cld` (zero operands).
    assert_eq!(
        insts[13].mnemonic,
        Mnemonic::Cld,
        "prologue inst[13] must be Cld, got {:?}",
        insts[13].mnemonic
    );
    assert!(
        insts[13].operands.is_empty(),
        "Cld takes zero operands"
    );
}

#[test]
fn interrupt_epilogue_pops_13_gprs_and_iretq_no_errcode() {
    let (_arena, walker, lambda_id) = build_and_walk_interrupt_unsafe(false);
    let insts = function_instructions(&walker, lambda_id);
    let n = insts.len();

    // Tail contract: 13 pops (reverse of push order) then iretq.
    // has_error_code = false, so NO `add rsp, 8` between the last pop and iretq.
    assert!(
        n >= 14,
        "expected 13-pop restore + iretq at tail; got {} insts",
        n
    );

    // iretq is the last instruction.
    assert_eq!(
        insts[n - 1].mnemonic,
        Mnemonic::Iretq,
        "tail inst[-1] must be Iretq, got {:?}",
        insts[n - 1].mnemonic
    );
    assert!(insts[n - 1].operands.is_empty(), "Iretq takes zero operands");

    // 13 pops preceding iretq, in reverse of the push order.
    for (i, expected_reg) in EXPECTED_SPILL_ORDER.iter().rev().enumerate() {
        let pos = n - 14 + i;
        assert_eq!(
            insts[pos].mnemonic,
            Mnemonic::Pop,
            "tail inst[{}] must be Pop, got {:?}",
            pos,
            insts[pos].mnemonic
        );
        assert_eq!(
            insts[pos].operands.as_slice(),
            &[Operand::Reg(*expected_reg)],
            "tail inst[{}] must pop reg {} (position {} in restore list)",
            pos,
            expected_reg.0,
            i
        );
    }

    // No `ret` anywhere — ISR handlers exit via iretq only.
    let has_ret = insts.iter().any(|i| i.mnemonic == Mnemonic::Ret);
    assert!(
        !has_ret,
        "ISR handler must NOT emit `ret`; iretq is the only exit"
    );
}

#[test]
fn interrupt_error_epilogue_adds_rsp_before_iretq() {
    let (_arena, walker, lambda_id) = build_and_walk_interrupt_unsafe(true);
    let insts = function_instructions(&walker, lambda_id);
    let n = insts.len();

    // Tail contract: 13 pops, then `add rsp, 8`, then iretq.
    assert!(
        n >= 15,
        "expected 13-pop + add rsp,8 + iretq at tail; got {} insts",
        n
    );

    assert_eq!(
        insts[n - 1].mnemonic,
        Mnemonic::Iretq,
        "tail inst[-1] must be Iretq"
    );

    assert_eq!(
        insts[n - 2].mnemonic,
        Mnemonic::Add,
        "tail inst[-2] must be Add (errcode skip), got {:?}",
        insts[n - 2].mnemonic
    );
    assert_eq!(
        insts[n - 2].operands.as_slice(),
        &[
            Operand::Reg(paideia_as_ir::abi::RSP),
            Operand::Imm64(8),
        ],
        "the errcode skip must be `add rsp, 8`"
    );

    // The pop immediately before `add rsp, 8` must restore rax — the last
    // register in the reverse-restore order.
    assert_eq!(
        insts[n - 3].mnemonic,
        Mnemonic::Pop,
        "tail inst[-3] must be Pop (rax)"
    );
    assert_eq!(
        insts[n - 3].operands.as_slice(),
        &[Operand::Reg(paideia_as_ir::abi::RAX)],
        "tail inst[-3] must pop rax"
    );
}

#[test]
fn interrupt_replaces_ret_with_iretq_on_non_unsafe_body() {
    let (_arena, walker, lambda_id) = build_and_walk_interrupt_identity();
    let insts = function_instructions(&walker, lambda_id);

    // Prologue (14) + identity body (mov rax, rdi) + epilogue (13 pops + iretq) = 29 insts.
    // No ret; iretq only.
    let has_ret = insts.iter().any(|i| i.mnemonic == Mnemonic::Ret);
    let has_iretq = insts.iter().any(|i| i.mnemonic == Mnemonic::Iretq);
    assert!(
        !has_ret,
        "non-unsafe ISR body must NOT emit `ret`; got: {:?}",
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );
    assert!(
        has_iretq,
        "non-unsafe ISR body must emit `iretq`; got: {:?}",
        insts.iter().map(|i| i.mnemonic).collect::<Vec<_>>()
    );

    // Tail is iretq.
    let n = insts.len();
    assert_eq!(insts[n - 1].mnemonic, Mnemonic::Iretq);
}

#[test]
fn interrupt_no_b1707_on_implicit_no_frame() {
    // The interrupt sugar implicitly sets @no_frame (phase-1 lower.rs). The
    // B1707 diagnostic exists to flag a redundant user-written @no_frame on
    // an unsafe-bodied lambda — but for interrupt handlers the flag is the
    // sugar's own doing, not the user's, and the ISR spill fills the frame
    // niche. Must not fire here.
    let (_arena, walker, _lambda_id) = build_and_walk_interrupt_unsafe(false);
    let has_b1707 = walker
        .structured_diagnostics
        .iter()
        .any(|d| d.code().number() == 1707);
    assert!(
        !has_b1707,
        "interrupt-marked lambda must NOT emit B1707 for the implicit @no_frame, got: {:?}",
        walker
            .structured_diagnostics
            .iter()
            .map(|d| d.code().number())
            .collect::<Vec<_>>()
    );
}

#[test]
fn interrupt_epilogue_sorts_after_body_for_unsafe() {
    // Verify the (emission_order, node_id) sort places the epilogue strictly
    // after the prologue — the invariant that lets the text emitter place
    // pop+iretq at the true tail even when the unsafe body has already
    // consumed intermediate emission_order values.
    let (_arena, walker, lambda_id) = build_and_walk_interrupt_unsafe(false);
    let insts = function_instructions(&walker, lambda_id);
    assert!(insts.len() >= 27, "prologue (14) + epilogue (14) minimum");

    // First push is at position 0, iretq is at position n-1: prologue
    // strictly precedes epilogue by construction.
    assert_eq!(insts[0].mnemonic, Mnemonic::Push);
    assert_eq!(insts[insts.len() - 1].mnemonic, Mnemonic::Iretq);
}
