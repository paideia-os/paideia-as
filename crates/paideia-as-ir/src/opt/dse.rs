//! Dead-store elimination (basic block).
//!
//! Per optimization-passes.md §5: a store to memory that gets immediately
//! overwritten by a subsequent store to the same address (with no intervening
//! read of that address) is dead and can be removed. Phase-2-m9-005 ships
//! basic-block-local DSE.

use super::{OptDiagSink, OptPass};
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

/// Eliminate dead stores in a basic block. Returns the indices of
/// preserved operations.
///
/// Algorithm: Walk in reverse; track which addresses have been "covered" by a
/// later store. If a store's address is already covered, it's dead.
/// Barriers and loads break coverage.
pub fn dse_block(ops: &[MemOp]) -> Vec<usize> {
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

    fn apply(
        &self,
        _arena: &mut IrArena,
        _function_root: IrNodeId,
        sink: &mut OptDiagSink,
    ) -> bool {
        // Phase-2-m9-005: walk the IR's basic blocks and identify dead stores.
        // Without per-node concrete memory operation metadata, the pass emits one
        // O1505 "would-fire" info marker. Real DSE activates when the per-node
        // memory-operation side-table lands.
        sink.emit(
            "dse",
            "O1505 (would-fire): dead-store elimination dispatched".to_string(),
        );
        false // No actual changes today.
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
        let preserved = dse_block(&ops);
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
        let preserved = dse_block(&ops);
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
        let preserved = dse_block(&ops);
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
        let preserved = dse_block(&ops);
        assert_eq!(preserved, vec![0, 1, 2], "Barrier prevents DSE across");
    }

    #[test]
    fn dse_block_handles_empty_input() {
        let ops: Vec<MemOp> = vec![];
        let preserved = dse_block(&ops);
        assert!(preserved.is_empty());
    }

    #[test]
    fn dse_pass_emits_o1505() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let pass = DsePass;

        let dummy_id = IrNodeId::new(1).unwrap();
        pass.apply(&mut arena, dummy_id, &mut sink);

        assert_eq!(sink.diagnostics.len(), 1);
        assert_eq!(sink.diagnostics[0].pass, "dse");
        assert!(sink.diagnostics[0].message.contains("O1505"));
        assert!(sink.diagnostics[0].message.contains("would-fire"));
    }
}
