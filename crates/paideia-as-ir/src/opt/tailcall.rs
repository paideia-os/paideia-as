//! Tail-call elimination.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::instruction::{Instruction, Mnemonic, Operand, RegId};
use crate::node::{IrKind, IrNodeId};

/// The tail-call elimination optimization pass.
pub struct TailCallPass;

/// Conditions that suppress TCO.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TcoBlocker {
    /// Calling across a capability frontier.
    CapabilityBoundary,
    /// Call installs a handler the caller would lose track of.
    EffectHandlerInstalling,
    /// ABI mismatch (e.g., SysV → MS-x64).
    DifferentCallConvention,
    /// Caller still needs to run epilogue (saved regs).
    FrameRequiresEpilogue,
}

/// Phase-3-m2-004: tail-call eligibility checker using InstructionSideTable.
///
/// PAS-DEBT-B3-001 landed the self-recursion gate. PAS-DEBT-B3-002 (#1515)
/// keeps this signature as a thin structural adapter for callers that only
/// hold an `InstructionSideTable`; the enriched arena+call+ret variant lives
/// in [`tco_arena_blocker`] and is what the pass actually calls.
pub fn tco_blocker(
    _side_table: &crate::instruction::InstructionSideTable,
    _call_id: crate::node::IrNodeId,
) -> Option<TcoBlocker> {
    None
}

/// PAS-DEBT-B3-002 (#1515): arena-aware tail-call blocker.
///
/// Returns `Some(reason)` iff the tail-call at `call_id` (returning at
/// `ret_id`, owned by `owner_name`) must NOT be rewritten. Checks:
///
/// 1. **Handler-install boundary** — any `IrKind::Handle` or
///    `IrKind::HandlerValue` node whose IrNodeId lies strictly between
///    `call_id` and `ret_id`. Also any populated `HandlerSideTable` entry
///    whose Handle id or op-body id falls in that range. Elision would
///    drop the handler frame's setup/teardown.
/// 2. **Capability boundary** — the enclosing function's declared cap set
///    (from `fn_declared_caps`) differs from the callee's required cap set
///    at this call site (from `call_site_required_caps`). Both tables
///    unpopulated → silent (no evidence, no block).
///
/// ABI mismatch and callee-saves are left to future waves; the
/// `TcoBlocker` variants exist for them but are not yet surfaced here.
#[must_use]
pub fn tco_arena_blocker(
    arena: &IrArena,
    call_id: IrNodeId,
    ret_id: IrNodeId,
    owner_name: &str,
) -> Option<TcoBlocker> {
    let lo = call_id.get().saturating_add(1);
    let hi = ret_id.get();

    // (1a) Structural handler node in (call_id, ret_id).
    for i in lo..hi {
        if let Some(id) = IrNodeId::new(i) {
            if let Some(data) = arena.get(id) {
                if matches!(data.kind, IrKind::Handle | IrKind::HandlerValue) {
                    return Some(TcoBlocker::EffectHandlerInstalling);
                }
            }
        }
    }

    // (1b) HandlerSideTable entry whose Handle id or op body id lands in range.
    for (handle_id, info) in arena.handler_side_table().iter() {
        let h = handle_id.get();
        if h >= lo && h < hi {
            return Some(TcoBlocker::EffectHandlerInstalling);
        }
        for (_, op_body_id) in &info.ops {
            let b = op_body_id.get();
            if b >= lo && b < hi {
                return Some(TcoBlocker::EffectHandlerInstalling);
            }
        }
    }

    // (2) Cap set disagreement between enclosing fn and this call site.
    if let (Some(declared), Some(required)) = (
        arena.fn_declared_caps().get(owner_name),
        arena.call_site_required_caps().get(call_id),
    ) {
        if declared != required {
            return Some(TcoBlocker::CapabilityBoundary);
        }
    }

    None
}

/// PAS-DEBT-B3-002 extension for paideia-as#1555: widened arena-aware
/// tail-call blocker used by Shape E' (3-inst `[Call sym; Pop r; Ret]`
/// window whose matching Push lives upstream in the function prologue).
///
/// Runs [`tco_arena_blocker`] on the trailing `(call_id, ret_id)` window
/// unchanged. When `earlier_span_start` is `Some(push_id)`, the function
/// additionally scans `[push_id, call_id)` for:
///
/// 1. **Handler-install evidence** — an `IrKind::Handle` /
///    `IrKind::HandlerValue` node, or a populated `HandlerSideTable`
///    entry, whose id lands in that upstream range. A handler frame
///    installed between the upstream `push r` and the `call` would
///    mean the tail-branch's leading `pop r` restores a stale value
///    (the push happened *before* the handler-frame teardown).
///
/// Capability-boundary checks stay attached to the call site itself
/// (already handled by [`tco_arena_blocker`]) — the earlier span is
/// straight-line code in the same enclosing function, so its declared
/// caps match by construction.
///
/// When `earlier_span_start` is `None`, this behaves exactly like
/// [`tco_arena_blocker`] — backward compatibility for Shapes A/D/E/B/C.
#[must_use]
pub fn tco_arena_blocker_with_earlier(
    arena: &IrArena,
    earlier_span_start: Option<IrNodeId>,
    call_id: IrNodeId,
    ret_id: IrNodeId,
    owner_name: &str,
) -> Option<TcoBlocker> {
    // Trailing window check (call_id, ret_id) — unchanged behaviour.
    if let Some(reason) = tco_arena_blocker(arena, call_id, ret_id, owner_name) {
        return Some(reason);
    }

    // Widened upstream check for Shape E' only.
    let start = match earlier_span_start {
        Some(s) => s.get(),
        None => return None,
    };
    let call_lo = call_id.get();
    if start >= call_lo {
        return None; // Empty / inverted span — nothing to scan.
    }

    // (1a) Structural handler node in [earlier_span_start, call_id).
    for i in start..call_lo {
        if let Some(id) = IrNodeId::new(i) {
            if let Some(data) = arena.get(id) {
                if matches!(data.kind, IrKind::Handle | IrKind::HandlerValue) {
                    return Some(TcoBlocker::EffectHandlerInstalling);
                }
            }
        }
    }

    // (1b) HandlerSideTable entry whose Handle id or op-body id lands in range.
    for (handle_id, info) in arena.handler_side_table().iter() {
        let h = handle_id.get();
        if h >= start && h < call_lo {
            return Some(TcoBlocker::EffectHandlerInstalling);
        }
        for (_, op_body_id) in &info.ops {
            let b = op_body_id.get();
            if b >= start && b < call_lo {
                return Some(TcoBlocker::EffectHandlerInstalling);
            }
        }
    }

    None
}

/// Human-readable reason string for O1516 diagnostics.
fn blocker_reason(b: TcoBlocker) -> &'static str {
    match b {
        TcoBlocker::CapabilityBoundary => "capability-declaration mismatch",
        TcoBlocker::EffectHandlerInstalling => "handler-install boundary",
        TcoBlocker::DifferentCallConvention => "ABI mismatch",
        TcoBlocker::FrameRequiresEpilogue => "frame requires epilogue",
    }
}

/// Internal implementation: TCO eligibility check on explicit boolean flags.
///
/// Takes 4 boolean conditions; returns the blocker (Some) or None (eligible).
/// Helper logic preserved from phase-2-m9-008.
#[doc(hidden)]
pub fn tco_blocker_impl(
    crosses_cap_boundary: bool,
    installs_handler: bool,
    abi_mismatch: bool,
    frame_has_callee_saves: bool,
) -> Option<TcoBlocker> {
    if crosses_cap_boundary {
        return Some(TcoBlocker::CapabilityBoundary);
    }
    if installs_handler {
        return Some(TcoBlocker::EffectHandlerInstalling);
    }
    if abi_mismatch {
        return Some(TcoBlocker::DifferentCallConvention);
    }
    if frame_has_callee_saves {
        return Some(TcoBlocker::FrameRequiresEpilogue);
    }
    None
}

/// Extract the target symbol name from a `Call` instruction, if the operand
/// is a direct `SymbolRef`. Register / memory targets are indirect and never
/// match self-recursion by name.
fn call_target_symbol(inst: &Instruction) -> Option<&str> {
    match inst.operands.first()? {
        Operand::SymbolRef { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

/// Extract the target register from a `Call` instruction, if the operand is
/// a single `Reg` (indirect call through a register). Used by PAS-DEBT-B3-007
/// (#1547) — the rewrite emits `jmp reg64` via the encoder support that
/// landed in Wave 15 (paideia-as#1546).
fn call_target_reg(inst: &Instruction) -> Option<RegId> {
    if inst.operands.len() != 1 {
        return None;
    }
    match inst.operands.first()? {
        Operand::Reg(r) => Some(*r),
        _ => None,
    }
}

/// Return the register a `Pop` restores, if the instruction is `pop r64`
/// exactly (single Reg operand). Used by the pop-restore-indirect shape
/// (PAS-DEBT-B3-007, #1547) to prove the two pop windows are equivalent.
fn pop_reg_operand(inst: &Instruction) -> Option<RegId> {
    if inst.mnemonic != Mnemonic::Pop || inst.operands.len() != 1 {
        return None;
    }
    match inst.operands.first()? {
        Operand::Reg(r) => Some(*r),
        _ => None,
    }
}

/// Return the register a `Push` saves, if the instruction is `push r64`
/// exactly (single Reg operand). Used by the push/pop-bracketed direct
/// shape (paideia-as#1551) to prove the pre- and post-call brackets name
/// the same register.
fn push_reg_operand(inst: &Instruction) -> Option<RegId> {
    if inst.mnemonic != Mnemonic::Push || inst.operands.len() != 1 {
        return None;
    }
    match inst.operands.first()? {
        Operand::Reg(r) => Some(*r),
        _ => None,
    }
}

/// If `inst` is `sub rsp, imm64`, return the immediate (as `i64`); else
/// `None`. The SysV alignment-pad idiom (paideia-as#1551, Shape 4) uses
/// `sub rsp, 8` immediately before an indirect call and `add rsp, 8`
/// immediately after; this extractor drives the pre-call side.
fn sub_rsp_imm_operand(inst: &Instruction) -> Option<i64> {
    if inst.mnemonic != Mnemonic::Sub || inst.operands.len() != 2 {
        return None;
    }
    let dst_is_rsp = matches!(inst.operands.first()?, Operand::Reg(r) if *r == crate::abi::RSP);
    if !dst_is_rsp {
        return None;
    }
    match inst.operands.get(1)? {
        Operand::Imm64(i) => Some(*i),
        _ => None,
    }
}

/// If `inst` is `add rsp, imm64`, return the immediate (as `i64`); else
/// `None`. Symmetric partner of [`sub_rsp_imm_operand`]; the alignment-pad
/// shape only fires when the two immediates match exactly.
fn add_rsp_imm_operand(inst: &Instruction) -> Option<i64> {
    if inst.mnemonic != Mnemonic::Add || inst.operands.len() != 2 {
        return None;
    }
    let dst_is_rsp = matches!(inst.operands.first()?, Operand::Reg(r) if *r == crate::abi::RSP);
    if !dst_is_rsp {
        return None;
    }
    match inst.operands.get(1)? {
        Operand::Imm64(i) => Some(*i),
        _ => None,
    }
}

impl OptPass for TailCallPass {
    fn name(&self) -> &'static str {
        "tailcall"
    }

    /// Rewrite six tail-call shapes; every other pattern is left alone.
    ///
    /// **Pattern C — direct self-recursion (PAS-DEBT-B3-001):**
    /// `Call SymbolRef(f); Ret` where owner(call) == f → `Jmp SymbolRef(f)`.
    /// Mutual recursion, non-tail self-calls, and calls whose owner cannot
    /// be proved are skipped.
    ///
    /// **Pattern B — simple indirect (PAS-DEBT-B3-007, #1547):**
    /// `Call Reg(r); Ret` → `Jmp Reg(r)`. No self-recursion gate applies
    /// (there is no name to compare); the rewrite is a local
    /// semantics-preserving transformation. Depends on the `jmp reg64`
    /// encoder support from paideia-as#1546 (Wave 15).
    ///
    /// **Pattern A — pop-restore indirect (PAS-DEBT-B3-007, #1547):**
    /// `Pop Reg(r_saved); Call Reg(r_target); Pop Reg(r_saved); Ret`
    /// → `Pop Reg(r_saved); Jmp Reg(r_target)`. Fires only when the
    /// pre-call and post-call pops name the *same* register — the
    /// reversal-invariance test that proves the two epilogue windows
    /// leave the machine in equivalent states. Anything asymmetric
    /// (different reg, extra restores) is left alone.
    ///
    /// **Pattern D — SysV alignment-pad indirect (paideia-as#1551):**
    /// `Sub Rsp, imm; Call Reg(r); Add Rsp, imm; Ret` (imm identical on
    /// both sides) → `Add Rsp, imm; Jmp Reg(r)`. The `sub rsp,imm` at the
    /// call site is dropped because the tail-jump reuses the caller's
    /// return context, which already carries the SysV `rsp%16==8`
    /// invariant; only the trailing `add rsp,imm` remains to undo the
    /// caller-visible half of the pad before the branch. Fires only when
    /// both operands are `rsp` (RegId(4)) and both immediates are equal.
    /// Motivating sites: `vops_read/write/…` dispatchers in
    /// `paideia-os/src/kernel/core/fs/vops.pdx` (RETIRE-5 / paideia-os#2512).
    ///
    /// **Pattern E — push/pop-bracketed direct (paideia-as#1551):**
    /// `Push Reg(r); Call SymbolRef(s); Pop Reg(r); Ret` (same reg on
    /// both brackets) → `Pop Reg(r); Jmp SymbolRef(s)`. The rewrite pulls
    /// the callee-save teardown ahead of the tail-branch so the target
    /// sees the caller-visible callee-save state restored. Fires only
    /// when the push and pop name the *same* register (symmetry mirror
    /// of Pattern A). See implementation note below on the physical
    /// layout of the motivating sites.
    ///
    /// **Pattern E' — trailing-pop-bracketed direct (paideia-as#1555):**
    /// `Call SymbolRef(s); Pop Reg(r); Ret` (3-inst window) with the
    /// matching `Push Reg(r)` living upstream in the function prologue
    /// (10+ instructions before the call — Shape E's 4-inst adjacent
    /// window would not fire). The pass walks backward from the Call
    /// within the enclosing function (owner boundary from
    /// `instr_owner`) looking for the matching Push; the search
    /// refuses on any intervening branch (Jmp/Jcc/Call/FarJmp),
    /// preserving the leaf-ish assumption. Rewrite in-place:
    /// `ids[i] Call → Pop r`, `ids[i+1] Pop → Jmp sym`, drop Ret.
    /// The upstream Push is preserved untouched — other prologue
    /// code and callee-save discipline through the function body
    /// still rely on it. Motivating sites:
    /// `nvme_log_smart_fetch` / `nvme_log_error_info_fetch` in
    /// `paideia-os/src/kernel/core/cap/nvme_admin_events.pdx`.
    ///
    /// All six shapes are gated by [`tco_arena_blocker`]:
    /// capability-boundary and effect-handler-install checks apply
    /// uniformly to direct and indirect tail-calls. Shape E' widens
    /// the handler-install scan via
    /// [`tco_arena_blocker_with_earlier`] to also cover the span
    /// between the upstream Push and the Call.
    fn apply(&self, arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        let mut ids: Vec<IrNodeId> = arena.instructions().entries().keys().copied().collect();
        ids.sort_by_key(|id| id.get());

        if ids.len() < 2 {
            return false;
        }

        let mut changed = false;
        let mut i = 0;

        while i < ids.len() {
            // ── Pattern D: Sub Rsp, imm; Call Reg; Add Rsp, imm; Ret ──
            //
            // SysV alignment-pad indirect tail-call. The Sub/Add pair is
            // an rsp pad the compiler emits around the indirect call so
            // the callee lands with `rsp%16==8` (SysV entry invariant);
            // when the caller tail-jumps, only the *trailing* Add is
            // needed to undo the caller-visible half of the pad — the
            // Sub is dropped because the jump reuses the caller's
            // already-aligned return context. Fires only when both
            // operands are `rsp` and both immediates match exactly.
            // Motivating sites: 14 vops.pdx dispatchers (RETIRE-5,
            // paideia-os#2512).
            if i + 3 < ids.len() {
                let (sub_imm, call_reg, add_imm, is_ret_after) = {
                    let table = arena.instructions();
                    let si = table.get(ids[i]).and_then(sub_rsp_imm_operand);
                    let cr = table
                        .get(ids[i + 1])
                        .filter(|c| c.mnemonic == Mnemonic::Call)
                        .and_then(call_target_reg);
                    let ai = table.get(ids[i + 2]).and_then(add_rsp_imm_operand);
                    let rt = table
                        .get(ids[i + 3])
                        .map(|n| n.mnemonic == Mnemonic::Ret)
                        .unwrap_or(false);
                    (si, cr, ai, rt)
                };
                if let (Some(imm_sub), Some(target_reg), Some(imm_add), true) =
                    (sub_imm, call_reg, add_imm, is_ret_after)
                {
                    if imm_sub == imm_add {
                        let sub_id = ids[i];
                        let call_id = ids[i + 1];
                        let ret_id = ids[i + 3];
                        let owner = arena
                            .instr_owner()
                            .get(call_id)
                            .map(std::string::ToString::to_string)
                            .unwrap_or_default();

                        if let Some(reason) =
                            tco_arena_blocker(arena, call_id, ret_id, &owner)
                        {
                            sink.emit(
                                "tailcall",
                                format!(
                                    "O1516: TCO refused for i{} (indirect align-pad r{} imm={}) — {} (owner={})",
                                    call_id.get(),
                                    target_reg.0,
                                    imm_sub,
                                    blocker_reason(reason),
                                    if owner.is_empty() { "?" } else { owner.as_str() },
                                ),
                            );
                            i += 1;
                            continue;
                        }

                        // Rewrite: drop the leading Sub Rsp,imm; leave the
                        // trailing Add Rsp,imm in place; Call→Jmp; drop
                        // Ret. Result window: [Add Rsp,imm; Jmp Reg].
                        arena.instructions_mut().remove(sub_id);
                        if let Some(inst) = arena.instructions_mut().get_mut(call_id) {
                            inst.mnemonic = Mnemonic::Jmp;
                        }
                        arena.instructions_mut().remove(ret_id);

                        sink.emit(
                            "tailcall",
                            format!(
                                "O1520: TCO indirect align-pad Call→Jmp i{} (reg=r{}) + drop Sub i{} + drop Ret i{} (imm={})",
                                call_id.get(),
                                target_reg.0,
                                sub_id.get(),
                                ret_id.get(),
                                imm_sub,
                            ),
                        );

                        changed = true;
                        i += 4;
                        continue;
                    }
                }
            }

            // ── Pattern E: Push reg; Call SymbolRef; Pop reg; Ret ────
            //
            // Callee-save-bracketed direct tail-call. The Push+Pop pair
            // saves and restores a callee-save register across the call;
            // the tail-call rewrite pulls the restore (Pop) ahead of the
            // branch so the target sees the caller-visible callee-save
            // state fully restored. Fires only when both brackets name
            // the *same* register — the symmetry mirror of Pattern A.
            //
            // Physical-layout note (paideia-as#1551): the motivating
            // nvme_admin_events.pdx fetchers place their `push rbx` at
            // function entry (10+ instructions before `call sym`), not
            // adjacent to the call. That physical shape is
            // `[Call sym, Pop rbx, Ret]` (3-inst window), which this
            // 4-inst matcher does NOT recognise. The 4-inst window
            // fires only when Push and Call are adjacent — the strict
            // reading of the shape spec. A follow-up may add the
            // 3-inst variant once the safety of "trust an earlier
            // Push" has been justified.
            if i + 3 < ids.len() {
                let (push_a, call_sym, pop_b, is_ret_after) = {
                    let table = arena.instructions();
                    let pa = table.get(ids[i]).and_then(push_reg_operand);
                    let call_opt = table
                        .get(ids[i + 1])
                        .filter(|c| c.mnemonic == Mnemonic::Call);
                    let cs = call_opt.and_then(call_target_symbol).map(str::to_string);
                    let pb = table.get(ids[i + 2]).and_then(pop_reg_operand);
                    let rt = table
                        .get(ids[i + 3])
                        .map(|n| n.mnemonic == Mnemonic::Ret)
                        .unwrap_or(false);
                    (pa, cs, pb, rt)
                };
                if let (Some(pushed), Some(target_sym), Some(popped), true) =
                    (push_a, call_sym, pop_b, is_ret_after)
                {
                    if pushed == popped {
                        let push_id = ids[i];
                        let call_id = ids[i + 1];
                        let pop_id = ids[i + 2];
                        let ret_id = ids[i + 3];
                        let owner = arena
                            .instr_owner()
                            .get(call_id)
                            .map(std::string::ToString::to_string)
                            .unwrap_or_default();

                        if let Some(reason) =
                            tco_arena_blocker(arena, call_id, ret_id, &owner)
                        {
                            sink.emit(
                                "tailcall",
                                format!(
                                    "O1516: TCO refused for i{} (direct push-pop-bracket r{} → {}) — {} (owner={})",
                                    call_id.get(),
                                    pushed.0,
                                    target_sym,
                                    blocker_reason(reason),
                                    if owner.is_empty() { "?" } else { owner.as_str() },
                                ),
                            );
                            i += 1;
                            continue;
                        }

                        // Rewrite: mutate the leading Push→Pop (so the
                        // callee-save value is restored before the tail
                        // branch), mutate Call→Jmp, drop the trailing
                        // Pop and Ret. Result window: [Pop reg; Jmp sym].
                        if let Some(inst) = arena.instructions_mut().get_mut(push_id) {
                            inst.mnemonic = Mnemonic::Pop;
                        }
                        if let Some(inst) = arena.instructions_mut().get_mut(call_id) {
                            inst.mnemonic = Mnemonic::Jmp;
                        }
                        arena.instructions_mut().remove(pop_id);
                        arena.instructions_mut().remove(ret_id);

                        sink.emit(
                            "tailcall",
                            format!(
                                "O1522: TCO direct push-pop-bracket Call→Jmp i{} (target={}) + Push→Pop i{} + drop Pop i{} + drop Ret i{} (saved=r{})",
                                call_id.get(),
                                target_sym,
                                push_id.get(),
                                pop_id.get(),
                                ret_id.get(),
                                pushed.0,
                            ),
                        );

                        changed = true;
                        i += 4;
                        continue;
                    }
                }
            }

            // ── Pattern E': Call SymbolRef; Pop reg; Ret ──────────────
            //
            // Trailing-pop-bracketed direct tail-call (paideia-as#1555).
            // The 3-inst window whose matching `Push reg` lives
            // upstream in the function prologue rather than adjacent
            // to the Call — the physical shape at the motivating
            // nvme_admin_events.pdx fetchers, where `push rbx` sits
            // 10+ instructions before `call nvme_get_log_page`.
            //
            // Discovery walks backward through `ids` from `i-1`,
            // bounded by the enclosing function's `instr_owner`
            // span. The walk refuses on any intervening branch
            // (Jmp / Jcc / Call / FarJmp) — a branch would let
            // execution reach the trailing window without the
            // Push being run, invalidating the callee-save
            // bracket. Handler installs in the upstream span are
            // caught by `tco_arena_blocker_with_earlier` (the
            // B3-002 guard widened for Shape E').
            //
            // Rewrite pulls the Pop ahead of the tail branch so
            // the stack is restored before control transfers:
            //   [..., Push r, ..., Call sym, Pop r, Ret]
            //   → [..., Push r, ..., Pop r, Jmp sym]
            // `ids[i]` (was Call) is mutated to `Pop r`;
            // `ids[i+1]` (was Pop r) is mutated to `Jmp sym`;
            // `ids[i+2]` (Ret) is removed. The upstream Push is
            // deliberately preserved — it belongs to the
            // function's prologue and other code (register
            // spills, later Pop, callee-save discipline through
            // the body) may depend on it.
            if i + 2 < ids.len() {
                let (call_sym_opt, pop_reg_opt, is_ret_after) = {
                    let table = arena.instructions();
                    let call_opt = table
                        .get(ids[i])
                        .filter(|c| c.mnemonic == Mnemonic::Call);
                    let cs = call_opt.and_then(call_target_symbol).map(str::to_string);
                    let pb = table.get(ids[i + 1]).and_then(pop_reg_operand);
                    let rt = table
                        .get(ids[i + 2])
                        .map(|n| n.mnemonic == Mnemonic::Ret)
                        .unwrap_or(false);
                    (cs, pb, rt)
                };
                if let (Some(target_sym), Some(popped), true) =
                    (call_sym_opt, pop_reg_opt, is_ret_after)
                {
                    let call_id = ids[i];
                    let pop_id = ids[i + 1];
                    let ret_id = ids[i + 2];
                    let owner = arena
                        .instr_owner()
                        .get(call_id)
                        .map(std::string::ToString::to_string)
                        .unwrap_or_default();

                    // Without owner evidence we cannot bound the
                    // backward walk to the enclosing function — be
                    // conservative and skip. Matches Shape C's
                    // owner-required discipline.
                    if !owner.is_empty() {
                        // Walk backward through the sorted `ids`
                        // list from position `i-1`, stopping at:
                        //  - owner mismatch (function boundary)
                        //  - matching `Push reg` (success)
                        //  - branch mnemonic (refuse)
                        let mut found_push: Option<IrNodeId> = None;
                        let mut had_branch = false;
                        let mut k = i;
                        while k > 0 {
                            k -= 1;
                            let earlier_id = ids[k];
                            match arena.instr_owner().get(earlier_id) {
                                Some(o) if o == owner.as_str() => {}
                                _ => break,
                            }
                            let earlier_inst =
                                match arena.instructions().get(earlier_id) {
                                    Some(inst) => inst,
                                    None => continue,
                                };
                            // Branch-in-span refusal: any non-fall-
                            // through control flow between the
                            // upstream Push and the Call breaks the
                            // leaf-ish assumption Shape E' relies on.
                            // Non-exhaustive `Mnemonic` — the
                            // wildcard arm keeps future variants
                            // safe by default (fall-through only).
                            let is_branch = match earlier_inst.mnemonic {
                                Mnemonic::Jmp
                                | Mnemonic::Jcc(_)
                                | Mnemonic::Call
                                | Mnemonic::FarJmp => true,
                                _ => false,
                            };
                            if is_branch {
                                had_branch = true;
                                break;
                            }
                            // Stack-shape safety: the FIRST stack
                            // operation encountered walking backward
                            // must be the matching `push r`. A
                            // mismatched Push (different reg) or any
                            // Pop between the Call and the prologue
                            // Push would leave the stack unbalanced
                            // when the tail branch reuses the
                            // caller's return context — refuse.
                            if let Some(pushed) =
                                push_reg_operand(earlier_inst)
                            {
                                if pushed == popped {
                                    found_push = Some(earlier_id);
                                }
                                // Matching or not, the walk stops
                                // here — a deeper Push would be a
                                // nested bracket we cannot prove
                                // balanced across the tail branch.
                                break;
                            }
                            if pop_reg_operand(earlier_inst).is_some() {
                                // A Pop before the matching Push
                                // means the stack level at the Call
                                // does not match `[caller frame,
                                // pushed r]` — refuse silently.
                                break;
                            }
                        }

                        if had_branch {
                            sink.emit(
                                "tailcall",
                                format!(
                                    "O1516: TCO refused for i{} (direct trailing pop-bracket r{} → {}) — branch between prologue push and call (owner={})",
                                    call_id.get(),
                                    popped.0,
                                    target_sym,
                                    owner,
                                ),
                            );
                            i += 1;
                            continue;
                        }

                        if let Some(push_id) = found_push {
                            if let Some(reason) = tco_arena_blocker_with_earlier(
                                arena,
                                Some(push_id),
                                call_id,
                                ret_id,
                                &owner,
                            ) {
                                sink.emit(
                                    "tailcall",
                                    format!(
                                        "O1516: TCO refused for i{} (direct trailing pop-bracket r{} → {}) — {} (owner={})",
                                        call_id.get(),
                                        popped.0,
                                        target_sym,
                                        blocker_reason(reason),
                                        owner,
                                    ),
                                );
                                i += 1;
                                continue;
                            }

                            // Rewrite: mutate ids[i] Call → Pop r
                            // (restore callee-save before the branch);
                            // mutate ids[i+1] Pop → Jmp sym; drop
                            // ids[i+2] Ret. Upstream Push untouched.
                            if let Some(inst) =
                                arena.instructions_mut().get_mut(call_id)
                            {
                                inst.mnemonic = Mnemonic::Pop;
                                inst.operands.clear();
                                inst.operands.push(Operand::Reg(popped));
                            }
                            if let Some(inst) =
                                arena.instructions_mut().get_mut(pop_id)
                            {
                                inst.mnemonic = Mnemonic::Jmp;
                                inst.operands.clear();
                                inst.operands.push(Operand::SymbolRef {
                                    name: target_sym.clone(),
                                    addend: 0,
                                });
                            }
                            arena.instructions_mut().remove(ret_id);

                            sink.emit(
                                "tailcall",
                                format!(
                                    "O1524: TCO direct trailing pop-bracket Call→Pop i{} + Pop→Jmp i{} (target={}) + drop Ret i{} (saved=r{}, prologue push i{})",
                                    call_id.get(),
                                    pop_id.get(),
                                    target_sym,
                                    ret_id.get(),
                                    popped.0,
                                    push_id.get(),
                                ),
                            );

                            changed = true;
                            i += 3;
                            continue;
                        }
                    }
                }
            }

            // ── Pattern A: Pop reg; Call Reg; Pop reg; Ret ────────────
            if i + 3 < ids.len() {
                let (pop_a, call_reg, pop_b, is_ret_after) = {
                    let table = arena.instructions();
                    let pa = table.get(ids[i]).and_then(pop_reg_operand);
                    let cr = table
                        .get(ids[i + 1])
                        .filter(|c| c.mnemonic == Mnemonic::Call)
                        .and_then(call_target_reg);
                    let pb = table.get(ids[i + 2]).and_then(pop_reg_operand);
                    let rt = table
                        .get(ids[i + 3])
                        .map(|n| n.mnemonic == Mnemonic::Ret)
                        .unwrap_or(false);
                    (pa, cr, pb, rt)
                };
                if let (Some(a), Some(target_reg), Some(b), true) =
                    (pop_a, call_reg, pop_b, is_ret_after)
                {
                    if a == b {
                        let call_id = ids[i + 1];
                        let ret_id = ids[i + 3];
                        let owner = arena
                            .instr_owner()
                            .get(call_id)
                            .map(std::string::ToString::to_string)
                            .unwrap_or_default();

                        if let Some(reason) =
                            tco_arena_blocker(arena, call_id, ret_id, &owner)
                        {
                            sink.emit(
                                "tailcall",
                                format!(
                                    "O1516: TCO refused for i{} (indirect pop-restore r{}) — {} (owner={})",
                                    call_id.get(),
                                    target_reg.0,
                                    blocker_reason(reason),
                                    if owner.is_empty() { "?" } else { owner.as_str() },
                                ),
                            );
                            i += 1;
                            continue;
                        }

                        // Rewrite: Call → Jmp; drop second Pop and trailing Ret.
                        if let Some(inst) = arena.instructions_mut().get_mut(call_id) {
                            inst.mnemonic = Mnemonic::Jmp;
                        }
                        let pop_b_id = ids[i + 2];
                        arena.instructions_mut().remove(pop_b_id);
                        arena.instructions_mut().remove(ret_id);

                        sink.emit(
                            "tailcall",
                            format!(
                                "O1518: TCO indirect pop-restore Call→Jmp i{} (reg=r{}) + drop Pop i{} + drop Ret i{} (saved=r{})",
                                call_id.get(),
                                target_reg.0,
                                pop_b_id.get(),
                                ret_id.get(),
                                a.0,
                            ),
                        );

                        changed = true;
                        i += 4;
                        continue;
                    }
                }
            }

            // ── Pattern B/C: Call …; Ret ──────────────────────────────
            if i + 1 < ids.len() {
                let call_id = ids[i];
                let next_id = ids[i + 1];

                let (call_sym, call_reg, is_ret) = {
                    let table = arena.instructions();
                    let call_opt = table.get(call_id).filter(|c| c.mnemonic == Mnemonic::Call);
                    let sym = call_opt.and_then(call_target_symbol).map(str::to_string);
                    let reg = call_opt.and_then(call_target_reg);
                    let ret = table
                        .get(next_id)
                        .map(|n| n.mnemonic == Mnemonic::Ret)
                        .unwrap_or(false);
                    (sym, reg, ret)
                };

                if is_ret {
                    // Pattern C — direct self-recursion (SymbolRef target).
                    if let Some(target_name) = call_sym {
                        let owner_snapshot =
                            arena.instr_owner().get(call_id).map(str::to_string);
                        let is_self_recursion = owner_snapshot
                            .as_deref()
                            .map(|owner| owner == target_name.as_str())
                            .unwrap_or(false);

                        if is_self_recursion {
                            let owner_name = owner_snapshot.unwrap();
                            if let Some(reason) =
                                tco_arena_blocker(arena, call_id, next_id, &owner_name)
                            {
                                sink.emit(
                                    "tailcall",
                                    format!(
                                        "O1516: TCO refused for i{} → {} — {} (owner={})",
                                        call_id.get(),
                                        target_name,
                                        blocker_reason(reason),
                                        owner_name,
                                    ),
                                );
                                i += 1;
                                continue;
                            }
                            if let Some(inst) =
                                arena.instructions_mut().get_mut(call_id)
                            {
                                inst.mnemonic = Mnemonic::Jmp;
                            }
                            arena.instructions_mut().remove(next_id);
                            sink.emit(
                                "tailcall",
                                format!(
                                    "O1514: TCO self-recursion Call→Jmp i{} + remove Ret i{} (owner={}, target={})",
                                    call_id.get(),
                                    next_id.get(),
                                    owner_name,
                                    target_name,
                                ),
                            );
                            changed = true;
                            i += 2;
                            continue;
                        }
                    // Pattern B — simple indirect (Reg target).
                    } else if let Some(target_reg) = call_reg {
                        let owner = arena
                            .instr_owner()
                            .get(call_id)
                            .map(std::string::ToString::to_string)
                            .unwrap_or_default();
                        if let Some(reason) =
                            tco_arena_blocker(arena, call_id, next_id, &owner)
                        {
                            sink.emit(
                                "tailcall",
                                format!(
                                    "O1516: TCO refused for i{} (indirect simple r{}) — {} (owner={})",
                                    call_id.get(),
                                    target_reg.0,
                                    blocker_reason(reason),
                                    if owner.is_empty() { "?" } else { owner.as_str() },
                                ),
                            );
                            i += 1;
                            continue;
                        }
                        if let Some(inst) = arena.instructions_mut().get_mut(call_id) {
                            inst.mnemonic = Mnemonic::Jmp;
                        }
                        arena.instructions_mut().remove(next_id);
                        sink.emit(
                            "tailcall",
                            format!(
                                "O1518: TCO indirect simple Call→Jmp i{} (reg=r{}) + remove Ret i{}",
                                call_id.get(),
                                target_reg.0,
                                next_id.get(),
                            ),
                        );
                        changed = true;
                        i += 2;
                        continue;
                    }
                }
            }

            i += 1;
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler_value::{EffectId, HandlerInfo};
    use crate::instruction::{InstrMode, Instruction, InstructionSideTable, Operand, RegId};
    use paideia_as_diagnostics::{FileId, Span};
    use smallvec::SmallVec;
    use std::collections::BTreeSet;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    // ── Helpers ────────────────────────────────────────────────────

    fn call_to(sym: &str) -> Instruction {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::SymbolRef {
            name: sym.to_string(),
            addend: 0,
        });
        Instruction {
            mnemonic: Mnemonic::Call,
            operands: ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        }
    }

    fn ret_inst() -> Instruction {
        Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        }
    }

    fn mov_inst() -> Instruction {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Reg(RegId(0)));
        ops.push(Operand::Reg(RegId(1)));
        Instruction {
            mnemonic: Mnemonic::Mov,
            operands: ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        }
    }

    /// PAS-DEBT-B3-007 (#1547): indirect `call rN`.
    fn call_reg_inst(reg: u8) -> Instruction {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Reg(RegId(reg)));
        Instruction {
            mnemonic: Mnemonic::Call,
            operands: ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        }
    }

    /// PAS-DEBT-B3-007 (#1547): `pop rN` — one-reg operand exactly.
    fn pop_reg_inst(reg: u8) -> Instruction {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Reg(RegId(reg)));
        Instruction {
            mnemonic: Mnemonic::Pop,
            operands: ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        }
    }

    /// paideia-as#1551: `push rN` — one-reg operand exactly.
    fn push_reg_inst(reg: u8) -> Instruction {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Reg(RegId(reg)));
        Instruction {
            mnemonic: Mnemonic::Push,
            operands: ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        }
    }

    /// paideia-as#1551: `sub rsp, imm64` — the SysV pre-call alignment
    /// half of Shape 4 (align-pad indirect tail-call).
    fn sub_rsp_imm_inst(imm: i64) -> Instruction {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Reg(crate::abi::RSP));
        ops.push(Operand::Imm64(imm));
        Instruction {
            mnemonic: Mnemonic::Sub,
            operands: ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        }
    }

    /// paideia-as#1551: `add rsp, imm64` — the SysV post-call alignment
    /// half of Shape 4 (align-pad indirect tail-call).
    fn add_rsp_imm_inst(imm: i64) -> Instruction {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Reg(crate::abi::RSP));
        ops.push(Operand::Imm64(imm));
        Instruction {
            mnemonic: Mnemonic::Add,
            operands: ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        }
    }

    /// paideia-as#1555: `jmp sym` — an intra-function branch used to
    /// exercise Shape E's branch-in-upstream-span refusal.
    fn jmp_sym_inst(sym: &str) -> Instruction {
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::SymbolRef {
            name: sym.to_string(),
            addend: 0,
        });
        Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        }
    }

    // ── Blocker unit tests ─────────────────────────────────────────

    #[test]
    fn tco_blocker_returns_none_when_eligible() {
        assert_eq!(tco_blocker_impl(false, false, false, false), None);
    }

    #[test]
    fn tco_blocker_returns_capability_boundary() {
        assert_eq!(
            tco_blocker_impl(true, false, false, false),
            Some(TcoBlocker::CapabilityBoundary)
        );
    }

    #[test]
    fn tco_blocker_returns_effect_handler_installing() {
        assert_eq!(
            tco_blocker_impl(false, true, false, false),
            Some(TcoBlocker::EffectHandlerInstalling)
        );
    }

    #[test]
    fn tco_blocker_returns_different_call_convention() {
        assert_eq!(
            tco_blocker_impl(false, false, true, false),
            Some(TcoBlocker::DifferentCallConvention)
        );
    }

    #[test]
    fn tco_blocker_returns_frame_requires_epilogue() {
        assert_eq!(
            tco_blocker_impl(false, false, false, true),
            Some(TcoBlocker::FrameRequiresEpilogue)
        );
    }

    #[test]
    fn tco_blocker_with_instruction_side_table() {
        let mut table = InstructionSideTable::new();
        let call_id = IrNodeId::new(1).unwrap();
        table.insert(call_id, call_to("f"));
        assert_eq!(tco_blocker(&table, call_id), None);
    }

    // ── Positive: self-recursion is rewritten ──────────────────────

    #[test]
    fn tco_rewrites_self_recursion_call_ret() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        arena.instructions_mut().insert(call_id, call_to("factorial"));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena
            .instr_owner_mut()
            .insert(call_id, "factorial".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(3).unwrap(), &mut sink);

        assert!(changed, "self-recursion Call+Ret must rewrite");
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Jmp
        );
        assert!(arena.instructions().get(ret_id).is_none());
        assert_eq!(sink.diagnostics.len(), 1);
        assert!(sink.diagnostics[0].message.contains("O1514"));
        assert!(sink.diagnostics[0].message.contains("factorial"));
    }

    // ── Negative: mutual recursion is preserved ────────────────────

    #[test]
    fn tco_preserves_mutual_recursion() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        // In function `f`, call `g` in tail position — must NOT rewrite.
        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        arena.instructions_mut().insert(call_id, call_to("g"));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena.instr_owner_mut().insert(call_id, "f".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(3).unwrap(), &mut sink);

        assert!(!changed, "mutual recursion must not be rewritten by TCO");
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert!(arena.instructions().get(ret_id).is_some());
        assert!(sink.diagnostics.is_empty());
    }

    // ── Negative: non-tail self-call is preserved ──────────────────

    #[test]
    fn tco_preserves_non_tail_self_call() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        // Call foo; Mov ...; Ret — the Call is not immediately followed by Ret.
        let call_id = IrNodeId::new(1).unwrap();
        let mov_id = IrNodeId::new(2).unwrap();
        let ret_id = IrNodeId::new(3).unwrap();

        arena.instructions_mut().insert(call_id, call_to("foo"));
        arena.instructions_mut().insert(mov_id, mov_inst());
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena.instr_owner_mut().insert(call_id, "foo".to_string());
        arena.instr_owner_mut().insert(mov_id, "foo".to_string());
        arena.instr_owner_mut().insert(ret_id, "foo".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(4).unwrap(), &mut sink);

        assert!(!changed, "non-tail self-call must not be rewritten");
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert!(arena.instructions().get(mov_id).is_some());
        assert!(arena.instructions().get(ret_id).is_some());
        assert!(sink.diagnostics.is_empty());
    }

    // ── Positive: simple indirect tail-call is rewritten ───────────
    //
    // PAS-DEBT-B3-007 (#1547): the pass now widens the self-recursion
    // gate to indirect targets. `Call Reg(r); Ret` is a semantics-
    // preserving rewrite to `Jmp Reg(r)` — no self-recursion evidence
    // needed, because there is no name to compare. Cap/handler guards
    // from B3-002 still fire (see `b3_007_intervening_handle_...`).
    // Depends on the `jmp reg64` encoder support in paideia-as#1546.

    #[test]
    fn tco_rewrites_indirect_call_to_jmp_reg() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        arena.instructions_mut().insert(call_id, call_reg_inst(0));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena.instr_owner_mut().insert(call_id, "foo".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(3).unwrap(), &mut sink);

        assert!(changed, "simple indirect Call+Ret must rewrite to Jmp reg");
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Jmp
        );
        // Reg operand preserved verbatim — encoder consumes it.
        let ops = &arena.instructions().get(call_id).unwrap().operands;
        assert!(matches!(ops.first(), Some(Operand::Reg(RegId(0)))));
        assert!(arena.instructions().get(ret_id).is_none());
        assert_eq!(sink.diagnostics.len(), 1);
        assert!(sink.diagnostics[0].message.contains("O1518"));
        assert!(sink.diagnostics[0].message.contains("indirect simple"));
    }

    // ── Negative: missing owner evidence is conservative ───────────

    #[test]
    fn tco_refuses_without_owner_evidence() {
        // PAS-DEBT-B3-001: without instr_owner data, the pass cannot prove
        // self-recursion and must leave the pattern alone. Prior behaviour
        // (rewrite any Call+Ret) was the bug the debt catalog flagged.
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        arena.instructions_mut().insert(call_id, call_to("factorial"));
        arena.instructions_mut().insert(ret_id, ret_inst());
        // No instr_owner entry.

        let changed = pass.apply(&mut arena, IrNodeId::new(3).unwrap(), &mut sink);

        assert!(!changed, "pass must not fire without ownership evidence");
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Call
        );
    }

    // ── Empty arena ────────────────────────────────────────────────

    #[test]
    fn tco_pass_emits_no_diagnostics_for_empty_arena() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let changed = pass.apply(&mut arena, IrNodeId::new(1).unwrap(), &mut sink);

        assert!(!changed);
        assert!(sink.diagnostics.is_empty());
    }

    // ── Two consecutive self-recursive tail calls (interior order) ─

    #[test]
    fn tco_handles_two_self_recursive_pairs_in_sequence() {
        // Layout: [i1 Call foo, i2 Ret, i3 Call foo, i4 Ret]
        // Both pairs are self-recursive → both rewritten.
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        for (n, name) in [(1u32, "foo"), (3, "foo")] {
            let call_id = IrNodeId::new(n).unwrap();
            let ret_id = IrNodeId::new(n + 1).unwrap();
            arena.instructions_mut().insert(call_id, call_to(name));
            arena.instructions_mut().insert(ret_id, ret_inst());
            arena
                .instr_owner_mut()
                .insert(call_id, "foo".to_string());
        }

        let changed = pass.apply(&mut arena, IrNodeId::new(99).unwrap(), &mut sink);

        assert!(changed);
        assert_eq!(
            arena
                .instructions()
                .get(IrNodeId::new(1).unwrap())
                .unwrap()
                .mnemonic,
            Mnemonic::Jmp
        );
        assert!(arena.instructions().get(IrNodeId::new(2).unwrap()).is_none());
        assert_eq!(
            arena
                .instructions()
                .get(IrNodeId::new(3).unwrap())
                .unwrap()
                .mnemonic,
            Mnemonic::Jmp
        );
        assert!(arena.instructions().get(IrNodeId::new(4).unwrap()).is_none());
        assert_eq!(sink.diagnostics.len(), 2);
    }

    // ── PAS-DEBT-B3-002 (#1515) — capability/handler-install guards ───

    /// Positive regression: self-recursion with matching caps still rewrites.
    /// Confirms the B3-002 guard does not over-fire on well-formed input.
    #[test]
    fn b3_002_self_recursion_with_matching_caps_still_rewrites() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        arena.instructions_mut().insert(call_id, call_to("factorial"));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena
            .instr_owner_mut()
            .insert(call_id, "factorial".to_string());

        let mut caps = BTreeSet::new();
        caps.insert("Fs".to_string());
        arena
            .fn_declared_caps_mut()
            .insert("factorial".to_string(), caps.clone());
        arena
            .call_site_required_caps_mut()
            .insert(call_id, caps);

        let changed = pass.apply(&mut arena, IrNodeId::new(3).unwrap(), &mut sink);

        assert!(changed, "matching caps must not block self-recursion TCO");
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Jmp
        );
        assert!(arena.instructions().get(ret_id).is_none());
        assert_eq!(sink.diagnostics.len(), 1);
        assert!(sink.diagnostics[0].message.contains("O1514"));
    }

    /// Negative: an `IrKind::Handle` node between Call and Ret blocks TCO;
    /// O1516 names the handler-install boundary.
    #[test]
    fn b3_002_intervening_handle_node_blocks_tco() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        // Allocate arena nodes at ids 1, 2, 3 — the middle one is a Handle.
        let call_node = arena.alloc(IrKind::App, span());
        let handle_node = arena.alloc(IrKind::Handle, span());
        let ret_node = arena.alloc(IrKind::Placeholder, span());
        assert_eq!(call_node.get(), 1);
        assert_eq!(handle_node.get(), 2);
        assert_eq!(ret_node.get(), 3);

        arena
            .instructions_mut()
            .insert(call_node, call_to("factorial"));
        arena.instructions_mut().insert(ret_node, ret_inst());
        arena
            .instr_owner_mut()
            .insert(call_node, "factorial".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(4).unwrap(), &mut sink);

        assert!(!changed, "handler-install boundary must block TCO");
        assert_eq!(
            arena.instructions().get(call_node).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert!(arena.instructions().get(ret_node).is_some());
        assert_eq!(sink.diagnostics.len(), 1);
        let msg = &sink.diagnostics[0].message;
        assert!(msg.contains("O1516"), "missing O1516: {}", msg);
        assert!(
            msg.contains("handler-install boundary"),
            "missing reason: {}",
            msg
        );
        assert!(msg.contains("factorial"));
    }

    /// Negative variant: a populated `HandlerSideTable` entry whose Handle id
    /// falls in the Call..Ret range also blocks — even without a raw node.
    #[test]
    fn b3_002_handler_side_table_entry_blocks_tco() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        // Reserve ids 1..=3 in the arena to expose a Handle-id slot at 2.
        let _n1 = arena.alloc(IrKind::App, span());
        let n2 = arena.alloc(IrKind::Placeholder, span());
        let _n3 = arena.alloc(IrKind::Placeholder, span());

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(3).unwrap();

        arena.instructions_mut().insert(call_id, call_to("factorial"));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena
            .instr_owner_mut()
            .insert(call_id, "factorial".to_string());

        // Populate a HandlerInfo whose op body lives at n2 (id 2, in range).
        arena.handler_side_table_mut().insert(
            IrNodeId::new(100).unwrap(),
            HandlerInfo {
                effect: EffectId(1),
                ops: vec![("op".to_string(), n2)],
                ret: None,
                finally: None,
            },
        );

        let changed = pass.apply(&mut arena, IrNodeId::new(4).unwrap(), &mut sink);

        assert!(!changed, "HandlerSideTable op-body in range must block TCO");
        assert_eq!(sink.diagnostics.len(), 1);
        assert!(sink.diagnostics[0].message.contains("O1516"));
        assert!(sink.diagnostics[0]
            .message
            .contains("handler-install boundary"));
    }

    /// Negative: enclosing fn declares caps the callee requirement disagrees
    /// with → TCO refused, O1516 names the capability-declaration mismatch.
    #[test]
    fn b3_002_cap_mismatch_blocks_tco() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        arena.instructions_mut().insert(call_id, call_to("worker"));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena
            .instr_owner_mut()
            .insert(call_id, "worker".to_string());

        // Caller declares {Fs, Net}; call site requires only {Fs} — mismatch.
        let mut declared = BTreeSet::new();
        declared.insert("Fs".to_string());
        declared.insert("Net".to_string());
        arena
            .fn_declared_caps_mut()
            .insert("worker".to_string(), declared);

        let mut required = BTreeSet::new();
        required.insert("Fs".to_string());
        arena
            .call_site_required_caps_mut()
            .insert(call_id, required);

        let changed = pass.apply(&mut arena, IrNodeId::new(3).unwrap(), &mut sink);

        assert!(!changed, "cap mismatch must block TCO");
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert!(arena.instructions().get(ret_id).is_some());
        assert_eq!(sink.diagnostics.len(), 1);
        let msg = &sink.diagnostics[0].message;
        assert!(msg.contains("O1516"), "missing O1516: {}", msg);
        assert!(
            msg.contains("capability-declaration mismatch"),
            "missing reason: {}",
            msg
        );
        assert!(msg.contains("worker"));
    }

    /// One-sided cap evidence (only declared, or only required) is treated as
    /// silent — the pass never blocks on a half-populated table.
    #[test]
    fn b3_002_half_populated_cap_tables_do_not_block() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        arena.instructions_mut().insert(call_id, call_to("worker"));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena
            .instr_owner_mut()
            .insert(call_id, "worker".to_string());

        // Only declared side is populated — call site is silent.
        let mut declared = BTreeSet::new();
        declared.insert("Fs".to_string());
        arena
            .fn_declared_caps_mut()
            .insert("worker".to_string(), declared);

        let changed = pass.apply(&mut arena, IrNodeId::new(3).unwrap(), &mut sink);

        assert!(changed, "half-populated cap tables must not block TCO");
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Jmp
        );
    }

    /// The arena blocker helper is stand-alone testable: an out-of-range
    /// Handle node (before the Call or after the Ret) does not block.
    #[test]
    fn b3_002_out_of_range_handle_node_does_not_block() {
        let mut arena = IrArena::new();

        // Layout: id 1 Handle (before call), 2 Call, 3 Ret, 4 Handle (after ret).
        let handle_before = arena.alloc(IrKind::Handle, span());
        let call_id = arena.alloc(IrKind::App, span());
        let ret_id = arena.alloc(IrKind::Placeholder, span());
        let handle_after = arena.alloc(IrKind::Handle, span());
        assert_eq!(handle_before.get(), 1);
        assert_eq!(call_id.get(), 2);
        assert_eq!(ret_id.get(), 3);
        assert_eq!(handle_after.get(), 4);

        let blocker = tco_arena_blocker(&arena, call_id, ret_id, "factorial");
        assert_eq!(blocker, None);
    }

    // ── PAS-DEBT-B3-007 (#1547) — pop-restore-indirect + guards ────────

    /// Positive: `Pop rbx; Call Reg(rax); Pop rbx; Ret` → `Pop rbx; Jmp Reg(rax)`.
    /// The paideia-os nvme_admin_events pattern: same reg popped before AND
    /// after the indirect call proves the two epilogue windows leave the
    /// machine in the same state, so the second pop and the ret are elided.
    #[test]
    fn b3_007_rewrites_pop_restore_indirect_tail_call() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        // rbx = r3, rax = r0 (arbitrary — only identity across the pair matters).
        let pop1_id = IrNodeId::new(1).unwrap();
        let call_id = IrNodeId::new(2).unwrap();
        let pop2_id = IrNodeId::new(3).unwrap();
        let ret_id = IrNodeId::new(4).unwrap();

        arena.instructions_mut().insert(pop1_id, pop_reg_inst(3));
        arena.instructions_mut().insert(call_id, call_reg_inst(0));
        arena.instructions_mut().insert(pop2_id, pop_reg_inst(3));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena.instr_owner_mut().insert(call_id, "nvme_admin".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(5).unwrap(), &mut sink);

        assert!(changed, "symmetric pop-restore indirect must rewrite");

        // First pop unchanged.
        assert_eq!(
            arena.instructions().get(pop1_id).unwrap().mnemonic,
            Mnemonic::Pop
        );
        // Call → Jmp (still targets the same reg).
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Jmp
        );
        let call_ops = &arena.instructions().get(call_id).unwrap().operands;
        assert!(matches!(call_ops.first(), Some(Operand::Reg(RegId(0)))));
        // Second pop and ret both gone.
        assert!(arena.instructions().get(pop2_id).is_none());
        assert!(arena.instructions().get(ret_id).is_none());

        assert_eq!(sink.diagnostics.len(), 1);
        let msg = &sink.diagnostics[0].message;
        assert!(msg.contains("O1518"), "missing O1518: {}", msg);
        assert!(msg.contains("pop-restore"), "missing shape name: {}", msg);
        assert!(msg.contains("saved=r3"));
    }

    /// Negative: asymmetric pop-restore (different regs before and after)
    /// must NOT rewrite — the two epilogue windows are not equivalent, so
    /// the pass falls back to the simple-indirect shape on `Call Reg; Ret`
    /// alone. The first Pop is preserved and the trailing Pop stays too:
    /// only the inner Call+Ret pair is eligible for the B rewrite.
    #[test]
    fn b3_007_preserves_asymmetric_pop_restore() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        // Pop rbx (r3); Call Reg(rax=r0); Pop rcx (r1); Ret — regs differ.
        let pop1_id = IrNodeId::new(1).unwrap();
        let call_id = IrNodeId::new(2).unwrap();
        let pop2_id = IrNodeId::new(3).unwrap();
        let ret_id = IrNodeId::new(4).unwrap();

        arena.instructions_mut().insert(pop1_id, pop_reg_inst(3));
        arena.instructions_mut().insert(call_id, call_reg_inst(0));
        arena.instructions_mut().insert(pop2_id, pop_reg_inst(1));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena.instr_owner_mut().insert(call_id, "worker".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(5).unwrap(), &mut sink);

        // The pop-restore shape rejects (r3 != r1); no other pattern applies
        // to the Call+Pop+Ret trailer either (Call's successor is Pop, not
        // Ret). Every instruction is preserved and no diagnostics fire.
        assert!(!changed, "asymmetric pop-restore must not rewrite");
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert!(arena.instructions().get(pop1_id).is_some());
        assert!(arena.instructions().get(pop2_id).is_some());
        assert!(arena.instructions().get(ret_id).is_some());
        assert!(sink.diagnostics.is_empty());
    }

    /// Negative: an `IrKind::Handle` node between the indirect Call and Ret
    /// still blocks — the B3-002 guards apply uniformly to direct and
    /// indirect targets. O1516 fires, not O1518.
    #[test]
    fn b3_007_intervening_handle_blocks_indirect_tco() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        // Allocate arena nodes so the middle id is a genuine Handle node.
        let call_node = arena.alloc(IrKind::App, span());
        let handle_node = arena.alloc(IrKind::Handle, span());
        let ret_node = arena.alloc(IrKind::Placeholder, span());
        assert_eq!(call_node.get(), 1);
        assert_eq!(handle_node.get(), 2);
        assert_eq!(ret_node.get(), 3);

        arena.instructions_mut().insert(call_node, call_reg_inst(0));
        arena.instructions_mut().insert(ret_node, ret_inst());
        arena.instr_owner_mut().insert(call_node, "foo".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(4).unwrap(), &mut sink);

        assert!(!changed, "handler-install boundary must block indirect TCO");
        assert_eq!(
            arena.instructions().get(call_node).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert!(arena.instructions().get(ret_node).is_some());
        assert_eq!(sink.diagnostics.len(), 1);
        let msg = &sink.diagnostics[0].message;
        assert!(msg.contains("O1516"), "expected O1516, got: {}", msg);
        assert!(msg.contains("handler-install boundary"));
        assert!(msg.contains("indirect simple"));
    }

    /// Negative: cap-set disagreement blocks the indirect simple shape too.
    /// Owner-name lookup for indirect calls comes from `instr_owner` (the
    /// enclosing fn), not the callee — the callee is a runtime value.
    #[test]
    fn b3_007_cap_mismatch_blocks_indirect_tco() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        arena.instructions_mut().insert(call_id, call_reg_inst(0));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena.instr_owner_mut().insert(call_id, "worker".to_string());

        let mut declared = BTreeSet::new();
        declared.insert("Fs".to_string());
        declared.insert("Net".to_string());
        arena
            .fn_declared_caps_mut()
            .insert("worker".to_string(), declared);
        let mut required = BTreeSet::new();
        required.insert("Fs".to_string());
        arena.call_site_required_caps_mut().insert(call_id, required);

        let changed = pass.apply(&mut arena, IrNodeId::new(3).unwrap(), &mut sink);

        assert!(!changed);
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert!(arena.instructions().get(ret_id).is_some());
        assert_eq!(sink.diagnostics.len(), 1);
        let msg = &sink.diagnostics[0].message;
        assert!(msg.contains("O1516"), "expected O1516, got: {}", msg);
        assert!(msg.contains("capability-declaration mismatch"));
        assert!(msg.contains("worker"));
    }

    // ── paideia-as#1551 — Shape D (SysV align-pad indirect) ─────────

    /// Positive: `Sub Rsp,8; Call Reg(rax); Add Rsp,8; Ret` →
    /// `Add Rsp,8; Jmp Reg(rax)`. The Sub is dropped (its purpose was
    /// to pre-align for the callee's own `call`; a tail-jump doesn't
    /// need it); the Add stays to undo the caller-visible half. Call
    /// becomes Jmp, Ret is elided. Motivating sites: 14 vops.pdx
    /// dispatchers (paideia-os#2512, RETIRE-5).
    #[test]
    fn wave26_rewrites_align_pad_indirect_tail_call() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let sub_id = IrNodeId::new(1).unwrap();
        let call_id = IrNodeId::new(2).unwrap();
        let add_id = IrNodeId::new(3).unwrap();
        let ret_id = IrNodeId::new(4).unwrap();

        arena.instructions_mut().insert(sub_id, sub_rsp_imm_inst(8));
        arena.instructions_mut().insert(call_id, call_reg_inst(0));
        arena.instructions_mut().insert(add_id, add_rsp_imm_inst(8));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena
            .instr_owner_mut()
            .insert(call_id, "vops_read".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(5).unwrap(), &mut sink);

        assert!(changed, "align-pad indirect Sub+Call+Add+Ret must rewrite");

        // Sub dropped; Call → Jmp; Add preserved; Ret dropped.
        assert!(arena.instructions().get(sub_id).is_none());
        let call_now = arena.instructions().get(call_id).unwrap();
        assert_eq!(call_now.mnemonic, Mnemonic::Jmp);
        assert!(matches!(
            call_now.operands.first(),
            Some(Operand::Reg(RegId(0)))
        ));
        let add_now = arena.instructions().get(add_id).unwrap();
        assert_eq!(add_now.mnemonic, Mnemonic::Add);
        assert!(matches!(
            add_now.operands.first(),
            Some(Operand::Reg(r)) if *r == crate::abi::RSP
        ));
        assert!(matches!(add_now.operands.get(1), Some(Operand::Imm64(8))));
        assert!(arena.instructions().get(ret_id).is_none());

        assert_eq!(sink.diagnostics.len(), 1);
        let msg = &sink.diagnostics[0].message;
        assert!(msg.contains("O1520"), "expected O1520, got: {}", msg);
        assert!(msg.contains("align-pad"));
        assert!(msg.contains("imm=8"));
    }

    /// Negative: pre-call `Sub Rsp,8` paired with post-call
    /// `Add Rsp,16` must NOT match (imm mismatch — the pad is asymmetric
    /// and the caller's rsp balance would drift by 8 across the branch).
    /// No other pattern applies either (Call's successor is Add, not
    /// Ret), so the window is left intact and no diagnostics fire.
    #[test]
    fn wave26_preserves_mismatched_align_pad_imm() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let sub_id = IrNodeId::new(1).unwrap();
        let call_id = IrNodeId::new(2).unwrap();
        let add_id = IrNodeId::new(3).unwrap();
        let ret_id = IrNodeId::new(4).unwrap();

        arena.instructions_mut().insert(sub_id, sub_rsp_imm_inst(8));
        arena.instructions_mut().insert(call_id, call_reg_inst(0));
        arena.instructions_mut().insert(add_id, add_rsp_imm_inst(16));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena
            .instr_owner_mut()
            .insert(call_id, "vops_read".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(5).unwrap(), &mut sink);

        assert!(!changed, "mismatched-imm align pad must not rewrite");
        assert_eq!(
            arena.instructions().get(sub_id).unwrap().mnemonic,
            Mnemonic::Sub
        );
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert_eq!(
            arena.instructions().get(add_id).unwrap().mnemonic,
            Mnemonic::Add
        );
        assert!(arena.instructions().get(ret_id).is_some());
        assert!(sink.diagnostics.is_empty());
    }

    // ── paideia-as#1551 — Shape E (push/pop-bracketed direct) ────────

    /// Positive: `Push rbx; Call SymbolRef(nvme_get_log_page); Pop rbx;
    /// Ret` → `Pop rbx; Jmp SymbolRef(nvme_get_log_page)`. The Push is
    /// mutated to a Pop (restore the callee-save value before the tail
    /// branch); Call becomes Jmp; the trailing Pop and Ret are elided.
    /// The rewrite fires only when both brackets name the same register
    /// (r3 = rbx here).
    #[test]
    fn wave26_rewrites_push_pop_bracketed_direct_tail_call() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let push_id = IrNodeId::new(1).unwrap();
        let call_id = IrNodeId::new(2).unwrap();
        let pop_id = IrNodeId::new(3).unwrap();
        let ret_id = IrNodeId::new(4).unwrap();

        arena.instructions_mut().insert(push_id, push_reg_inst(3));
        arena
            .instructions_mut()
            .insert(call_id, call_to("nvme_get_log_page"));
        arena.instructions_mut().insert(pop_id, pop_reg_inst(3));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena
            .instr_owner_mut()
            .insert(call_id, "nvme_log_smart_fetch".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(5).unwrap(), &mut sink);

        assert!(changed, "symmetric push/pop bracketed direct must rewrite");

        // First window slot mutated Push → Pop, targeting the same reg.
        let head = arena.instructions().get(push_id).unwrap();
        assert_eq!(head.mnemonic, Mnemonic::Pop);
        assert!(matches!(head.operands.first(), Some(Operand::Reg(RegId(3)))));

        // Call → Jmp; the SymbolRef target is preserved verbatim.
        let branch = arena.instructions().get(call_id).unwrap();
        assert_eq!(branch.mnemonic, Mnemonic::Jmp);
        match branch.operands.first() {
            Some(Operand::SymbolRef { name, addend }) => {
                assert_eq!(name, "nvme_get_log_page");
                assert_eq!(*addend, 0);
            }
            other => panic!("expected SymbolRef target, got {:?}", other),
        }

        // Trailing Pop and Ret are gone.
        assert!(arena.instructions().get(pop_id).is_none());
        assert!(arena.instructions().get(ret_id).is_none());

        assert_eq!(sink.diagnostics.len(), 1);
        let msg = &sink.diagnostics[0].message;
        assert!(msg.contains("O1522"), "expected O1522, got: {}", msg);
        assert!(msg.contains("push-pop-bracket"));
        assert!(msg.contains("saved=r3"));
        assert!(msg.contains("nvme_get_log_page"));
    }

    /// Negative: asymmetric brackets (Push rbx, Pop rcx around a direct
    /// call) must NOT rewrite — the mirror invariance of Pattern A/E
    /// requires both bracket regs to be identical, else the epilogue
    /// windows leave the machine in different states. Nothing else in
    /// the pass matches `[Push, Call sym, Pop, Ret]` when the pop reg
    /// disagrees, so the window is preserved intact.
    #[test]
    fn wave26_preserves_asymmetric_push_pop_bracket() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let push_id = IrNodeId::new(1).unwrap();
        let call_id = IrNodeId::new(2).unwrap();
        let pop_id = IrNodeId::new(3).unwrap();
        let ret_id = IrNodeId::new(4).unwrap();

        // Push r3 (rbx), then Pop r1 (rcx) — different regs.
        arena.instructions_mut().insert(push_id, push_reg_inst(3));
        arena
            .instructions_mut()
            .insert(call_id, call_to("nvme_get_log_page"));
        arena.instructions_mut().insert(pop_id, pop_reg_inst(1));
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena
            .instr_owner_mut()
            .insert(call_id, "nvme_log_smart_fetch".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(5).unwrap(), &mut sink);

        assert!(!changed, "asymmetric push/pop bracket must not rewrite");
        assert_eq!(
            arena.instructions().get(push_id).unwrap().mnemonic,
            Mnemonic::Push
        );
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert_eq!(
            arena.instructions().get(pop_id).unwrap().mnemonic,
            Mnemonic::Pop
        );
        assert!(arena.instructions().get(ret_id).is_some());
        assert!(sink.diagnostics.is_empty());
    }

    // ── paideia-as#1555 — Shape E' (trailing pop-bracket, upstream Push) ──

    /// Positive: `push rbx (prologue); mov; mov; call sym; pop rbx; ret`
    /// → `push rbx (prologue); mov; mov; pop rbx; jmp sym`. The upstream
    /// Push stays put (belongs to the function's prologue); the trailing
    /// 3-inst window is rewritten so the callee-save restore precedes the
    /// tail branch. Motivating physical shape at
    /// `nvme_log_smart_fetch` / `nvme_log_error_info_fetch` in
    /// paideia-os/src/kernel/core/cap/nvme_admin_events.pdx.
    #[test]
    fn wave27_shape_e_prime_rewrites_trailing_pop_bracket_with_upstream_push() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let push_id = IrNodeId::new(1).unwrap();
        let mov1_id = IrNodeId::new(2).unwrap();
        let mov2_id = IrNodeId::new(3).unwrap();
        let call_id = IrNodeId::new(4).unwrap();
        let pop_id = IrNodeId::new(5).unwrap();
        let ret_id = IrNodeId::new(6).unwrap();

        arena.instructions_mut().insert(push_id, push_reg_inst(3));
        arena.instructions_mut().insert(mov1_id, mov_inst());
        arena.instructions_mut().insert(mov2_id, mov_inst());
        arena
            .instructions_mut()
            .insert(call_id, call_to("nvme_get_log_page"));
        arena.instructions_mut().insert(pop_id, pop_reg_inst(3));
        arena.instructions_mut().insert(ret_id, ret_inst());

        // Every instruction owned by the same enclosing function — the
        // backward walk stops at the owner boundary otherwise.
        for id in [push_id, mov1_id, mov2_id, call_id, pop_id, ret_id] {
            arena
                .instr_owner_mut()
                .insert(id, "nvme_log_smart_fetch".to_string());
        }

        let changed = pass.apply(&mut arena, IrNodeId::new(7).unwrap(), &mut sink);

        assert!(
            changed,
            "Shape E' 3-inst window with upstream Push must rewrite"
        );

        // Upstream Push preserved verbatim.
        let head = arena.instructions().get(push_id).unwrap();
        assert_eq!(head.mnemonic, Mnemonic::Push);
        assert!(matches!(head.operands.first(), Some(Operand::Reg(RegId(3)))));

        // Intervening Movs untouched.
        assert_eq!(
            arena.instructions().get(mov1_id).unwrap().mnemonic,
            Mnemonic::Mov
        );
        assert_eq!(
            arena.instructions().get(mov2_id).unwrap().mnemonic,
            Mnemonic::Mov
        );

        // Call slot mutated to Pop r3 (restore before branch).
        let pop_now = arena.instructions().get(call_id).unwrap();
        assert_eq!(pop_now.mnemonic, Mnemonic::Pop);
        assert!(matches!(
            pop_now.operands.first(),
            Some(Operand::Reg(RegId(3)))
        ));

        // Pop slot mutated to Jmp SymbolRef(nvme_get_log_page).
        let jmp_now = arena.instructions().get(pop_id).unwrap();
        assert_eq!(jmp_now.mnemonic, Mnemonic::Jmp);
        match jmp_now.operands.first() {
            Some(Operand::SymbolRef { name, addend }) => {
                assert_eq!(name, "nvme_get_log_page");
                assert_eq!(*addend, 0);
            }
            other => panic!("expected SymbolRef target, got {:?}", other),
        }

        // Trailing Ret is gone.
        assert!(arena.instructions().get(ret_id).is_none());

        assert_eq!(sink.diagnostics.len(), 1);
        let msg = &sink.diagnostics[0].message;
        assert!(msg.contains("O1524"), "expected O1524, got: {}", msg);
        assert!(msg.contains("trailing pop-bracket"), "shape name: {}", msg);
        assert!(msg.contains("saved=r3"));
        assert!(msg.contains("nvme_get_log_page"));
        assert!(msg.contains(&format!("prologue push i{}", push_id.get())));
    }

    /// Negative: an intra-function branch (Jmp) between the upstream Push
    /// and the Call breaks the leaf-ish assumption Shape E' relies on —
    /// execution could reach the trailing window without the Push being
    /// run. The pass refuses and emits an O1516 diagnostic naming the
    /// branch-in-span reason; the 3-inst window is left intact.
    #[test]
    fn wave27_shape_e_prime_refuses_when_branch_intervenes_between_push_and_call() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let push_id = IrNodeId::new(1).unwrap();
        let jmp_id = IrNodeId::new(2).unwrap();
        let call_id = IrNodeId::new(3).unwrap();
        let pop_id = IrNodeId::new(4).unwrap();
        let ret_id = IrNodeId::new(5).unwrap();

        arena.instructions_mut().insert(push_id, push_reg_inst(3));
        arena
            .instructions_mut()
            .insert(jmp_id, jmp_sym_inst(".Lbody"));
        arena
            .instructions_mut()
            .insert(call_id, call_to("nvme_get_log_page"));
        arena.instructions_mut().insert(pop_id, pop_reg_inst(3));
        arena.instructions_mut().insert(ret_id, ret_inst());
        for id in [push_id, jmp_id, call_id, pop_id, ret_id] {
            arena
                .instr_owner_mut()
                .insert(id, "nvme_log_smart_fetch".to_string());
        }

        let changed = pass.apply(&mut arena, IrNodeId::new(6).unwrap(), &mut sink);

        assert!(!changed, "intervening branch must block Shape E' rewrite");

        // Every instruction in the window is preserved.
        assert_eq!(
            arena.instructions().get(push_id).unwrap().mnemonic,
            Mnemonic::Push
        );
        assert_eq!(
            arena.instructions().get(jmp_id).unwrap().mnemonic,
            Mnemonic::Jmp
        );
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert_eq!(
            arena.instructions().get(pop_id).unwrap().mnemonic,
            Mnemonic::Pop
        );
        assert!(arena.instructions().get(ret_id).is_some());

        // Refusal diagnostic surfaced.
        assert_eq!(sink.diagnostics.len(), 1);
        let msg = &sink.diagnostics[0].message;
        assert!(msg.contains("O1516"), "expected O1516, got: {}", msg);
        assert!(msg.contains("trailing pop-bracket"), "shape name: {}", msg);
        assert!(
            msg.contains("branch between prologue push and call"),
            "reason: {}",
            msg
        );
    }

    /// Negative: an `IrKind::Handle` node whose id lands between the
    /// upstream Push and the Call blocks Shape E' — the tail branch's
    /// leading Pop would restore a callee-save value from before the
    /// handler frame's teardown. The widened B3-002 guard
    /// (`tco_arena_blocker_with_earlier`) catches this and the pass
    /// emits an O1516 diagnostic naming the handler-install boundary.
    ///
    /// A `mov` instruction sits between the Push and Call so Shape E's
    /// 4-inst adjacent window does not preempt Shape E' — this test
    /// specifically exercises the 3-inst window plus upstream Push walk.
    #[test]
    fn wave27_shape_e_prime_refuses_when_handler_installed_before_call() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        // Allocate arena ids 1..=6 so a real `IrKind::Handle` node can
        // land at id 2 (inside the upstream span [push, call) once ids
        // are populated).
        //  id 1 push-slot  → push rbx
        //  id 2 Handle     → NO instruction; only the IrKind matters
        //  id 3 mov-slot   → mov (interior body instruction, defeats
        //                    Shape E's 4-inst adjacency check)
        //  id 4 call-slot  → call nvme_get_log_page
        //  id 5 pop-slot   → pop rbx
        //  id 6 ret-slot   → ret
        let push_node = arena.alloc(IrKind::App, span());
        let handle_node = arena.alloc(IrKind::Handle, span());
        let mov_node = arena.alloc(IrKind::App, span());
        let call_node = arena.alloc(IrKind::App, span());
        let pop_node = arena.alloc(IrKind::App, span());
        let ret_node = arena.alloc(IrKind::Placeholder, span());
        assert_eq!(push_node.get(), 1);
        assert_eq!(handle_node.get(), 2);
        assert_eq!(mov_node.get(), 3);
        assert_eq!(call_node.get(), 4);
        assert_eq!(pop_node.get(), 5);
        assert_eq!(ret_node.get(), 6);

        arena.instructions_mut().insert(push_node, push_reg_inst(3));
        arena.instructions_mut().insert(mov_node, mov_inst());
        arena
            .instructions_mut()
            .insert(call_node, call_to("nvme_get_log_page"));
        arena.instructions_mut().insert(pop_node, pop_reg_inst(3));
        arena.instructions_mut().insert(ret_node, ret_inst());
        for id in [push_node, mov_node, call_node, pop_node, ret_node] {
            arena
                .instr_owner_mut()
                .insert(id, "nvme_log_smart_fetch".to_string());
        }

        let changed = pass.apply(&mut arena, IrNodeId::new(7).unwrap(), &mut sink);

        assert!(
            !changed,
            "handler-install between prologue push and call must block Shape E'"
        );

        // Window preserved intact.
        assert_eq!(
            arena.instructions().get(push_node).unwrap().mnemonic,
            Mnemonic::Push
        );
        assert_eq!(
            arena.instructions().get(mov_node).unwrap().mnemonic,
            Mnemonic::Mov
        );
        assert_eq!(
            arena.instructions().get(call_node).unwrap().mnemonic,
            Mnemonic::Call
        );
        assert_eq!(
            arena.instructions().get(pop_node).unwrap().mnemonic,
            Mnemonic::Pop
        );
        assert!(arena.instructions().get(ret_node).is_some());

        // O1516 refusal names the handler-install boundary.
        assert_eq!(sink.diagnostics.len(), 1);
        let msg = &sink.diagnostics[0].message;
        assert!(msg.contains("O1516"), "expected O1516, got: {}", msg);
        assert!(msg.contains("trailing pop-bracket"), "shape name: {}", msg);
        assert!(
            msg.contains("handler-install boundary"),
            "reason: {}",
            msg
        );
    }

    /// Guard-helper unit test: `tco_arena_blocker_with_earlier` returns
    /// `None` when the earlier span is `None` (backward-compatibility
    /// alias for `tco_arena_blocker`) and reports the handler boundary
    /// when a Handle node lands inside `[earlier_span_start, call_id)`.
    #[test]
    fn wave27_widened_blocker_matches_base_when_earlier_span_none() {
        let mut arena = IrArena::new();

        // Layout: 1 push-slot, 2 Handle, 3 Call, 4 Ret.
        let push_node = arena.alloc(IrKind::App, span());
        let handle_node = arena.alloc(IrKind::Handle, span());
        let call_node = arena.alloc(IrKind::App, span());
        let ret_node = arena.alloc(IrKind::Placeholder, span());
        assert_eq!(push_node.get(), 1);
        assert_eq!(handle_node.get(), 2);
        assert_eq!(call_node.get(), 3);
        assert_eq!(ret_node.get(), 4);

        // Without the earlier-span hint, the Handle at id 2 lives
        // outside (call_id, ret_id) = (3, 4) → base blocker sees no
        // handler evidence.
        let base = tco_arena_blocker_with_earlier(
            &arena, None, call_node, ret_node, "f",
        );
        assert_eq!(base, None);

        // With `earlier_span_start = push_node` the widened span
        // [1, 3) catches the Handle at id 2.
        let widened = tco_arena_blocker_with_earlier(
            &arena,
            Some(push_node),
            call_node,
            ret_node,
            "f",
        );
        assert_eq!(widened, Some(TcoBlocker::EffectHandlerInstalling));
    }
}
