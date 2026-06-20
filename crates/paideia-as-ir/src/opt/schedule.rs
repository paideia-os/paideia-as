//! Instruction scheduling pass.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::node::IrNodeId;

/// Instruction scheduling pass for hiding latency within basic blocks.
pub struct InstructionSchedulingPass;

/// Latency model for individual instruction classes.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum InstructionClass {
    /// Register-to-register move, ALU op (latency = 1).
    AluReg,
    /// Load from L1 cache (latency = 4).
    LoadL1,
    /// Load from L2/L3/RAM (latency = 12-100; treat as 12 for the scheduler).
    LoadHigher,
    /// Store (latency = 1 to issue; commit latency hidden).
    Store,
    /// Branch (latency = 1 predicted, ~15 misprediction).
    Branch,
    /// LOCK-prefixed atomic operation — memory barrier; no reorder across.
    AtomicLocked,
    /// Other (treat as 3-cycle conservative default).
    Other,
}

impl InstructionClass {
    /// Returns the latency in cycles for this instruction class.
    pub fn latency(self) -> u32 {
        match self {
            Self::AluReg => 1,
            Self::LoadL1 => 4,
            Self::LoadHigher => 12,
            Self::Store => 1,
            Self::Branch => 1,
            Self::AtomicLocked => 1, // serialised; not the reorder target
            Self::Other => 3,
        }
    }

    /// Whether this instruction acts as a barrier for reordering.
    pub fn is_barrier(self) -> bool {
        matches!(self, Self::AtomicLocked | Self::Branch)
    }
}

impl OptPass for InstructionSchedulingPass {
    fn name(&self) -> &'static str {
        "schedule"
    }

    fn apply(
        &self,
        _arena: &mut IrArena,
        _function_root: IrNodeId,
        sink: &mut OptDiagSink,
    ) -> bool {
        // Phase-2-m9-003: walk the IR's basic blocks and identify
        // reorderable sequences. Without per-node x86_64 mnemonics
        // (m1-002 kind-only), the pass emits one O1503 "would-fire"
        // info marker. Real reordering activates when the per-node
        // instruction-class side-table lands.
        sink.emit(
            "schedule",
            "O1503 (would-fire): instruction scheduling pass dispatched".to_string(),
        );
        false
    }
}

/// Phase-3-m2-004: instruction scheduling helper operating on an
/// InstructionSideTable with a sequence of node IDs.
///
/// Takes an instruction side-table and an ordered list of IrNodeIds;
/// returns a reordered list of indices into the input sequence. Reordering rules:
/// 1. Loads can move EARLIER (toward the start) to hide latency.
/// 2. Instructions can move past non-barrier independent ones.
/// 3. Reordering stops at any barrier (AtomicLocked, Branch).
///
/// Returns a permutation of indices [0..len(nodes)).
pub fn schedule_block(
    _side_table: &crate::instruction::InstructionSideTable,
    nodes: &[crate::node::IrNodeId],
) -> Vec<usize> {
    // Phase-3-m2-004: For now, construct synthetic instruction list
    // from mnemonic data in the side-table. Future PRs may optimize
    // this by walking the side-table directly.
    //
    // TODO(phase-3-m3): Extract InstructionClass from Mnemonic and operands,
    // then apply the scheduling heuristic below.

    // Placeholder: no reordering yet (just return original order).
    (0..nodes.len()).collect()
}

/// Internal implementation: scheduling heuristic on explicit instruction classes.
///
/// Takes a list of (instruction_index, InstructionClass) tuples;
/// returns a reordered list. Helper logic preserved from phase-2-m9-003.
#[doc(hidden)]
pub fn schedule_block_impl(instructions: &[(usize, InstructionClass)]) -> Vec<usize> {
    let mut result: Vec<usize> = Vec::with_capacity(instructions.len());
    let mut i = 0;
    while i < instructions.len() {
        let (idx, class) = instructions[i];
        if class.is_barrier() {
            // Flush everything from i upward as-is.
            for j in i..instructions.len() {
                result.push(instructions[j].0);
                if instructions[j].1.is_barrier() {
                    i = j + 1;
                    break;
                }
                if j + 1 == instructions.len() {
                    i = j + 1;
                    break;
                }
            }
            continue;
        }
        // For non-barrier instructions, simple heuristic: if the next is a load
        // with higher latency, swap them.
        if i + 1 < instructions.len() {
            let next = instructions[i + 1];
            if !next.1.is_barrier() && next.1.latency() > class.latency() {
                // Move the higher-latency instruction earlier.
                result.push(next.0);
                result.push(idx);
                i += 2;
                continue;
            }
        }
        result.push(idx);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_class_latency_values_match_documented_ranges() {
        assert_eq!(InstructionClass::AluReg.latency(), 1);
        assert_eq!(InstructionClass::LoadL1.latency(), 4);
        assert_eq!(InstructionClass::LoadHigher.latency(), 12);
        assert_eq!(InstructionClass::Store.latency(), 1);
        assert_eq!(InstructionClass::Branch.latency(), 1);
        assert_eq!(InstructionClass::AtomicLocked.latency(), 1);
        assert_eq!(InstructionClass::Other.latency(), 3);
    }

    #[test]
    fn is_barrier_returns_true_for_atomic_and_branch() {
        assert!(InstructionClass::AtomicLocked.is_barrier());
        assert!(InstructionClass::Branch.is_barrier());
        assert!(!InstructionClass::AluReg.is_barrier());
        assert!(!InstructionClass::LoadL1.is_barrier());
        assert!(!InstructionClass::LoadHigher.is_barrier());
        assert!(!InstructionClass::Store.is_barrier());
        assert!(!InstructionClass::Other.is_barrier());
    }

    #[test]
    fn schedule_block_hoists_higher_latency_load_earlier() {
        // AC 1: input [(0, AluReg), (1, LoadHigher)]; output [1, 0]
        let input = vec![
            (0, InstructionClass::AluReg),
            (1, InstructionClass::LoadHigher),
        ];
        let output = schedule_block_impl(&input);
        assert_eq!(output, vec![1, 0]);
    }

    #[test]
    fn schedule_block_respects_lock_barrier() {
        // AC 2: input [(0, AluReg), (1, AtomicLocked), (2, LoadHigher)];
        // output preserves order around the barrier.
        let input = vec![
            (0, InstructionClass::AluReg),
            (1, InstructionClass::AtomicLocked),
            (2, InstructionClass::LoadHigher),
        ];
        let output = schedule_block_impl(&input);
        // The barrier at position 1 should prevent reordering across it.
        // We should get 0, 1, 2 in order.
        assert_eq!(output, vec![0, 1, 2]);
    }

    #[test]
    fn schedule_block_with_instruction_side_table() {
        use crate::instruction::{Instruction, InstructionSideTable, Mnemonic, Operand, RegId};
        use crate::node::IrNodeId;
        use smallvec::SmallVec;

        // Create a synthetic instruction side-table with latency-bearing instructions.
        let mut table = InstructionSideTable::new();

        let n0 = IrNodeId::new(1).unwrap();
        let n1 = IrNodeId::new(2).unwrap();

        // n0: mov r0, r1 (AluReg latency)
        table.insert(
            n0,
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(0)));
                    ops.push(Operand::Reg(RegId(1)));
                    ops
                },
                encoding_hint: None,
            },
        );

        // n1: lea r2, [rax + rbx*4 + 8] (LoadL1 latency)
        table.insert(
            n1,
            Instruction {
                mnemonic: Mnemonic::Lea,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(2)));
                    ops.push(Operand::MemSib {
                        base: RegId(0),
                        index: Some(RegId(3)),
                        scale: crate::instruction::Scale::X4,
                        disp: 8,
                    });
                    ops
                },
                encoding_hint: None,
            },
        );

        // Call the new signature; verify it accepts the table.
        let _result = schedule_block(&table, &[n0, n1]);
        // Phase-3-m2-004 stub: currently returns identity permutation.
        // Real reordering logic activates in phase-3-m3.
    }

    #[test]
    fn schedule_pass_emits_o1503() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let pass = InstructionSchedulingPass;

        let dummy_id = IrNodeId::new(1).unwrap();
        pass.apply(&mut arena, dummy_id, &mut sink);

        assert_eq!(sink.diagnostics.len(), 1);
        assert_eq!(sink.diagnostics[0].pass, "schedule");
        assert!(sink.diagnostics[0].message.contains("O1503"));
        assert!(sink.diagnostics[0].message.contains("would-fire"));
    }
}
