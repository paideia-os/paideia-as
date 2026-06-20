//! Side-table mapping IR Loop nodes to entry/exit label ids.
//!
//! When the encoder processes a Loop node, it may need to reference
//! entry and exit labels for control flow. This table stores those
//! label ids indexed by the Loop node's IrNodeId.

use std::collections::HashMap;

use crate::node::IrNodeId;

/// Metadata for a single Loop IR node.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct LoopMeta {
    /// Label id for loop entry (used by encoder to mark loop beginning).
    pub entry_label: u32,
    /// Label id for loop exit (target of Break; also used for fallthrough).
    pub exit_label: u32,
}

/// Side-table mapping Loop node IDs to their metadata.
///
/// Used during IR lowering and encoding to associate control-flow labels
/// with Loop nodes. Break and Continue instructions reference these labels
/// to implement non-local jumps.
#[derive(Clone, Debug)]
pub struct LoopMetaTable {
    entries: HashMap<IrNodeId, LoopMeta>,
}

impl LoopMetaTable {
    /// Construct a new empty LoopMetaTable.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert a LoopMeta entry for a given Loop node.
    ///
    /// If a node already exists, its entry is overwritten.
    pub fn insert(&mut self, node_id: IrNodeId, meta: LoopMeta) {
        self.entries.insert(node_id, meta);
    }

    /// Retrieve the LoopMeta for a Loop node, if it exists.
    #[must_use]
    pub fn get(&self, node_id: IrNodeId) -> Option<LoopMeta> {
        self.entries.get(&node_id).copied()
    }

    /// Return the number of entries in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if this table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for LoopMetaTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_meta_table_insert_and_get() {
        let mut table = LoopMetaTable::new();
        let loop_id = IrNodeId::new(1).unwrap();
        let meta = LoopMeta {
            entry_label: 10,
            exit_label: 20,
        };

        table.insert(loop_id, meta);
        assert_eq!(table.get(loop_id), Some(meta));
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }

    #[test]
    fn loop_meta_table_handles_nested() {
        let mut table = LoopMetaTable::new();

        let loop_id_1 = IrNodeId::new(1).unwrap();
        let meta_1 = LoopMeta {
            entry_label: 10,
            exit_label: 20,
        };

        let loop_id_2 = IrNodeId::new(2).unwrap();
        let meta_2 = LoopMeta {
            entry_label: 30,
            exit_label: 40,
        };

        table.insert(loop_id_1, meta_1);
        table.insert(loop_id_2, meta_2);

        assert_eq!(table.get(loop_id_1), Some(meta_1));
        assert_eq!(table.get(loop_id_2), Some(meta_2));
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn loop_meta_handles_distinct_loops() {
        let mut table = LoopMetaTable::new();

        let loop_id_outer = IrNodeId::new(5).unwrap();
        let meta_outer = LoopMeta {
            entry_label: 100,
            exit_label: 200,
        };

        let loop_id_inner = IrNodeId::new(6).unwrap();
        let meta_inner = LoopMeta {
            entry_label: 110,
            exit_label: 210,
        };

        table.insert(loop_id_outer, meta_outer);
        table.insert(loop_id_inner, meta_inner);

        assert_eq!(table.get(loop_id_outer), Some(meta_outer));
        assert_eq!(table.get(loop_id_inner), Some(meta_inner));
        assert_ne!(meta_outer, meta_inner);
    }

    #[test]
    fn ir_kind_loop_break_continue_present() {
        use crate::IrKind;

        // Verify that Loop, Break, Continue variants exist on IrKind.
        let _loop_kind = IrKind::Loop;
        let _break_kind = IrKind::Break;
        let _continue_kind = IrKind::Continue;

        // Verify they compare correctly.
        assert_eq!(IrKind::Loop, IrKind::Loop);
        assert_eq!(IrKind::Break, IrKind::Break);
        assert_eq!(IrKind::Continue, IrKind::Continue);
        assert_ne!(IrKind::Loop, IrKind::Break);
        assert_ne!(IrKind::Break, IrKind::Continue);
    }
}
