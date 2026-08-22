//! Side-table: item-level Let binding → `AtomicOrdering`.
//!
//! paideia-as#1301 (v0.21-003c, phase-2). Module-level `pub let mut`
//! bindings may carry an `@atomic(Ordering)` trailing symbol attribute:
//!
//! ```text
//! pub let mut counter : u64 = 0 @atomic(SeqCst);
//! ```
//!
//! Rather than growing every construction/destructuring site of
//! [`crate::ItemData::Let`] (dozens of files touched, mostly by adding
//! `atomic: None,`), we keep the ordering on this sparse side-table
//! keyed by the `Let` node's [`NodeId`]. The parser inserts into it
//! immediately after allocating the ItemData::Let. The elaborator's
//! `populate_let_meta` pass reads from it to stamp the ordering into
//! the IR-level `LetInfo::atomic`, at which point the emit walker's
//! load/store sites gate their fence emission on it.
//!
//! Design parallels [`crate::PatternTypeHints`] — both are sparse
//! per-node side-tables on the AST arena.

use std::collections::HashMap;

use crate::items::AtomicOrdering;
use crate::node_id::NodeId;

/// Maps Let-item `NodeId` → `AtomicOrdering`.
///
/// Sparse: only Let bindings that carry `@atomic(Ordering)` at parse
/// time have an entry. Absence is the common case.
#[derive(Debug, Default)]
pub struct ItemAtomicTable {
    entries: HashMap<NodeId, AtomicOrdering>,
}

impl ItemAtomicTable {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Insert a Let-item → ordering mapping (overwrites any prior entry).
    pub fn insert(&mut self, let_id: NodeId, ordering: AtomicOrdering) {
        self.entries.insert(let_id, ordering);
    }

    /// Look up the ordering discipline for a Let item.
    #[must_use]
    pub fn get(&self, let_id: NodeId) -> Option<AtomicOrdering> {
        self.entries.get(&let_id).copied()
    }

    /// Number of entries in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over (Let-id, ordering) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, AtomicOrdering)> + '_ {
        self.entries.iter().map(|(k, v)| (*k, *v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let t = ItemAtomicTable::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn insert_and_get() {
        let mut t = ItemAtomicTable::new();
        let id = NodeId::new(5).unwrap();
        t.insert(id, AtomicOrdering::SeqCst);
        assert_eq!(t.get(id), Some(AtomicOrdering::SeqCst));
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn get_absent_returns_none() {
        let t = ItemAtomicTable::new();
        assert!(t.get(NodeId::new(1).unwrap()).is_none());
    }

    #[test]
    fn insert_overwrites() {
        let mut t = ItemAtomicTable::new();
        let id = NodeId::new(3).unwrap();
        t.insert(id, AtomicOrdering::Relaxed);
        t.insert(id, AtomicOrdering::Acquire);
        assert_eq!(t.get(id), Some(AtomicOrdering::Acquire));
        assert_eq!(t.len(), 1);
    }
}
