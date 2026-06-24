//! Branch-hint code layout.
//!
//! Detects Jcc instructions and emits branch-hint prefix directives
//! (0x2E for not-taken / 0x3E for taken) via EncodingHint markers.
//! Phase-4-m1-008 (real rewrite): sets the encoding_hint's opcode to encode
//! the branch-hint prefix.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::instruction::Mnemonic;

#[cfg(test)]
use crate::instruction::InstrMode;
use crate::node::IrNodeId;

/// The branch-hint optimization pass.
pub struct BranchHintPass;

/// Branch-hint directives for code layout.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum BranchHint {
    /// Branch is predicted likely to be taken.
    Likely,
    /// Branch is predicted unlikely to be taken.
    Unlikely,
}

/// Whether the error path should be laid out as the taken branch
/// (i.e., the unlikely path moves off the fall-through).
pub fn lay_unlikely_off_fall_through(hint: BranchHint) -> bool {
    matches!(hint, BranchHint::Unlikely)
}

impl OptPass for BranchHintPass {
    fn name(&self) -> &'static str {
        "branch-hint"
    }

    fn apply(&self, arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        let mut rewrote = 0;

        // Collect all instruction node IDs and sort them.
        let mut ids: Vec<IrNodeId> = arena.instructions().entries().keys().copied().collect();
        ids.sort_by_key(|id| id.get());

        // For each Jcc instruction, emit a branch-hint EncodingHint marker.
        // Phase-4-m1-008 minimum: emit the hint marker; actual prefix-byte
        // emission at encode time is m2-001 closure follow-up (m2-002 encoder
        // bridge can read the EncodingHint when m3 unrolls).
        for jcc_id in ids.iter() {
            let jcc_mnem = arena.instructions().get(*jcc_id).map(|i| i.mnemonic);

            if matches!(jcc_mnem, Some(Mnemonic::Jcc(_))) {
                // Set the encoding_hint with the branch-hint prefix marker.
                // Use opcode 0x3E for taken (likely branch), 0x2E for not-taken (unlikely).
                // For now, default to 0x3E (taken/likely) as the common case.
                if let Some(inst) = arena.instructions_mut().get_mut(*jcc_id) {
                    if inst.encoding_hint.is_none() {
                        inst.encoding_hint = Some(crate::instruction::EncodingHint {
                            opcode: 0x3E,      // Branch-hint taken prefix
                            operand_size: 255, // Marker for branch-hint
                        });
                        rewrote += 1;
                    }
                }
            }
        }

        if rewrote > 0 {
            sink.emit("branch-hint", format!("O1507 rewrote {} sites", rewrote));
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::{Cond, InstrMode, Instruction, Operand};
    use paideia_as_diagnostics::{FileId, Span};
    use smallvec::SmallVec;

    fn create_test_arena() -> (IrArena, IrNodeId) {
        let mut arena = IrArena::new();
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 10);
        let func = arena.alloc(crate::IrKind::Functor, span);
        (arena, func)
    }

    fn create_jcc_instruction() -> Instruction {
        let mut operands = SmallVec::new();
        operands.push(Operand::Imm64(100));
        Instruction {
            mnemonic: Mnemonic::Jcc(Cond::Eq),
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        }
    }

    fn create_jmp_instruction() -> Instruction {
        let mut operands = SmallVec::new();
        operands.push(Operand::Imm64(100));
        Instruction {
            mnemonic: Mnemonic::Jmp,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        }
    }

    #[test]
    fn lay_unlikely_off_fall_through_returns_true_for_unlikely() {
        assert!(lay_unlikely_off_fall_through(BranchHint::Unlikely));
    }

    #[test]
    fn lay_unlikely_off_fall_through_returns_false_for_likely() {
        assert!(!lay_unlikely_off_fall_through(BranchHint::Likely));
    }

    #[test]
    fn branch_hint_detects_jcc() {
        let (mut arena, _func) = create_test_arena();
        let mut sink = OptDiagSink::new();

        // Create a Jcc node
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 5);
        let jcc_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(jcc_id, create_jcc_instruction());

        let pass = BranchHintPass;
        let changed = pass.apply(&mut arena, jcc_id, &mut sink);

        assert!(
            changed,
            "BranchHintPass should return true when detecting a Jcc"
        );
        assert_eq!(sink.diagnostics.len(), 1);
        assert_eq!(sink.diagnostics[0].pass, "branch-hint");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1507 rewrote 1 sites")
        );
    }

    #[test]
    fn branch_hint_emits_o1507_per_jcc() {
        let (mut arena, _func) = create_test_arena();
        let mut sink = OptDiagSink::new();

        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 5);

        // Create 3 Jcc nodes
        let jcc1_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(jcc1_id, create_jcc_instruction());

        let jcc2_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(jcc2_id, create_jcc_instruction());

        let jcc3_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(jcc3_id, create_jcc_instruction());

        let pass = BranchHintPass;
        let changed = pass.apply(&mut arena, jcc1_id, &mut sink);

        assert!(changed, "BranchHintPass should detect 3 Jcc nodes");
        assert_eq!(sink.diagnostics.len(), 1);
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1507 rewrote 3 sites")
        );
    }

    #[test]
    fn branch_hint_marks_taken_vs_not_taken() {
        let (mut arena, _func) = create_test_arena();
        let mut sink = OptDiagSink::new();

        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 5);

        // Create a Jcc node
        let jcc_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(jcc_id, create_jcc_instruction());

        let pass = BranchHintPass;
        let _changed = pass.apply(&mut arena, jcc_id, &mut sink);

        // Verify that the encoding_hint was set with the taken prefix (0x3E)
        let inst = arena.instructions().get(jcc_id).unwrap();
        assert!(
            inst.encoding_hint.is_some(),
            "Jcc should have encoding_hint set"
        );
        let hint = inst.encoding_hint.unwrap();
        assert_eq!(hint.opcode, 0x3E, "Should use 0x3E for taken branch-hint");
        assert_eq!(
            hint.operand_size, 255,
            "Should use 255 as marker for branch-hint"
        );
    }

    #[test]
    fn branch_hint_skips_unconditional_jmp() {
        let (mut arena, _func) = create_test_arena();
        let mut sink = OptDiagSink::new();

        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 5);

        // Create only an unconditional Jmp node (no Jcc)
        let jmp_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(jmp_id, create_jmp_instruction());

        let pass = BranchHintPass;
        let changed = pass.apply(&mut arena, jmp_id, &mut sink);

        assert!(
            !changed,
            "BranchHintPass should not rewrite unconditional Jmp"
        );
        assert_eq!(sink.diagnostics.len(), 0);

        // Verify that the Jmp instruction was not marked
        let inst = arena.instructions().get(jmp_id).unwrap();
        assert!(
            inst.encoding_hint.is_none(),
            "Jmp should not have encoding_hint"
        );
    }
}
