//! Side-table for Borrow / BorrowMut IR nodes recording reference metadata.
//!
//! Each `IrKind::Borrow` and `IrKind::BorrowMut` node carries the source expression
//! as a child in the arena's `children_table`. This module provides a side-table
//! (`BorrowSideTable`) mapping Borrow/BorrowMut node ids to their reference metadata:
//! source binding id, lifetime id (0 = 'static; nonzero = m5 region id), and
//! mutability flag.
//!
//! This design parallels `LoadStoreSideTable` and keeps `IrNodeData` at 48 bytes
//! while allowing compact encoding of borrow attributes.
//!
//! Real wiring with the borrow checker activates in phase-4-m6.

use std::collections::HashMap;

use crate::node::IrNodeId;

/// Metadata for a Borrow or BorrowMut IR node.
///
/// Records the source binding id (resolver-internal identifier),
/// lifetime id (0 = 'static; nonzero = m5 region id), and
/// mutability flag (false for Borrow, true for BorrowMut).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BorrowMeta {
    /// Source binding id (resolver-internal).
    pub source_binding: u32,
    /// Lifetime id: 0 = 'static; nonzero = m5 region id.
    pub lifetime_id: u32,
    /// Mutability: false for immutable borrow, true for mutable borrow.
    pub mutable: bool,
}

/// Side-table mapping Borrow/BorrowMut IrNodeIds to their metadata.
///
/// Parallels the arena's `children_table` pattern: uses a HashMap indexed
/// by `IrNodeId` so that lookups are O(1) and portable across systems.
///
/// Phase-1: populated by the IR builder (or lowerer) as Borrow/BorrowMut nodes
/// are constructed. Phase-4-m6 (borrow checker) reads entries to determine
/// reference properties and validate borrow semantics.
#[derive(Default, Debug, Clone)]
pub struct BorrowSideTable {
    /// Sparse mapping: Borrow/BorrowMut node id -> BorrowMeta.
    /// Only Borrow and BorrowMut nodes have entries; other nodes don't.
    entries: HashMap<IrNodeId, BorrowMeta>,
}

impl BorrowSideTable {
    /// Construct an empty borrow side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the metadata for a Borrow/BorrowMut node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, meta: BorrowMeta) -> Option<BorrowMeta> {
        self.entries.insert(id, meta)
    }

    /// Look up the metadata for a Borrow/BorrowMut node.
    ///
    /// Returns `None` if the node was never registered or is not a Borrow/BorrowMut node.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&BorrowMeta> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the metadata for a Borrow/BorrowMut node.
    ///
    /// Allows elaborators to mutate the metadata (if needed in future phases)
    /// without cloning.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut BorrowMeta> {
        self.entries.get_mut(&id)
    }

    /// Number of borrow operations registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no borrow operations are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── BorrowMeta tests ────────────────────────────────────────────

    #[test]
    fn borrow_meta_immutable_and_mutable() {
        let imm = BorrowMeta {
            source_binding: 1,
            lifetime_id: 0,
            mutable: false,
        };
        let mut_b = BorrowMeta {
            source_binding: 1,
            lifetime_id: 0,
            mutable: true,
        };

        assert!(!imm.mutable);
        assert!(mut_b.mutable);
    }

    #[test]
    fn borrow_meta_carries_lifetime_id() {
        let static_borrow = BorrowMeta {
            source_binding: 42,
            lifetime_id: 0,
            mutable: false,
        };
        let region_borrow = BorrowMeta {
            source_binding: 42,
            lifetime_id: 5,
            mutable: false,
        };

        assert_eq!(static_borrow.lifetime_id, 0); // 'static
        assert_eq!(region_borrow.lifetime_id, 5); // region id 5 (m5)
    }

    // ── BorrowSideTable tests ───────────────────────────────────────

    #[test]
    fn borrow_side_table_insert_and_get() {
        let mut table = BorrowSideTable::new();
        let borrow_id = IrNodeId::new(1).unwrap();

        let meta = BorrowMeta {
            source_binding: 10,
            lifetime_id: 0,
            mutable: false,
        };

        // Insert and verify
        table.insert(borrow_id, meta);
        let retrieved = table.get(borrow_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().source_binding, 10);
        assert_eq!(retrieved.unwrap().lifetime_id, 0);
        assert!(!retrieved.unwrap().mutable);
    }

    #[test]
    fn borrow_side_table_get_returns_none_for_missing() {
        let table = BorrowSideTable::new();
        let unset_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unset_id), None);
    }

    #[test]
    fn ir_kind_borrow_borrow_mut_deref_present() {
        // Smoke test: verify the IR kinds exist and can be compared.
        use crate::node::IrKind;

        let borrow = IrKind::Borrow;
        let borrow_mut = IrKind::BorrowMut;
        let deref = IrKind::Deref;

        // Each should be distinct.
        assert_ne!(borrow, borrow_mut);
        assert_ne!(borrow, deref);
        assert_ne!(borrow_mut, deref);
    }

    #[test]
    fn borrow_meta_mutability_distinguished() {
        let imm_meta = BorrowMeta {
            source_binding: 1,
            lifetime_id: 0,
            mutable: false,
        };
        let mut_meta = BorrowMeta {
            source_binding: 1,
            lifetime_id: 0,
            mutable: true,
        };

        // These should be distinct despite identical source_binding and lifetime_id.
        assert_ne!(imm_meta, mut_meta);
        assert!(!imm_meta.mutable);
        assert!(mut_meta.mutable);
    }

    #[test]
    fn borrow_side_table_handles_distinct_borrows() {
        let mut table = BorrowSideTable::new();

        let borrow_id_1 = IrNodeId::new(1).unwrap();
        let borrow_id_2 = IrNodeId::new(2).unwrap();
        let borrow_id_3 = IrNodeId::new(3).unwrap();

        let meta_1 = BorrowMeta {
            source_binding: 10,
            lifetime_id: 0,
            mutable: false,
        };
        let meta_2 = BorrowMeta {
            source_binding: 20,
            lifetime_id: 5,
            mutable: true,
        };
        let meta_3 = BorrowMeta {
            source_binding: 30,
            lifetime_id: 0,
            mutable: false,
        };

        table.insert(borrow_id_1, meta_1);
        table.insert(borrow_id_2, meta_2);
        table.insert(borrow_id_3, meta_3);

        // Verify all three are stored and retrievable.
        assert_eq!(table.len(), 3);
        assert_eq!(table.get(borrow_id_1).unwrap().source_binding, 10);
        assert_eq!(table.get(borrow_id_2).unwrap().source_binding, 20);
        assert_eq!(table.get(borrow_id_3).unwrap().source_binding, 30);

        // Verify mutability is preserved.
        assert!(!table.get(borrow_id_1).unwrap().mutable);
        assert!(table.get(borrow_id_2).unwrap().mutable);
        assert!(!table.get(borrow_id_3).unwrap().mutable);
    }

    #[test]
    fn borrow_side_table_empty_by_default() {
        let table = BorrowSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn borrow_side_table_get_mut_allows_mutation() {
        let mut table = BorrowSideTable::new();
        let borrow_id = IrNodeId::new(1).unwrap();

        let meta = BorrowMeta {
            source_binding: 10,
            lifetime_id: 0,
            mutable: false,
        };
        table.insert(borrow_id, meta);

        // Mutate via get_mut
        if let Some(m) = table.get_mut(borrow_id) {
            m.mutable = true;
        }

        // Verify mutation took effect
        let retrieved = table.get(borrow_id).unwrap();
        assert!(retrieved.mutable);
    }
}
