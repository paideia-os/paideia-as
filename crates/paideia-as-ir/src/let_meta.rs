//! Side-table for Let IR nodes recording mutability information.
//!
//! Phase 6 m5-002: Each `IrKind::Let` node carries structural children
//! in the arena's `children_table`. This module provides a side-table
//! (`LetMetaTable`) mapping Let node ids to their mutability metadata.
//!
//! This design parallels `LoadStoreSideTable` and keeps `IrNodeData` at 48 bytes
//! while allowing tracking of whether a let binding is mutable.

use std::collections::HashMap;

use crate::monomorphisation::TypeId;
use crate::node::IrNodeId;

/// Metadata for a Let IR node.
///
/// Records whether the let binding is mutable (let mut x : T = ...) and,
/// optionally, the declared type of the binding.
///
/// Phase 6 m5-002: `mutable` distinguishes rodata (immutable), data
/// (mutable initialized), and bss (mutable uninitialized) sections.
///
/// Phase 7 m4-003 (PA7C-m4-003): `ty` carries the binding's declared
/// [`TypeId`] (when known) so the emit pass can width-thread integer-literal
/// bindings — e.g. `let x : u32 = 42` emits a 5-byte `B8 imm32` move instead
/// of the generic 10-byte 64-bit move. `ty` is `None` for untyped/legacy
/// bindings, in which case the generic 64-bit path is preserved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LetInfo {
    /// true if this is `let mut x : T = ...`, false for `let x : T = ...`.
    pub mutable: bool,
    /// Declared type of the binding, if resolved. `None` for untyped bindings.
    pub ty: Option<TypeId>,
}

impl LetInfo {
    /// Construct a new LetInfo for an immutable binding (no declared type).
    #[must_use]
    pub fn immutable() -> Self {
        Self {
            mutable: false,
            ty: None,
        }
    }

    /// Construct a new LetInfo for a mutable binding (no declared type).
    #[must_use]
    pub fn mutable() -> Self {
        Self {
            mutable: true,
            ty: None,
        }
    }

    /// Construct a LetInfo with an explicit mutability and optional declared type.
    ///
    /// Phase 7 m4-003: the lowerer calls this when the binding's declared type
    /// is known, enabling width-threaded integer-literal emission.
    #[must_use]
    pub fn with_type(mutable: bool, ty: Option<TypeId>) -> Self {
        Self { mutable, ty }
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
    fn let_info_immutable_has_no_type() {
        assert_eq!(LetInfo::immutable().ty, None);
    }

    #[test]
    fn let_info_with_type_records_mutability_and_type() {
        let ty = TypeId(7);
        let info = LetInfo::with_type(true, Some(ty));
        assert!(info.mutable);
        assert_eq!(info.ty, Some(ty));

        let untyped = LetInfo::with_type(false, None);
        assert!(!untyped.mutable);
        assert_eq!(untyped.ty, None);
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
