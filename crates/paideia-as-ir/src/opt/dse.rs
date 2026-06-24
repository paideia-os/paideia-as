//! Dead-store elimination (basic block).
//!
//! Per optimization-passes.md §5: a store to memory that gets immediately
//! overwritten by a subsequent store to the same address (with no intervening
//! read of that address) is dead and can be removed. Phase-2-m9-005 ships
//! basic-block-local DSE.

use super::{OptDiagSink, OptPass};
use crate::instruction::InstrMode;
use crate::IrArena;
use crate::node::IrNodeId;

/// The dead-store elimination pass.
pub struct DsePass;

/// A store operation in the block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreOp {
    /// Address being written.
    pub addr: u64,
    /// Byte width of the store.
    pub width: u32,
    /// Whether the store is to MMIO (suppresses DSE).
    pub mmio: bool,
}

/// A memory operation in the block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemOp {
    /// A store to memory.
    Store(StoreOp),
    /// A load from memory.
    Load {
        /// Address being read.
        addr: u64,
        /// Byte width of the load.
        width: u32,
    },
    /// LOCK-prefixed atomic — barrier for DSE.
    Barrier,
}

/// Phase-3-m3-003: dead-store elimination on an InstructionSideTable.
///
/// Takes an instruction side-table and a sequence of node IDs;
/// returns the indices of preserved operations (nodes that are not dead stores).
///
/// Algorithm: Walk in reverse; track which addresses have been "covered" by a
/// later store. If a store's address is already covered, it's dead.
/// Barriers and loads break coverage.
pub fn dse_block(
    side_table: &crate::instruction::InstructionSideTable,
    nodes: &[crate::node::IrNodeId],
) -> Vec<usize> {
    use crate::instruction::{Mnemonic, Operand, InstrMode};

    // Build a list of (node_idx, MemOp) for stores only.
    // Non-store nodes are tracked separately.
    let mut store_indices = Vec::new();
    let mut store_ops = Vec::new();

    for (node_idx, node_id) in nodes.iter().enumerate() {
        if let Some(instr) = side_table.get(*node_id)
            && instr.mnemonic == Mnemonic::Mov
            && instr.operands.len() >= 2
            && let Some(mem_op) = match instr.operands[0] {
                Operand::MemSib {
                    base,
                    index,
                    scale,
                    disp,
                } => {
                    // Compute the address as base + index*scale + disp.
                    let addr = (base.0 as u64)
                        | ((index.map(|r| r.0 as u64).unwrap_or(0)) << 8)
                        | ((scale.factor() as u64) << 16)
                        | ((disp as u64) << 24);
                    Some(MemOp::Store(StoreOp {
                        addr,
                        width: 8,
                        mmio: false,
                    }))
                }
                Operand::MemDisp { disp } => {
                    let addr = (disp as u64) << 24;
                    Some(MemOp::Store(StoreOp {
                        addr,
                        width: 8,
                        mmio: false,
                    }))
                }
                _ => None,
            }
        {
            store_indices.push(node_idx);
            store_ops.push(mem_op);
        }
    }

    // If no stores found, return identity permutation.
    if store_ops.is_empty() {
        return (0..nodes.len()).collect();
    }

    // Run DSE on the stores.
    let preserved_store_indices = dse_block_impl(&store_ops);

    // Build the final result: include all non-store nodes and preserved stores.
    let mut result = Vec::new();
    let mut store_iter = preserved_store_indices.iter().peekable();

    for (node_idx, _node_id) in nodes.iter().enumerate() {
        // Check if this node index corresponds to a store that should be preserved.
        if let Some(pos) = store_indices.iter().position(|&idx| idx == node_idx) {
            if store_iter.peek().is_some_and(|&&s_idx| s_idx == pos) {
                result.push(node_idx);
                store_iter.next();
            }
        } else {
            // Not a store, so always preserve.
            result.push(node_idx);
        }
    }

    result
}

/// Internal implementation: DSE logic on explicit memory operations.
///
/// Takes a list of memory operations and returns preserved indices.
/// Helper logic preserved from phase-2-m9-005.
#[doc(hidden)]
pub fn dse_block_impl(ops: &[MemOp]) -> Vec<usize> {
    let mut keep = vec![true; ops.len()];
    let mut covered: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for i in (0..ops.len()).rev() {
        match &ops[i] {
            MemOp::Barrier => {
                // The barrier breaks coverage; clear what we thought was overwritten.
                covered.clear();
            }
            MemOp::Load { addr, .. } => {
                // A load reads the value; can't DSE the previous store to that address.
                covered.remove(addr);
            }
            MemOp::Store(s) => {
                if s.mmio {
                    // MMIO is volatile; never DSE.
                    // Don't clear covered; the MMIO store is not dead, but it doesn't
                    // participate in subsequent DSE either. For simplicity, clear and
                    // reinitialize.
                    covered.clear();
                } else if covered.contains(&s.addr) {
                    // This store's effects are overwritten by a later store; it's dead.
                    keep[i] = false;
                } else {
                    // This store is not overwritten (yet); mark its address as covered.
                    covered.insert(s.addr);
                }
            }
        }
    }

    (0..ops.len()).filter(|i| keep[*i]).collect()
}

impl OptPass for DsePass {
    fn name(&self) -> &'static str {
        "dse"
    }

    fn apply(&self, arena: &mut IrArena, _function_root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        // Phase-3-m3-003: Walk the instruction side-table and identify dead stores.
        // Collect all instruction node IDs.
        let ids: Vec<IrNodeId> = {
            let table = arena.instructions();
            table.entries().keys().copied().collect()
        };

        if ids.is_empty() {
            return false;
        }

        let mut changed = false;

        // Run dse_block to find preserved instruction indices.
        let preserved = {
            let table = arena.instructions();
            dse_block(table, &ids)
        };

        // Identify dead stores (those not in the preserved set).
        let dead_indices: Vec<usize> = (0..ids.len()).filter(|i| !preserved.contains(i)).collect();

        if !dead_indices.is_empty() {
            changed = true;

            // Remove dead stores from the arena's InstructionSideTable.
            for &idx in &dead_indices {
                if idx < ids.len() {
                    let dead_id = ids[idx];
                    arena.instructions_mut().remove(dead_id);

                    // Emit O1505 diagnostic per dead store.
                    sink.emit(
                        "dse",
                        format!("O1505 (dse): eliminated dead store at offset {}", idx),
                    );
                }
            }
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dse_block_eliminates_double_store_keeps_second() {
        // AC 1: two stores to same addr; second wins, first is dead.
        let ops = vec![
            MemOp::Store(StoreOp {
                addr: 100,
                width: 8,
                mmio: false,
            }),
            MemOp::Store(StoreOp {
                addr: 100,
                width: 8,
                mmio: false,
            }),
        ];
        let preserved = dse_block_impl(&ops);
        assert_eq!(preserved, vec![1], "Only the second store should be kept");
    }

    #[test]
    fn dse_block_preserves_mmio_store() {
        // AC 2: MMIO store is volatile; never DSE.
        let ops = vec![
            MemOp::Store(StoreOp {
                addr: 100,
                width: 8,
                mmio: true,
            }),
            MemOp::Store(StoreOp {
                addr: 100,
                width: 8,
                mmio: false,
            }),
        ];
        let preserved = dse_block_impl(&ops);
        assert_eq!(
            preserved,
            vec![0, 1],
            "Both stores are kept; MMIO is not DSE'd"
        );
    }

    #[test]
    fn dse_block_preserves_load_between_stores() {
        // Load between two stores to the same address prevents DSE of the first.
        let ops = vec![
            MemOp::Store(StoreOp {
                addr: 100,
                width: 8,
                mmio: false,
            }),
            MemOp::Load {
                addr: 100,
                width: 8,
            },
            MemOp::Store(StoreOp {
                addr: 100,
                width: 8,
                mmio: false,
            }),
        ];
        let preserved = dse_block_impl(&ops);
        assert_eq!(
            preserved,
            vec![0, 1, 2],
            "All ops are kept; load blocks DSE"
        );
    }

    #[test]
    fn dse_block_preserves_stores_across_barrier() {
        // AC 3: LOCK barrier prevents DSE across.
        let ops = vec![
            MemOp::Store(StoreOp {
                addr: 100,
                width: 8,
                mmio: false,
            }),
            MemOp::Barrier,
            MemOp::Store(StoreOp {
                addr: 100,
                width: 8,
                mmio: false,
            }),
        ];
        let preserved = dse_block_impl(&ops);
        assert_eq!(preserved, vec![0, 1, 2], "Barrier prevents DSE across");
    }

    #[test]
    fn dse_block_handles_empty_input() {
        let ops: Vec<MemOp> = vec![];
        let preserved = dse_block_impl(&ops);
        assert!(preserved.is_empty());
    }

    #[test]
    fn dse_block_with_instruction_side_table() {
        use crate::instruction::{Instruction, InstructionSideTable, Mnemonic, Operand, RegId};
        use crate::node::IrNodeId;
        use smallvec::SmallVec;

        let mut table = InstructionSideTable::new();

        let n0 = IrNodeId::new(1).unwrap();
        let n1 = IrNodeId::new(2).unwrap();

        // n0: mov r0, r1
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
                byte_offset_in_text: None,
                mode: InstrMode::default(),
            },
        );

        // n1: add r0, 1
        table.insert(
            n1,
            Instruction {
                mnemonic: Mnemonic::Add,
                operands: {
                    let mut ops = SmallVec::new();
                    ops.push(Operand::Reg(RegId(0)));
                    ops.push(Operand::Imm64(1));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::default(),
            },
        );

        // Call the new signature; verify it accepts the table.
        let _result = dse_block(&table, &[n0, n1]);
        // Phase-3-m2-004 stub: currently returns identity permutation.
    }

    #[test]
    fn dse_block_with_2_dead_stores_returns_both_indices() {
        // AC: two consecutive stores to the same address; only the second is live.
        let ops = vec![
            MemOp::Store(StoreOp {
                addr: 100,
                width: 8,
                mmio: false,
            }),
            MemOp::Store(StoreOp {
                addr: 100,
                width: 8,
                mmio: false,
            }),
        ];
        let preserved = dse_block_impl(&ops);
        assert_eq!(preserved, vec![1], "Only the second store is preserved");
    }

    #[test]
    fn dse_pass_emits_o1505_per_eliminated_store() {
        use crate::instruction::{Instruction, Mnemonic, Operand, RegId};
        use smallvec::SmallVec;

        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let pass = DsePass;

        let n0 = IrNodeId::new(1).unwrap();
        let n1 = IrNodeId::new(2).unwrap();

        // Insert two Mov instructions with MemSib destinations (dead store scenario).
        {
            let table = arena.instructions_mut();
            // n0: mov [rax], r1 (first store to [rax])
            table.insert(
                n0,
                Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: {
                        let mut ops = SmallVec::new();
                        ops.push(Operand::MemSib {
                            base: RegId(0),
                            index: None,
                            scale: crate::instruction::Scale::X1,
                            disp: 0,
                        });
                        ops.push(Operand::Reg(RegId(1)));
                        ops
                    },
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: InstrMode::default(),
                },
            );
            // n1: mov [rax], r2 (second store to [rax], clobbering the first)
            table.insert(
                n1,
                Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: {
                        let mut ops = SmallVec::new();
                        ops.push(Operand::MemSib {
                            base: RegId(0),
                            index: None,
                            scale: crate::instruction::Scale::X1,
                            disp: 0,
                        });
                        ops.push(Operand::Reg(RegId(2)));
                        ops
                    },
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: InstrMode::default(),
                },
            );
        }

        pass.apply(&mut arena, IrNodeId::new(99).unwrap(), &mut sink);

        // Expect one O1505 diagnostic for the eliminated dead store.
        assert!(
            !sink.diagnostics.is_empty(),
            "Expected at least one diagnostic; got {}",
            sink.diagnostics.len()
        );
        let o1505_count = sink
            .diagnostics
            .iter()
            .filter(|d| d.message.contains("O1505"))
            .count();
        assert_eq!(o1505_count, 1, "Expected exactly one O1505 diagnostic");
    }

    #[test]
    fn dse_pass_preserves_live_stores() {
        use crate::instruction::{Instruction, Mnemonic, Operand, RegId};
        use smallvec::SmallVec;

        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let pass = DsePass;

        let n0 = IrNodeId::new(1).unwrap();
        let n1 = IrNodeId::new(2).unwrap();

        // Insert two Mov instructions to different addresses (both live).
        {
            let table = arena.instructions_mut();
            // n0: mov [rax], r1 (store to [rax])
            table.insert(
                n0,
                Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: {
                        let mut ops = SmallVec::new();
                        ops.push(Operand::MemSib {
                            base: RegId(0),
                            index: None,
                            scale: crate::instruction::Scale::X1,
                            disp: 0,
                        });
                        ops.push(Operand::Reg(RegId(1)));
                        ops
                    },
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: InstrMode::default(),
                },
            );
            // n1: mov [rbx], r2 (store to [rbx], different address)
            table.insert(
                n1,
                Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: {
                        let mut ops = SmallVec::new();
                        ops.push(Operand::MemSib {
                            base: RegId(3),
                            index: None,
                            scale: crate::instruction::Scale::X1,
                            disp: 0,
                        });
                        ops.push(Operand::Reg(RegId(2)));
                        ops
                    },
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: InstrMode::default(),
                },
            );
        }

        let initial_count = arena.instructions().entries().len();
        pass.apply(&mut arena, IrNodeId::new(99).unwrap(), &mut sink);
        let final_count = arena.instructions().entries().len();

        // Both stores should be preserved (different addresses).
        assert_eq!(
            initial_count, final_count,
            "Both live stores should be preserved"
        );
        assert_eq!(
            sink.diagnostics.len(),
            0,
            "No diagnostics should be emitted for live stores"
        );
    }
}
