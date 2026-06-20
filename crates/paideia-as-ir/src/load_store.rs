//! Side-table for Load / Store IR nodes recording width / signedness /
//! alignment without expanding IrNodeData past 48 bytes.
//!
//! Each `IrKind::Load` and `IrKind::Store` node carries structural children
//! in the arena's `children_table`. This module provides a side-table
//! (`LoadStoreSideTable`) mapping Load/Store node ids to their full metadata:
//! width (1, 2, 4, or 8 bytes), signedness (signed or unsigned), and alignment.
//!
//! This design parallels `HandlerSideTable` and keeps `IrNodeData` at 48 bytes
//! while allowing compact encoding of load/store attributes.

use std::collections::HashMap;

use crate::{
    IrArena,
    node::{IrKind, IrNodeId},
};
use paideia_as_diagnostics::Span;

/// Memory access width in bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Width {
    /// 1 byte
    Byte,
    /// 2 bytes
    Half,
    /// 4 bytes
    Word,
    /// 8 bytes
    Quad,
}

impl Width {
    /// Convert width to byte count.
    #[must_use]
    pub fn bytes(self) -> u32 {
        match self {
            Width::Byte => 1,
            Width::Half => 2,
            Width::Word => 4,
            Width::Quad => 8,
        }
    }

    /// Construct a Width from a byte count.
    /// Returns `None` if the byte count is not a canonical width (1, 2, 4, or 8).
    #[must_use]
    pub fn from_bytes(b: u32) -> Option<Self> {
        match b {
            1 => Some(Width::Byte),
            2 => Some(Width::Half),
            4 => Some(Width::Word),
            8 => Some(Width::Quad),
            _ => None,
        }
    }
}

/// Signedness of a load / store.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Signedness {
    /// Unsigned integer / float.
    Unsigned,
    /// Signed integer.
    Signed,
}

/// Metadata for a Load or Store IR node.
///
/// Records the memory access width (1, 2, 4, or 8 bytes), signedness
/// (for integer loads, determines sign-extension behavior), and alignment
/// (typically equal to width, but may be stricter for performance).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadStoreInfo {
    /// Memory access width.
    pub width: Width,
    /// Signedness (determines sign-extension for loads).
    pub signedness: Signedness,
    /// Alignment in bytes; typically equal to width.
    pub alignment: u32,
}

/// Side-table mapping Load/Store IrNodeIds to their metadata.
///
/// Parallels the arena's `children_table` pattern: uses a HashMap indexed
/// by `IrNodeId` so that lookups are O(1) and portable across systems.
///
/// Phase-1: populated by the IR builder (or lowerer) as Load/Store nodes
/// are constructed. Elaborators (phase-2+) read entries to determine
/// memory access properties.
#[derive(Default, Debug, Clone)]
pub struct LoadStoreSideTable {
    /// Sparse mapping: Load/Store node id -> LoadStoreInfo.
    /// Only Load and Store nodes have entries; other nodes don't.
    entries: HashMap<IrNodeId, LoadStoreInfo>,
}

impl LoadStoreSideTable {
    /// Construct an empty load/store side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the metadata for a Load/Store node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, info: LoadStoreInfo) -> Option<LoadStoreInfo> {
        self.entries.insert(id, info)
    }

    /// Look up the metadata for a Load/Store node.
    ///
    /// Returns `None` if the node was never registered or is not a Load/Store node.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&LoadStoreInfo> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the metadata for a Load/Store node.
    ///
    /// Allows elaborators to mutate the metadata (if needed in future phases)
    /// without cloning.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut LoadStoreInfo> {
        self.entries.get_mut(&id)
    }

    /// Number of load/store operations registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no load/store operations are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Allocate a Load node with side-table entry.
///
/// Creates an `IrKind::Load` node with children `[pointer, index]` and
/// inserts a `LoadStoreInfo` entry into the side-table.
///
/// # Arguments
///
/// * `arena` - The IR arena for node allocation.
/// * `table` - The load/store side-table to update.
/// * `pointer` - IrNodeId of the pointer operand.
/// * `index` - IrNodeId of the index/offset operand.
/// * `info` - The LoadStoreInfo describing width, signedness, alignment.
///
/// # Returns
///
/// The freshly-allocated `IrNodeId` for the Load node.
pub fn alloc_load(
    arena: &mut IrArena,
    table: &mut LoadStoreSideTable,
    pointer: IrNodeId,
    index: IrNodeId,
    info: LoadStoreInfo,
    span: Span,
) -> IrNodeId {
    let load_id = arena.alloc_with_children(IrKind::Load, span, [pointer, index]);
    table.insert(load_id, info);
    load_id
}

/// Allocate a Store node with side-table entry.
///
/// Creates an `IrKind::Store` node with children `[pointer, index, value]` and
/// inserts a `LoadStoreInfo` entry into the side-table.
///
/// # Arguments
///
/// * `arena` - The IR arena for node allocation.
/// * `table` - The load/store side-table to update.
/// * `pointer` - IrNodeId of the pointer operand.
/// * `index` - IrNodeId of the index/offset operand.
/// * `value` - IrNodeId of the value operand.
/// * `info` - The LoadStoreInfo describing width, signedness, alignment.
///
/// # Returns
///
/// The freshly-allocated `IrNodeId` for the Store node.
pub fn alloc_store(
    arena: &mut IrArena,
    table: &mut LoadStoreSideTable,
    pointer: IrNodeId,
    index: IrNodeId,
    value: IrNodeId,
    info: LoadStoreInfo,
    span: Span,
) -> IrNodeId {
    let store_id = arena.alloc_with_children(IrKind::Store, span, [pointer, index, value]);
    table.insert(store_id, info);
    store_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    // ── Width tests ─────────────────────────────────────────────────

    #[test]
    fn width_bytes_returns_expected_powers() {
        assert_eq!(Width::Byte.bytes(), 1);
        assert_eq!(Width::Half.bytes(), 2);
        assert_eq!(Width::Word.bytes(), 4);
        assert_eq!(Width::Quad.bytes(), 8);
    }

    #[test]
    fn width_from_bytes_handles_canonical_widths() {
        assert_eq!(Width::from_bytes(1), Some(Width::Byte));
        assert_eq!(Width::from_bytes(2), Some(Width::Half));
        assert_eq!(Width::from_bytes(4), Some(Width::Word));
        assert_eq!(Width::from_bytes(8), Some(Width::Quad));
    }

    #[test]
    fn width_from_bytes_returns_none_for_invalid() {
        assert_eq!(Width::from_bytes(0), None);
        assert_eq!(Width::from_bytes(3), None);
        assert_eq!(Width::from_bytes(5), None);
        assert_eq!(Width::from_bytes(6), None);
        assert_eq!(Width::from_bytes(7), None);
        assert_eq!(Width::from_bytes(16), None);
    }

    // ── LoadStoreSideTable tests ────────────────────────────────────

    #[test]
    fn load_store_side_table_insert_and_get() {
        let mut table = LoadStoreSideTable::new();
        let load_id = IrNodeId::new(1).unwrap();

        let info = LoadStoreInfo {
            width: Width::Word,
            signedness: Signedness::Unsigned,
            alignment: 4,
        };

        // Insert and verify
        table.insert(load_id, info);
        let retrieved = table.get(load_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().width, Width::Word);
        assert_eq!(retrieved.unwrap().signedness, Signedness::Unsigned);
        assert_eq!(retrieved.unwrap().alignment, 4);
    }

    #[test]
    fn load_store_side_table_get_returns_none_for_missing() {
        let table = LoadStoreSideTable::new();
        let unset_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unset_id), None);
    }

    #[test]
    fn load_store_side_table_len_tracks_inserts() {
        let mut table = LoadStoreSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        for i in 1u32..=5 {
            let id = IrNodeId::new(i).unwrap();
            let info = LoadStoreInfo {
                width: Width::Byte,
                signedness: Signedness::Signed,
                alignment: 1,
            };
            table.insert(id, info);
            assert_eq!(table.len(), i as usize);
        }

        assert!(!table.is_empty());
    }

    // ── alloc_load / alloc_store tests ──────────────────────────────

    #[test]
    fn alloc_load_creates_node_and_side_table_entry() {
        let mut arena = IrArena::new();
        let mut table = LoadStoreSideTable::new();

        let ptr_id = arena.alloc(IrKind::Var, span());
        let idx_id = arena.alloc(IrKind::Literal, span());

        let info = LoadStoreInfo {
            width: Width::Quad,
            signedness: Signedness::Unsigned,
            alignment: 8,
        };

        let load_id = alloc_load(&mut arena, &mut table, ptr_id, idx_id, info, span());

        // Verify the node was created
        assert_eq!(arena[load_id].kind, IrKind::Load);

        // Verify the children are correct
        let children = arena.children(load_id);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0], ptr_id);
        assert_eq!(children[1], idx_id);

        // Verify the side-table entry
        let entry = table.get(load_id).unwrap();
        assert_eq!(entry.width, Width::Quad);
        assert_eq!(entry.signedness, Signedness::Unsigned);
        assert_eq!(entry.alignment, 8);
    }

    #[test]
    fn alloc_store_creates_node_with_three_children() {
        let mut arena = IrArena::new();
        let mut table = LoadStoreSideTable::new();

        let ptr_id = arena.alloc(IrKind::Var, span());
        let idx_id = arena.alloc(IrKind::Literal, span());
        let val_id = arena.alloc(IrKind::Literal, span());

        let info = LoadStoreInfo {
            width: Width::Half,
            signedness: Signedness::Signed,
            alignment: 2,
        };

        let store_id = alloc_store(&mut arena, &mut table, ptr_id, idx_id, val_id, info, span());

        // Verify the node was created
        assert_eq!(arena[store_id].kind, IrKind::Store);

        // Verify the children are correct (must be [pointer, index, value])
        let children = arena.children(store_id);
        assert_eq!(children.len(), 3);
        assert_eq!(children[0], ptr_id);
        assert_eq!(children[1], idx_id);
        assert_eq!(children[2], val_id);

        // Verify the side-table entry
        let entry = table.get(store_id).unwrap();
        assert_eq!(entry.width, Width::Half);
        assert_eq!(entry.signedness, Signedness::Signed);
        assert_eq!(entry.alignment, 2);
    }

    #[test]
    fn load_store_side_table_get_mut_allows_mutation() {
        let mut table = LoadStoreSideTable::new();
        let load_id = IrNodeId::new(1).unwrap();

        let info = LoadStoreInfo {
            width: Width::Byte,
            signedness: Signedness::Unsigned,
            alignment: 1,
        };
        table.insert(load_id, info);

        // Mutate via get_mut
        if let Some(info_mut) = table.get_mut(load_id) {
            info_mut.signedness = Signedness::Signed;
        }

        // Verify mutation took effect
        let retrieved = table.get(load_id).unwrap();
        assert_eq!(retrieved.signedness, Signedness::Signed);
    }

    #[test]
    fn load_store_side_table_empty_by_default() {
        let table = LoadStoreSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }
}
