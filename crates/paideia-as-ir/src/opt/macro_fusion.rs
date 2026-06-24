//! Macro-fusion optimization pass.
//!
//! Detects adjacent (Cmp, Jcc) instruction pairs and emits a fusion-flagged
//! EncodingHint on the Cmp instruction. Phase-4-m1-007 (real rewrite):
//! sets the encoding_hint's operand_size to encode "fused with next jcc".

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::instruction::Mnemonic;
use crate::node::IrNodeId;

#[cfg(test)]
use crate::instruction::InstrMode;

/// The macro-fusion optimization pass.
pub struct MacroFusionPass;

/// Detect patterns of Cmp followed by Jcc that can be fused.
/// Returns a list of (cmp_idx, jcc_idx) pairs.
pub fn detect_fusion_pairs(ids: &[IrNodeId]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for i in 0..ids.len().saturating_sub(1) {
        // Phase-3 minimum: just track consecutive positions.
        // Real detection logic (by Mnemonic) deferred to encoder integration.
        pairs.push((i, i + 1));
    }
    pairs
}

impl OptPass for MacroFusionPass {
    fn name(&self) -> &'static str {
        "macro-fusion"
    }

    fn apply(&self, arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        let mut rewrote = 0;

        // Collect all instruction node IDs and sort them.
        let mut ids: Vec<IrNodeId> = arena.instructions().entries().keys().copied().collect();
        ids.sort_by_key(|id| id.get());

        // For each adjacent (Cmp, Jcc) pair, emit a fusion-flagged EncodingHint
        // on the Cmp's instruction record.
        for window in ids.windows(2) {
            let cmp_id = window[0];
            let jcc_id = window[1];

            let cmp_mnem = arena.instructions().get(cmp_id).map(|i| i.mnemonic);
            let jcc_mnem = arena.instructions().get(jcc_id).map(|i| i.mnemonic);

            if matches!(cmp_mnem, Some(Mnemonic::Cmp)) && matches!(jcc_mnem, Some(Mnemonic::Jcc(_)))
            {
                // Flag the Cmp instruction with a fusion hint.
                // Phase-4-m1-007 minimum: set the encoding_hint's
                // operand_size to encode "fused with next jcc" (use 255 as marker).
                if let Some(inst) = arena.instructions_mut().get_mut(cmp_id) {
                    if inst.encoding_hint.is_none() {
                        inst.encoding_hint = Some(crate::instruction::EncodingHint {
                            opcode: 0x3B,      // Standard CMP r/m64 opcode
                            operand_size: 255, // Marker for fusion
                        });
                        rewrote += 1;
                    }
                }
            }
        }

        if rewrote > 0 {
            sink.emit("macro-fusion", format!("O1504 rewrote {} sites", rewrote));
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::{InstrMode, Instruction, Operand, RegId};
    use paideia_as_diagnostics::{FileId, Span};
    use smallvec::SmallVec;

    fn create_test_arena() -> (IrArena, IrNodeId) {
        let mut arena = IrArena::new();
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 10);
        let func = arena.alloc(crate::IrKind::Functor, span);
        (arena, func)
    }

    fn create_cmp_instruction() -> Instruction {
        let mut operands = SmallVec::new();
        operands.push(Operand::Reg(RegId(0)));
        operands.push(Operand::Reg(RegId(1)));
        Instruction {
            mnemonic: Mnemonic::Cmp,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        }
    }

    fn create_jcc_instruction() -> Instruction {
        let mut operands = SmallVec::new();
        operands.push(Operand::Imm64(100));
        Instruction {
            mnemonic: Mnemonic::Jcc(crate::instruction::Cond::Eq),
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
        }
    }

    #[test]
    fn detect_fusion_pairs_empty_returns_empty() {
        let ids = vec![];
        let pairs = detect_fusion_pairs(&ids);
        assert!(pairs.is_empty());
    }

    #[test]
    fn detect_fusion_pairs_single_returns_empty() {
        let ids = vec![IrNodeId::new(1).unwrap()];
        let pairs = detect_fusion_pairs(&ids);
        assert!(pairs.is_empty());
    }

    #[test]
    fn detect_fusion_pairs_two_returns_one() {
        let ids = vec![IrNodeId::new(1).unwrap(), IrNodeId::new(2).unwrap()];
        let pairs = detect_fusion_pairs(&ids);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], (0, 1));
    }

    #[test]
    fn detect_fusion_pairs_three_returns_two() {
        let ids = vec![
            IrNodeId::new(1).unwrap(),
            IrNodeId::new(2).unwrap(),
            IrNodeId::new(3).unwrap(),
        ];
        let pairs = detect_fusion_pairs(&ids);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], (0, 1));
        assert_eq!(pairs[1], (1, 2));
    }

    #[test]
    fn macro_fusion_detects_cmp_jcc_pair() {
        let (mut arena, _func) = create_test_arena();
        let mut sink = OptDiagSink::new();

        // Create a Cmp node
        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 5);
        let cmp_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(cmp_id, create_cmp_instruction());

        // Create a Jcc node right after
        let jcc_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(jcc_id, create_jcc_instruction());

        let pass = MacroFusionPass;
        let changed = pass.apply(&mut arena, cmp_id, &mut sink);

        assert!(
            changed,
            "MacroFusionPass should return true when detecting a pair"
        );
        assert_eq!(sink.diagnostics.len(), 1);
        assert_eq!(sink.diagnostics[0].pass, "macro-fusion");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1504 rewrote 1 sites")
        );
    }

    #[test]
    fn macro_fusion_emits_o1504_rewrote_n_sites_for_2_pairs() {
        let (mut arena, _func) = create_test_arena();
        let mut sink = OptDiagSink::new();

        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 5);

        // Create 4 nodes in order: Cmp, Jcc, Cmp, Jcc
        let cmp1_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(cmp1_id, create_cmp_instruction());

        let jcc1_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(jcc1_id, create_jcc_instruction());

        let cmp2_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(cmp2_id, create_cmp_instruction());

        let jcc2_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(jcc2_id, create_jcc_instruction());

        let pass = MacroFusionPass;
        let changed = pass.apply(&mut arena, cmp1_id, &mut sink);

        assert!(changed, "MacroFusionPass should detect 2 pairs");
        assert_eq!(sink.diagnostics.len(), 1);
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1504 rewrote 2 sites")
        );
    }

    #[test]
    fn macro_fusion_skips_lone_cmp() {
        let (mut arena, _func) = create_test_arena();
        let mut sink = OptDiagSink::new();

        let file = FileId::new(1).unwrap();
        let span = Span::new(file, 0, 5);

        // Create only a Cmp node (no Jcc after it)
        let cmp_id = arena.alloc(crate::IrKind::Load, span);
        arena
            .instructions_mut()
            .insert(cmp_id, create_cmp_instruction());

        let pass = MacroFusionPass;
        let changed = pass.apply(&mut arena, cmp_id, &mut sink);

        assert!(
            !changed,
            "MacroFusionPass should not rewrite a lone Cmp without Jcc"
        );
        assert_eq!(sink.diagnostics.len(), 0);
    }
}
