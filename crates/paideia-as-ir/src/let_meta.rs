//! Side-table for Let IR nodes recording mutability information.
//!
//! Phase 6 m5-002: Each `IrKind::Let` node carries structural children
//! in the arena's `children_table`. This module provides a side-table
//! (`LetMetaTable`) mapping Let node ids to their mutability metadata.
//!
//! This design parallels `LoadStoreSideTable` and keeps `IrNodeData` at 48 bytes
//! while allowing tracking of whether a let binding is mutable.

use std::collections::HashMap;

use crate::node::IrNodeId;

/// Metadata for a Let IR node.
///
/// Records whether the let binding is mutable (let mut x : T = ...).
/// Phase 6 m5-002: Used to distinguish between rodata (immutable),
/// data (mutable initialized), and bss (mutable uninitialized) sections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LetInfo {
    /// true if this is `let mut x : T = ...`, false for `let x : T = ...`.
    pub mutable: bool,
}

impl LetInfo {
    /// Construct a new LetInfo for an immutable binding.
    #[must_use]
    pub fn immutable() -> Self {
        Self { mutable: false }
    }

    /// Construct a new LetInfo for a mutable binding.
    #[must_use]
    pub fn mutable() -> Self {
        Self { mutable: true }
    }
}

/// Side-table mapping Let IR node IDs → LetInfo.
///
/// Sparse mapping: let node id -> LetInfo.
#[derive(Default, Debug, Clone)]
pub struct LetMetaTable {
    entries: HashMap<IrNodeId, LetInfo>,
}

impl LetMetaTable {
    /// Construct an empty LetMetaTable.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) a let metadata entry.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, info: LetInfo) -> Option<LetInfo> {
        self.entries.insert(id, info)
    }

    /// Look up let metadata.
    ///
    /// Returns `None` if the node was never registered or is not mutable.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&LetInfo> {
        self.entries.get(&id)
    }

    /// Look up let metadata (mutable).
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut LetInfo> {
        self.entries.get_mut(&id)
    }

    /// Number of let metadata entries registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no let metadata entries are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove a let metadata entry.
    ///
    /// Returns the entry if one existed.
    pub fn remove(&mut self, id: IrNodeId) -> Option<LetInfo> {
        self.entries.remove(&id)
    }

    /// Iterate over all entries (id, info) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&IrNodeId, &LetInfo)> {
        self.entries.iter()
    }

    /// Borrow the underlying HashMap (read-only).
    #[must_use]
    pub fn entries(&self) -> &HashMap<IrNodeId, LetInfo> {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn let_info_immutable_constructs() {
        let info = LetInfo::immutable();
        assert!(!info.mutable);
    }

    #[test]
    fn let_info_mutable_constructs() {
        let info = LetInfo::mutable();
        assert!(info.mutable);
    }

    #[test]
    fn let_meta_table_insert_and_get() {
        let mut table = LetMetaTable::new();
        let let_id = IrNodeId::new(1).unwrap();
        let info = LetInfo::mutable();

        table.insert(let_id, info);
        let retrieved = table.get(let_id).unwrap();
        assert!(retrieved.mutable);
    }

    #[test]
    fn let_meta_table_get_returns_none_for_unknown() {
        let table = LetMetaTable::new();
        let unknown_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unknown_id), None);
    }

    #[test]
    fn let_meta_table_len_and_is_empty() {
        let mut table = LetMetaTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);

        let id1 = IrNodeId::new(1).unwrap();
        table.insert(id1, LetInfo::mutable());
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }

    #[test]
    fn let_meta_table_remove() {
        let mut table = LetMetaTable::new();
        let let_id = IrNodeId::new(1).unwrap();
        let info = LetInfo::mutable();

        table.insert(let_id, info);
        assert_eq!(table.len(), 1);

        let removed = table.remove(let_id).unwrap();
        assert!(removed.mutable);
        assert_eq!(table.len(), 0);
    }
}
