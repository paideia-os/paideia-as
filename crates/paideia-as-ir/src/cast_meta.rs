//! Side-table for `Cast` IR nodes recording the target type.
//!
//! Each `IrKind::Cast` node carries the source expression as its sole child in
//! the arena's `children_table`. This module provides a side-table
//! (`CastSideTable`) mapping `Cast` node ids to the [`TypeId`] of the cast
//! target, so the emit pass can choose the correct width-conversion
//! instruction (`movsx` / `movzx` / `mov`) without re-deriving the type.
//!
//! This design parallels `BorrowSideTable` / `LoadStoreSideTable` and keeps
//! `IrNodeData` at its byte budget while allowing compact encoding of cast
//! attributes.
//!
//! Phase 7 m4-002.

use std::collections::HashMap;

use crate::monomorphisation::TypeId;
use crate::node::IrNodeId;

/// Side-table mapping `Cast` IrNodeIds to their target [`TypeId`].
///
/// Parallels the arena's `children_table` pattern: uses a HashMap indexed
/// by `IrNodeId` so that lookups are O(1) and portable across systems.
///
/// Populated by the lowerer as `Cast` nodes are constructed (the AST records
/// the target type as a `Type*` node; the lowerer resolves it to a `TypeId`).
/// The emit pass reads entries to determine the destination width / signedness.
#[derive(Default, Debug, Clone)]
pub struct CastSideTable {
    /// Sparse mapping: Cast node id -> target TypeId.
    /// Only `Cast` nodes have entries; other nodes don't.
    entries: HashMap<IrNodeId, TypeId>,
}

impl CastSideTable {
    /// Construct an empty cast side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the target type for a `Cast` node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, target: TypeId) -> Option<TypeId> {
        self.entries.insert(id, target)
    }

    /// Look up the target type for a `Cast` node.
    ///
    /// Returns `None` if the node was never registered or is not a `Cast` node.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<TypeId> {
        self.entries.get(&id).copied()
    }

    /// Number of cast operations registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no cast operations are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(n: u32) -> IrNodeId {
        IrNodeId::new(n).unwrap()
    }

    #[test]
    fn cast_side_table_empty_by_default() {
        let table = CastSideTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn cast_side_table_insert_and_get() {
        let mut table = CastSideTable::new();
        let id = node(7);
        let target = TypeId::from_index(3);

        assert!(table.insert(id, target).is_none());
        assert_eq!(table.get(id), Some(target));
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }

    #[test]
    fn cast_side_table_get_returns_none_for_missing() {
        let table = CastSideTable::new();
        assert_eq!(table.get(node(42)), None);
    }

    #[test]
    fn cast_side_table_insert_overwrites() {
        let mut table = CastSideTable::new();
        let id = node(1);
        assert!(table.insert(id, TypeId::from_index(1)).is_none());
        let prev = table.insert(id, TypeId::from_index(2));
        assert_eq!(prev, Some(TypeId::from_index(1)));
        assert_eq!(table.get(id), Some(TypeId::from_index(2)));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn cast_side_table_handles_distinct_casts() {
        let mut table = CastSideTable::new();
        table.insert(node(1), TypeId::from_index(10));
        table.insert(node(2), TypeId::from_index(20));
        assert_eq!(table.get(node(1)), Some(TypeId::from_index(10)));
        assert_eq!(table.get(node(2)), Some(TypeId::from_index(20)));
        assert_eq!(table.len(), 2);
    }
}
