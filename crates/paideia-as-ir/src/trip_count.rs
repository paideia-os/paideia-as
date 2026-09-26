//! Trip-count side-table: maps a Loop IR node to its compile-time-known
//! iteration count, when one can be proven.
//!
//! PAS-DEBT-B3-003 (issue #1516): the `opt::unroll` pass consumes this
//! table to decide whether — and by how much — to unroll a loop.
//! Absent entry means "trip count is symbolic / unknown"; the unroll
//! pass then falls back to the pre-existing O1511 "would-fire"
//! recognition path and refuses to rewrite.
//!
//! Population is deferred to a future elaborator hook (B3-003c) that
//! folds range-literal bounds (`for i in 0..N { ... }`) and other
//! obvious constants into this table.
//!
//! Pattern follows `binding_name::BindingNameTable` and
//! `instr_owner::InstrOwnerTable`.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Sparse mapping: Loop IrNodeId → compile-time-known trip count.
#[derive(Default, Debug, Clone)]
pub struct TripCountTable {
    entries: HashMap<IrNodeId, u32>,
}

impl TripCountTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the trip count for a Loop node.
    pub fn insert(&mut self, loop_id: IrNodeId, trip: u32) {
        self.entries.insert(loop_id, trip);
    }

    /// Trip count for `loop_id`, if recorded.
    #[must_use]
    pub fn get(&self, loop_id: IrNodeId) -> Option<u32> {
        self.entries.get(&loop_id).copied()
    }

    /// True iff no entries have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Drop all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get() {
        let mut t = TripCountTable::new();
        let id = IrNodeId::new(3).unwrap();
        t.insert(id, 8);
        assert_eq!(t.get(id), Some(8));
    }

    #[test]
    fn missing_returns_none() {
        let t = TripCountTable::new();
        assert_eq!(t.get(IrNodeId::new(1).unwrap()), None);
    }

    #[test]
    fn len_and_clear() {
        let mut t = TripCountTable::new();
        assert!(t.is_empty());
        t.insert(IrNodeId::new(1).unwrap(), 4);
        t.insert(IrNodeId::new(2).unwrap(), 12);
        assert_eq!(t.len(), 2);
        t.clear();
        assert!(t.is_empty());
    }
}
