//! Side-table for IR Call nodes recording the callee name + arg metadata.
//!
//! Each `IrKind::App` node carries structural children in the arena's
//! `children_table`. This module provides a side-table (`CallSideTable`)
//! mapping App node ids to their full metadata: the callee name, argument
//! count, and whether this is an intrinsic call.
//!
//! This design parallels `LoadStoreSideTable` and `HandlerSideTable`,
//! keeping `IrNodeData` at 48 bytes while allowing compact encoding of
//! call attributes.

use std::collections::HashMap;

use crate::node::IrNodeId;

/// Metadata for an App (function call) IR node.
///
/// Records the callee name (for intrinsic lookup), the argument count
/// (for arity checking during synthesis), and a flag indicating whether
/// this call targets an intrinsic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallMeta {
    /// The callee's name (for lookup in intrinsic registry).
    pub callee_name: String,
    /// Number of arguments.
    pub arg_count: u32,
    /// `true` if this call targets a registered intrinsic.
    pub is_intrinsic: bool,
}

/// Side-table mapping App IrNodeIds to their call metadata.
///
/// Parallels the arena's `children_table` pattern: uses a HashMap indexed
/// by `IrNodeId` so that lookups are O(1) and portable across systems.
///
/// Phase-1: populated by the IR builder (or elaborator) as App nodes
/// are constructed or recognized. Elaborators (phase-2+) read entries to
/// determine call properties and synthesise instruction payloads for
/// intrinsic calls.
#[derive(Default, Debug)]
pub struct CallSideTable {
    /// Sparse mapping: App node id -> CallMeta.
    /// Only App nodes have entries; other nodes don't.
    entries: HashMap<IrNodeId, CallMeta>,
}

impl CallSideTable {
    /// Construct an empty call side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the metadata for an App node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, meta: CallMeta) -> Option<CallMeta> {
        self.entries.insert(id, meta)
    }

    /// Look up the metadata for an App node.
    ///
    /// Returns `None` if the node was never registered or is not an App node.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&CallMeta> {
        self.entries.get(&id)
    }

    /// Iterate over all intrinsic call node ids.
    ///
    /// Filters entries to yield only nodes marked as intrinsic calls.
    pub fn intrinsic_call_ids(&self) -> impl Iterator<Item = IrNodeId> + '_ {
        self.entries
            .iter()
            .filter_map(|(id, m)| if m.is_intrinsic { Some(*id) } else { None })
    }

    /// Number of App nodes registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no App nodes are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Count of intrinsic calls in the table.
    #[must_use]
    pub fn intrinsic_count(&self) -> usize {
        self.entries.values().filter(|m| m.is_intrinsic).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_side_table_insert_and_get() {
        let mut table = CallSideTable::new();
        let call_id = IrNodeId::new(1).unwrap();

        let meta = CallMeta {
            callee_name: "index_u64".to_string(),
            arg_count: 2,
            is_intrinsic: true,
        };

        table.insert(call_id, meta.clone());
        let retrieved = table.get(call_id);

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().callee_name, "index_u64");
        assert_eq!(retrieved.unwrap().arg_count, 2);
        assert!(retrieved.unwrap().is_intrinsic);
    }

    #[test]
    fn call_side_table_get_returns_none_for_missing() {
        let table = CallSideTable::new();
        let unset_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unset_id), None);
    }

    #[test]
    fn call_side_table_intrinsic_call_ids_filters() {
        let mut table = CallSideTable::new();

        let intrinsic_id = IrNodeId::new(1).unwrap();
        let user_id = IrNodeId::new(2).unwrap();

        table.insert(
            intrinsic_id,
            CallMeta {
                callee_name: "index_u64".to_string(),
                arg_count: 2,
                is_intrinsic: true,
            },
        );

        table.insert(
            user_id,
            CallMeta {
                callee_name: "my_func".to_string(),
                arg_count: 1,
                is_intrinsic: false,
            },
        );

        let intrinsic_ids: Vec<_> = table.intrinsic_call_ids().collect();
        assert_eq!(intrinsic_ids.len(), 1);
        assert_eq!(intrinsic_ids[0], intrinsic_id);
    }

    #[test]
    fn call_side_table_len_tracks_inserts() {
        let mut table = CallSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        for i in 1u32..=5 {
            let id = IrNodeId::new(i).unwrap();
            let meta = CallMeta {
                callee_name: format!("func_{}", i),
                arg_count: i,
                is_intrinsic: i % 2 == 0,
            };
            table.insert(id, meta);
            assert_eq!(table.len(), i as usize);
        }

        assert!(!table.is_empty());
    }

    #[test]
    fn call_side_table_intrinsic_count() {
        let mut table = CallSideTable::new();

        for i in 1u32..=5 {
            let id = IrNodeId::new(i).unwrap();
            let meta = CallMeta {
                callee_name: format!("func_{}", i),
                arg_count: i,
                is_intrinsic: i % 2 == 0,
            };
            table.insert(id, meta);
        }

        // i=2,4 are intrinsic
        assert_eq!(table.intrinsic_count(), 2);
    }
}
