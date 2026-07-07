//! Side-table for `Cast` IR nodes recording the target type.
//!
//! Each `IrKind::Cast` node carries the source expression as its sole child in
//! the arena's `children_table`. This module provides a side-table
//! (`CastSideTable`) mapping `Cast` node ids to the [`TypeId`] of the cast
//! target, so the emit pass can choose the correct width-conversion
//! instruction (`movsx` / `movzx` / `mov`) without re-deriving the type.
//!
//! Phase 7 m4-002.

use crate::impl_named_side_table;
use crate::monomorphisation::TypeId;
use crate::node::IrNodeId;

impl_named_side_table!(
    /// Side-table mapping `Cast` IrNodeIds to their target [`TypeId`].
    ///
    /// Populated by the lowerer as `Cast` nodes are constructed (the AST
    /// records the target type as a `Type*` node; the lowerer resolves it to
    /// a `TypeId`). The emit pass reads entries to determine the destination
    /// width / signedness.
    pub struct CastSideTable, IrNodeId => TypeId
);

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
        assert_eq!(table.get(id), Some(&target));
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
        assert_eq!(table.get(id), Some(&TypeId::from_index(2)));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn cast_side_table_handles_distinct_casts() {
        let mut table = CastSideTable::new();
        table.insert(node(1), TypeId::from_index(10));
        table.insert(node(2), TypeId::from_index(20));
        assert_eq!(table.get(node(1)), Some(&TypeId::from_index(10)));
        assert_eq!(table.get(node(2)), Some(&TypeId::from_index(20)));
        assert_eq!(table.len(), 2);
    }
}
