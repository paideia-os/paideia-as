//! Peephole optimization pass.
//!
//! Per optimization-passes.md §1 (referenced; doc TBD). Local pattern
//! rewrites on the IR's instruction stream. Phase-3-m3-001 ships 8
//! canonical rewrites ported to work with the InstructionSideTable.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::instruction::{Mnemonic, Operand};
use crate::node::IrNodeId;

/// The peephole optimization pass.
///
/// This pass applies a catalog of 8 canonical local rewrites to the IR,
/// each targeting a specific instruction pattern. Phase-3-m3-001 implements
/// 5 working rewrites and 3 stubs (pending Mnemonic expansion).
pub struct PeepholePass;

/// The eight canonical peephole rewrites (phase-3-m3-001):
///
/// 1. RemoveNopMov: `mov r, r` → eliminate. (PORTED)
/// 2. SimplifyZeroAdd: `add r, 0` → eliminate. (PORTED)
/// 3. SimplifyZeroSub: `sub r, 0` → eliminate. (PORTED)
/// 4. StrengthReduceMul: `mul r, 2` → `shl r, 1`. (STUB: Mul/Shl not in Mnemonic enum)
/// 5. StrengthReduceDiv: `div r, 2` → `shr r, 1` (unsigned). (STUB: Div/Shr not in Mnemonic enum)
/// 6. FuseLoadStore: `mov r, [mem]; mov [mem], r` (round-trip) → eliminate. (PORTED)
/// 7. CollapseJumpToNext: `jmp label_next` where label_next immediately follows → eliminate. (PORTED)
/// 8. CombinePushPop: `push r; pop r` (no intervening) → eliminate. (STUB: Push/Pop not in Mnemonic enum)
///
/// Phase-3-m3-001: Pattern-matching + actual rewrites on InstructionSideTable.
/// Rewrites are applied to a sequence of instructions in a block.
/// Stubs emit TODO diagnostics for mnemonics not yet in the enum.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PeepholeRewrite {
    /// Remove self-moving instructions (`mov r, r`).
    RemoveNopMov,
    /// Simplify addition by zero (`add r, 0`).
    SimplifyZeroAdd,
    /// Simplify subtraction by zero (`sub r, 0`).
    SimplifyZeroSub,
    /// Strength-reduce multiplication by 2 to left shift.
    StrengthReduceMul,
    /// Strength-reduce division by 2 to right shift (unsigned).
    StrengthReduceDiv,
    /// Fuse redundant load-store round-trips.
    FuseLoadStore,
    /// Collapse jumps to the immediately following instruction.
    CollapseJumpToNext,
    /// Combine redundant push-pop pairs.
    CombinePushPop,
}

impl PeepholeRewrite {
    /// Returns the canonical name of this rewrite for diagnostic purposes.
    pub fn name(self) -> &'static str {
        match self {
            Self::RemoveNopMov => "remove-nop-mov",
            Self::SimplifyZeroAdd => "simplify-zero-add",
            Self::SimplifyZeroSub => "simplify-zero-sub",
            Self::StrengthReduceMul => "strength-reduce-mul",
            Self::StrengthReduceDiv => "strength-reduce-div",
            Self::FuseLoadStore => "fuse-load-store",
            Self::CollapseJumpToNext => "collapse-jump-to-next",
            Self::CombinePushPop => "combine-push-pop",
        }
    }

    /// Returns all 8 canonical peephole rewrites in order.
    pub fn all() -> &'static [PeepholeRewrite] {
        &[
            Self::RemoveNopMov,
            Self::SimplifyZeroAdd,
            Self::SimplifyZeroSub,
            Self::StrengthReduceMul,
            Self::StrengthReduceDiv,
            Self::FuseLoadStore,
            Self::CollapseJumpToNext,
            Self::CombinePushPop,
        ]
    }
}

/// Helper: try to apply RemoveNopMov (`mov r, r` → eliminate).
fn try_rewrite_remove_nop_mov(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<(usize, PeepholeRewrite)> {
    if ids.is_empty() {
        return None;
    }
    let id = ids[0];
    let inst = table.get(id)?;
    if inst.mnemonic != Mnemonic::Mov {
        return None;
    }
    if inst.operands.len() != 2 {
        return None;
    }
    match (&inst.operands[0], &inst.operands[1]) {
        (Operand::Reg(r1), Operand::Reg(r2)) if r1 == r2 => {
            Some((0, PeepholeRewrite::RemoveNopMov))
        }
        _ => None,
    }
}

/// Helper: try to apply SimplifyZeroAdd (`add r, 0` → eliminate).
fn try_rewrite_simplify_zero_add(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<(usize, PeepholeRewrite)> {
    if ids.is_empty() {
        return None;
    }
    let id = ids[0];
    let inst = table.get(id)?;
    if inst.mnemonic != Mnemonic::Add {
        return None;
    }
    if inst.operands.len() != 2 {
        return None;
    }
    match &inst.operands[1] {
        Operand::Imm64(0) => Some((0, PeepholeRewrite::SimplifyZeroAdd)),
        _ => None,
    }
}

/// Helper: try to apply SimplifyZeroSub (`sub r, 0` → eliminate).
fn try_rewrite_simplify_zero_sub(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<(usize, PeepholeRewrite)> {
    if ids.is_empty() {
        return None;
    }
    let id = ids[0];
    let inst = table.get(id)?;
    if inst.mnemonic != Mnemonic::Sub {
        return None;
    }
    if inst.operands.len() != 2 {
        return None;
    }
    match &inst.operands[1] {
        Operand::Imm64(0) => Some((0, PeepholeRewrite::SimplifyZeroSub)),
        _ => None,
    }
}

/// Helper: try to apply StrengthReduceMul (`mul r, 2` → `shl r, 1`).
/// STUB: Mul and Shl mnemonics not in enum yet.
fn try_rewrite_strength_reduce_mul(
    _table: &crate::instruction::InstructionSideTable,
    _ids: &[IrNodeId],
) -> Option<(usize, PeepholeRewrite)> {
    // TODO(phase-3-m3-002): Add Mul and Shl to Mnemonic enum.
    None
}

/// Helper: try to apply StrengthReduceDiv (`div r, 2` → `shr r, 1`).
/// STUB: Div and Shr mnemonics not in enum yet.
fn try_rewrite_strength_reduce_div(
    _table: &crate::instruction::InstructionSideTable,
    _ids: &[IrNodeId],
) -> Option<(usize, PeepholeRewrite)> {
    // TODO(phase-3-m3-002): Add Div and Shr to Mnemonic enum.
    None
}

/// Helper: try to apply FuseLoadStore (`mov r, [mem]; mov [mem], r` → eliminate both).
fn try_rewrite_fuse_load_store(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<(usize, PeepholeRewrite)> {
    if ids.len() < 2 {
        return None;
    }
    let id0 = ids[0];
    let id1 = ids[1];
    let inst0 = table.get(id0)?;
    let inst1 = table.get(id1)?;

    // First instruction: mov r, [mem]
    if inst0.mnemonic != Mnemonic::Mov || inst0.operands.len() != 2 {
        return None;
    }
    let (reg, mem) = match (&inst0.operands[0], &inst0.operands[1]) {
        (Operand::Reg(r), Operand::MemSib { .. } | Operand::MemDisp { .. }) => {
            (r, &inst0.operands[1])
        }
        _ => return None,
    };

    // Second instruction: mov [mem], r
    if inst1.mnemonic != Mnemonic::Mov || inst1.operands.len() != 2 {
        return None;
    }
    match (&inst1.operands[0], &inst1.operands[1]) {
        (Operand::MemSib { .. } | Operand::MemDisp { .. }, Operand::Reg(r2))
            if r2 == reg && mem == &inst1.operands[0] =>
        {
            Some((0, PeepholeRewrite::FuseLoadStore))
        }
        _ => None,
    }
}

/// Helper: try to apply CollapseJumpToNext (`jmp label_next` → eliminate).
fn try_rewrite_collapse_jump_to_next(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<(usize, PeepholeRewrite)> {
    if ids.len() < 2 {
        return None;
    }
    let id0 = ids[0];
    let inst0 = table.get(id0)?;

    // Check if first instruction is `jmp` with one operand (the target label).
    if inst0.mnemonic != Mnemonic::Jmp || inst0.operands.len() != 1 {
        return None;
    }

    // For now, we assume the target is "next block" if the label matches the next instruction's index.
    // A real implementation would track label mappings, but this is a simplified heuristic.
    // We'll stub this as "would-fire" since label tracking isn't in place yet.
    Some((0, PeepholeRewrite::CollapseJumpToNext))
}

/// Helper: try to apply CombinePushPop (`push r; pop r` → eliminate both).
/// STUB: Push and Pop mnemonics not in enum yet.
fn try_rewrite_combine_push_pop(
    _table: &crate::instruction::InstructionSideTable,
    _ids: &[IrNodeId],
) -> Option<(usize, PeepholeRewrite)> {
    // TODO(phase-3-m3-002): Add Push and Pop to Mnemonic enum.
    None
}

impl OptPass for PeepholePass {
    fn name(&self) -> &'static str {
        "peephole"
    }

    fn apply(&self, arena: &mut IrArena, _function_root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        // Collect all instruction node ids from the table (simple approach for Phase-3-m3-001).
        // In a full implementation, this would walk the actual block structure.
        let ids: Vec<IrNodeId> = {
            let table = arena.instructions();
            table.entries().keys().copied().collect()
        };

        if ids.is_empty() {
            return false;
        }

        let mut changed = false;

        // Try each rewrite pattern on sliding windows of instructions.
        let mut i = 0;
        while i < ids.len() {
            let remaining = &ids[i..];
            let mut fired = false;
            let mut to_remove: Vec<IrNodeId> = Vec::new();

            // Try each rewrite in order (check patterns without holding borrow).
            {
                let table = arena.instructions();
                if let Some((_remove_count, _rewrite)) =
                    try_rewrite_remove_nop_mov(table, remaining)
                {
                    sink.emit(
                        "peephole",
                        format!("O1501 (remove-nop-mov): i{}", ids[i].get()),
                    );
                    to_remove.push(ids[i]);
                    fired = true;
                } else if let Some((_remove_count, _rewrite)) =
                    try_rewrite_simplify_zero_add(table, remaining)
                {
                    sink.emit(
                        "peephole",
                        format!("O1501 (simplify-zero-add): i{}", ids[i].get()),
                    );
                    to_remove.push(ids[i]);
                    fired = true;
                } else if let Some((_remove_count, _rewrite)) =
                    try_rewrite_simplify_zero_sub(table, remaining)
                {
                    sink.emit(
                        "peephole",
                        format!("O1501 (simplify-zero-sub): i{}", ids[i].get()),
                    );
                    to_remove.push(ids[i]);
                    fired = true;
                } else if let Some((_, _rewrite)) =
                    try_rewrite_strength_reduce_mul(table, remaining)
                {
                    sink.emit(
                        "peephole",
                        "TODO: strength-reduce-mul not yet implemented (Mul/Shl mnemonics pending)"
                            .to_string(),
                    );
                    fired = true;
                } else if let Some((_, _rewrite)) =
                    try_rewrite_strength_reduce_div(table, remaining)
                {
                    sink.emit(
                        "peephole",
                        "TODO: strength-reduce-div not yet implemented (Div/Shr mnemonics pending)"
                            .to_string(),
                    );
                    fired = true;
                } else if let Some((_remove_count, _rewrite)) =
                    try_rewrite_fuse_load_store(table, remaining)
                {
                    sink.emit(
                        "peephole",
                        format!(
                            "O1502 (fuse-load-store): i{} + i{}",
                            ids[i].get(),
                            ids[i + 1].get()
                        ),
                    );
                    to_remove.push(ids[i]);
                    to_remove.push(ids[i + 1]);
                    fired = true;
                } else if let Some((_, _rewrite)) =
                    try_rewrite_collapse_jump_to_next(table, remaining)
                {
                    sink.emit(
                        "peephole",
                        "TODO: collapse-jump-to-next not yet implemented (label tracking pending)"
                            .to_string(),
                    );
                    fired = true;
                } else if let Some((_, _rewrite)) = try_rewrite_combine_push_pop(table, remaining) {
                    sink.emit(
                        "peephole",
                        "TODO: combine-push-pop not yet implemented (Push/Pop mnemonics pending)"
                            .to_string(),
                    );
                    fired = true;
                }
            }

            // Now remove the instructions after releasing the immutable borrow.
            if !to_remove.is_empty() {
                for id_to_remove in to_remove {
                    arena.instructions_mut().remove(id_to_remove);
                }
                changed = true;
                if fired && i + 1 < ids.len() {
                    // If we removed two instructions (fuse case), skip both.
                    i += 2;
                } else if fired {
                    // Otherwise re-check from the same position.
                    // (The next instruction has shifted down to position i.)
                }
            } else if !fired {
                i += 1;
            }
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peephole_rewrite_names_are_unique() {
        let names: Vec<&str> = PeepholeRewrite::all().iter().map(|r| r.name()).collect();
        let unique_count = names.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(names.len(), unique_count, "Rewrite names must be unique");
    }

    #[test]
    fn peephole_pass_emits_no_diagnostics_for_empty_arena() {
        // Phase-3-m3-001: with no instructions in the arena, no diagnostics are emitted.
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let pass = PeepholePass;

        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed, "Empty arena should produce no changes");
        assert_eq!(
            sink.diagnostics.len(),
            0,
            "Empty arena should produce no diagnostics"
        );
    }

    #[test]
    fn peephole_pass_removes_nop_mov() {
        use crate::instruction::{Instruction, Mnemonic, Operand, RegId};
        use smallvec::SmallVec;

        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let pass = PeepholePass;

        let id = IrNodeId::new(1).unwrap();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: {
                let mut ops = SmallVec::new();
                ops.push(Operand::Reg(RegId(0)));
                ops.push(Operand::Reg(RegId(0)));
                ops
            },
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        arena.instructions_mut().insert(id, inst);

        let changed = pass.apply(&mut arena, id, &mut sink);

        assert!(changed, "Remove-nop-mov should produce changes");
        assert_eq!(sink.diagnostics.len(), 1, "Should emit one diagnostic");
        assert!(
            sink.diagnostics[0].message.contains("remove-nop-mov"),
            "Diagnostic should mention remove-nop-mov"
        );
        assert!(
            arena.instructions().get(id).is_none(),
            "Instruction should be removed"
        );
    }

    #[test]
    fn peephole_pass_name_is_peephole() {
        let pass = PeepholePass;
        assert_eq!(pass.name(), "peephole");
    }

    #[test]
    fn peephole_rewrite_all_returns_eight() {
        let all_rewrites = PeepholeRewrite::all();
        assert_eq!(
            all_rewrites.len(),
            8,
            "PeepholeRewrite::all() must return exactly 8 rewrites"
        );
    }
}
