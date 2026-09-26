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

impl OptPass for TailCallPass {
    fn name(&self) -> &'static str {
        "tailcall"
    }

    /// Rewrite three tail-call shapes; every other pattern is left alone.
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
    /// All three shapes are still gated by [`tco_arena_blocker`]:
    /// capability-boundary and effect-handler-install checks apply
    /// uniformly to direct and indirect tail-calls.
    fn apply(&self, arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        let mut ids: Vec<IrNodeId> = arena.instructions().entries().keys().copied().collect();
        ids.sort_by_key(|id| id.get());

        if ids.len() < 2 {
            return false;
        }

        let mut changed = false;
        let mut i = 0;

        while i < ids.len() {
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
}
