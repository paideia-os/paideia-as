//! Tail-call elimination.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::instruction::{Mnemonic, Operand};
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
            let owner_snapshot = arena.instr_owner().get(call_id).map(str::to_string);
            let is_self_recursion = owner_snapshot
                .as_deref()
                .map(|owner| owner == target_name.as_str())
                .unwrap_or(false);

            if !is_self_recursion {
                i += 1;
                continue;
            }

            // Owner is Some here (self-recursion required it).
            let owner_name = owner_snapshot.unwrap();

            // PAS-DEBT-B3-002: capability / handler-install / effect-row blocker.
            if let Some(reason) = tco_arena_blocker(arena, call_id, next_id, &owner_name) {
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
}
