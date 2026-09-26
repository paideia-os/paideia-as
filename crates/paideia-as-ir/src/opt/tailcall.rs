//! Tail-call elimination.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::instruction::{Mnemonic, Operand};
use crate::node::IrNodeId;

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
/// Takes an instruction side-table and a call site node ID;
/// returns whether the call is eligible for TCO (None), or the blocker (Some).
///
/// Phase-3-m3-005: structural precondition checks only.
///
/// Phase-4-m1-004: per-branch walker visibility is in place via Branch walker
/// support; branch-arm recursion is analysable.
///
/// PAS-DEBT-B3-001: recursion identity is now handled by the pass itself via
/// `instr_owner` + `SymbolRef` name match — see `apply()`. This helper stays
/// focused on the capability / handler / ABI / frame blockers surfaced by the
/// side-table; those are still stubbed pending B3-002.
pub fn tco_blocker(
    _side_table: &crate::instruction::InstructionSideTable,
    _call_id: crate::node::IrNodeId,
) -> Option<TcoBlocker> {
    // B3-002: extract blockers from the side-table + call site.
    None
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
fn call_target_symbol(inst: &crate::instruction::Instruction) -> Option<&str> {
    match inst.operands.first()? {
        Operand::SymbolRef { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

impl OptPass for TailCallPass {
    fn name(&self) -> &'static str {
        "tailcall"
    }

    /// PAS-DEBT-B3-001: rewrite only `Call target; Ret` where `target` is a
    /// direct `SymbolRef` naming the enclosing function (self-recursion).
    /// Mutual recursion, indirect calls, and non-tail self-calls are left
    /// alone. Ownership evidence comes from `arena.instr_owner()`; an absent
    /// owner is treated as "cannot prove self-recursion" and skipped.
    fn apply(&self, arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        let mut ids: Vec<IrNodeId> = arena.instructions().entries().keys().copied().collect();
        ids.sort_by_key(|id| id.get());

        if ids.len() < 2 {
            return false;
        }

        let mut changed = false;

        let mut i = 0;
        while i < ids.len() - 1 {
            let call_id = ids[i];
            let next_id = ids[i + 1];

            // Snapshot call target name + ret-follows before any mutation.
            let (target_name, is_ret) = {
                let table = arena.instructions();
                let call_opt = table.get(call_id);
                let next_opt = table.get(next_id);
                let name = call_opt
                    .filter(|c| c.mnemonic == Mnemonic::Call)
                    .and_then(call_target_symbol)
                    .map(str::to_string);
                let ret = next_opt.map(|n| n.mnemonic == Mnemonic::Ret).unwrap_or(false);
                (name, ret)
            };

            let Some(target_name) = target_name else {
                i += 1;
                continue;
            };

            if !is_ret {
                i += 1;
                continue;
            }

            // Self-recursion gate: owner must match the call target.
            let is_self_recursion = arena
                .instr_owner()
                .get(call_id)
                .map(|owner| owner == target_name.as_str())
                .unwrap_or(false);

            if !is_self_recursion {
                i += 1;
                continue;
            }

            // Structural blockers (capability boundary, ABI mismatch, ...).
            if tco_blocker(arena.instructions(), call_id).is_some() {
                i += 1;
                continue;
            }

            // Rewrite: Call → Jmp; drop the trailing Ret.
            if let Some(inst) = arena.instructions_mut().get_mut(call_id) {
                inst.mnemonic = Mnemonic::Jmp;
            }
            arena.instructions_mut().remove(next_id);

            sink.emit(
                "tailcall",
                format!(
                    "O1514: TCO self-recursion Call→Jmp i{} + remove Ret i{} (owner={}, target={})",
                    call_id.get(),
                    next_id.get(),
                    target_name,
                    target_name,
                ),
            );

            changed = true;
            // Re-check the same index: ids[i+1] was removed from the table
            // but the local ids vec still references it, so advance past it.
            i += 2;
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::{InstrMode, Instruction, InstructionSideTable, Operand, RegId};
    use smallvec::SmallVec;

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

    // ── Negative: indirect (register) call is preserved ────────────

    #[test]
    fn tco_preserves_indirect_call() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        // Indirect call: register operand, not SymbolRef.
        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ops.push(Operand::Reg(RegId(0)));
        arena.instructions_mut().insert(
            call_id,
            Instruction {
                mnemonic: Mnemonic::Call,
                operands: ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::default(),
                emission_order: 0,
            },
        );
        arena.instructions_mut().insert(ret_id, ret_inst());
        arena.instr_owner_mut().insert(call_id, "foo".to_string());

        let changed = pass.apply(&mut arena, IrNodeId::new(3).unwrap(), &mut sink);

        assert!(!changed, "indirect call must not be rewritten");
        assert_eq!(
            arena.instructions().get(call_id).unwrap().mnemonic,
            Mnemonic::Call
        );
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
}
