//! Tail-call elimination.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::instruction::Mnemonic;
use crate::node::IrNodeId;

#[cfg(test)]
use crate::instruction::InstrMode;

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
/// Phase-3-m3-005: Minimum implementation checks structural preconditions.
/// Recursion detection (call target == enclosing function symbol) is TODO
/// pending elaborator chokepoint that surfaces call-target symbol in IR.
///
/// Phase-4-m1-004: Per-branch walker visibility is now in place via Branch walker
/// support. Recursion checks can now properly account for conditional branches
/// where recursion occurs only in specific arms (then-arm or else-arm).
pub fn tco_blocker(
    _side_table: &crate::instruction::InstructionSideTable,
    _call_id: crate::node::IrNodeId,
) -> Option<TcoBlocker> {
    // Phase-3-m3-005: TODO extract capability boundary, handler install,
    // ABI mismatch, and frame layout info from the side-table and call site.
    // For now: always eligible (None), placeholder for blockers.
    // Phase-4-m1-004: Recursion detection gate is lifting; branch-aware analysis pending.
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

impl OptPass for TailCallPass {
    fn name(&self) -> &'static str {
        "tailcall"
    }

    fn apply(&self, arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        // Collect all instruction node ids from the table and sort by id.
        let mut ids: Vec<IrNodeId> = {
            let table = arena.instructions();
            table.entries().keys().copied().collect()
        };
        ids.sort_by_key(|id| id.get());

        if ids.is_empty() {
            return false;
        }

        let mut changed = false;

        // Iterate through instructions looking for Call + Ret patterns.
        let mut i = 0;
        while i < ids.len() - 1 {
            let call_id = ids[i];
            let next_id = ids[i + 1];

            // Check if current instruction is a Call.
            let is_call = {
                let table = arena.instructions();
                table
                    .get(call_id)
                    .map(|inst| inst.mnemonic == Mnemonic::Call)
                    .unwrap_or(false)
            };

            if !is_call {
                i += 1;
                continue;
            }

            // Check if next instruction is a Ret.
            let is_ret = {
                let table = arena.instructions();
                table
                    .get(next_id)
                    .map(|inst| inst.mnemonic == Mnemonic::Ret)
                    .unwrap_or(false)
            };

            if !is_ret {
                i += 1;
                continue;
            }

            // Check for blockers on this call.
            let blocked = {
                let table = arena.instructions();
                tco_blocker(table, call_id).is_some()
            };

            if blocked {
                i += 1;
                continue;
            }

            // Pattern matched: Call followed by Ret and not blocked.
            // Rewrite Call → Jmp and remove Ret.
            {
                let table = arena.instructions_mut();
                if let Some(inst) = table.get_mut(call_id) {
                    inst.mnemonic = Mnemonic::Jmp;
                }
            }

            // Remove the Ret instruction.
            arena.instructions_mut().remove(next_id);

            sink.emit(
                "tailcall",
                format!(
                    "O1510: TCO rewrite Call→Jmp i{} + remove Ret i{}",
                    call_id.get(),
                    next_id.get()
                ),
            );

            changed = true;
            // Don't increment i; re-check from the same position in case
            // the next instruction has shifted.
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::{InstrMode, Instruction, InstructionSideTable, Operand, RegId};
    use smallvec::SmallVec;

    #[test]
    fn tco_blocker_returns_none_when_eligible() {
        let result = tco_blocker_impl(false, false, false, false);
        assert_eq!(result, None);
    }

    #[test]
    fn tco_blocker_returns_capability_boundary() {
        let result = tco_blocker_impl(true, false, false, false);
        assert_eq!(result, Some(TcoBlocker::CapabilityBoundary));
    }

    #[test]
    fn tco_blocker_returns_effect_handler_installing() {
        let result = tco_blocker_impl(false, true, false, false);
        assert_eq!(result, Some(TcoBlocker::EffectHandlerInstalling));
    }

    #[test]
    fn tco_blocker_returns_different_call_convention() {
        let result = tco_blocker_impl(false, false, true, false);
        assert_eq!(result, Some(TcoBlocker::DifferentCallConvention));
    }

    #[test]
    fn tco_blocker_returns_frame_requires_epilogue() {
        let result = tco_blocker_impl(false, false, false, true);
        assert_eq!(result, Some(TcoBlocker::FrameRequiresEpilogue));
    }

    #[test]
    fn tco_blocker_with_instruction_side_table() {
        let mut table = InstructionSideTable::new();

        let call_id = IrNodeId::new(1).unwrap();

        // Populate table with a call instruction
        table.insert(
            call_id,
            Instruction {
                mnemonic: Mnemonic::Call,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(0)));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::default(),
            },
        );

        // Call the new signature; verify it accepts the table.
        let _result = tco_blocker(&table, call_id);
        // Phase-3-m3-005 stub: currently always returns None (eligible).
    }

    #[test]
    fn tco_pass_rewrites_call_followed_by_ret_to_jmp() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        // Add Call instruction.
        arena.instructions_mut().insert(
            call_id,
            Instruction {
                mnemonic: Mnemonic::Call,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(0)));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::default(),
            },
        );

        // Add Ret instruction after Call.
        arena.instructions_mut().insert(
            ret_id,
            Instruction {
                mnemonic: Mnemonic::Ret,
                operands: SmallVec::new(),
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::default(),
            },
        );

        let dummy_root = IrNodeId::new(3).unwrap();
        let changed = pass.apply(&mut arena, dummy_root, &mut sink);

        assert!(changed, "TailCallPass should rewrite Call+Ret");
        // Call should be converted to Jmp.
        let call_inst = arena.instructions().get(call_id).unwrap();
        assert_eq!(call_inst.mnemonic, Mnemonic::Jmp);
        // Ret should be removed.
        assert!(
            arena.instructions().get(ret_id).is_none(),
            "Ret instruction should be removed"
        );
    }

    #[test]
    fn tco_pass_emits_o1510_per_rewrite() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let ret_id = IrNodeId::new(2).unwrap();

        // Add Call instruction.
        arena.instructions_mut().insert(
            call_id,
            Instruction {
                mnemonic: Mnemonic::Call,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(0)));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::default(),
            },
        );

        // Add Ret instruction after Call.
        arena.instructions_mut().insert(
            ret_id,
            Instruction {
                mnemonic: Mnemonic::Ret,
                operands: SmallVec::new(),
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::default(),
            },
        );

        let dummy_root = IrNodeId::new(3).unwrap();
        let changed = pass.apply(&mut arena, dummy_root, &mut sink);

        assert!(changed, "TailCallPass should fire");
        assert_eq!(
            sink.diagnostics.len(),
            1,
            "Should emit exactly one O1510 diagnostic"
        );
        assert_eq!(sink.diagnostics[0].pass, "tailcall");
        assert!(
            sink.diagnostics[0].message.contains("O1510"),
            "Diagnostic should mention O1510"
        );
    }

    #[test]
    fn tco_pass_preserves_call_not_followed_by_ret() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let call_id = IrNodeId::new(1).unwrap();
        let other_id = IrNodeId::new(2).unwrap();

        // Add Call instruction.
        arena.instructions_mut().insert(
            call_id,
            Instruction {
                mnemonic: Mnemonic::Call,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(0)));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::default(),
            },
        );

        // Add a non-Ret instruction after Call (e.g., Mov).
        arena.instructions_mut().insert(
            other_id,
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(0)));
                    ops.push(Operand::Reg(RegId(1)));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::default(),
            },
        );

        let dummy_root = IrNodeId::new(3).unwrap();
        let changed = pass.apply(&mut arena, dummy_root, &mut sink);

        assert!(
            !changed,
            "TailCallPass should not rewrite Call not followed by Ret"
        );
        // Call should remain a Call.
        let call_inst = arena.instructions().get(call_id).unwrap();
        assert_eq!(call_inst.mnemonic, Mnemonic::Call);
        // Other instruction should remain.
        assert!(
            arena.instructions().get(other_id).is_some(),
            "Other instruction should be preserved"
        );
        assert_eq!(
            sink.diagnostics.len(),
            0,
            "No diagnostics should be emitted"
        );
    }

    #[test]
    fn tco_pass_emits_no_diagnostics_for_empty_arena() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_root = IrNodeId::new(1).unwrap();
        let changed = pass.apply(&mut arena, dummy_root, &mut sink);

        assert!(!changed, "Empty arena should produce no changes");
        assert_eq!(
            sink.diagnostics.len(),
            0,
            "Empty arena should produce no diagnostics"
        );
    }
}
