//! Side-tables for enum construction and discriminant extraction.
//!
//! Provides storage for type and variant information for `EnumCons` and
//! `EnumDiscriminant` IR nodes, keeping `IrNodeData` at 48 bytes while
//! allowing rich structural metadata via sparse side-tables.
//!
//! This module follows the side-table pattern established in `load_store.rs`
//! and `instruction.rs`: each IR node variant that requires extra metadata
//! has a dedicated HashMap-based side-table for O(1) lookups.

use std::collections::HashMap;

use crate::node::IrNodeId;

/// A stable type identifier for enums (would come from the type system in later phases).
/// For now, this is a simple wrapper around a u32.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct EnumTypeId(pub u32);

/// Metadata for an enum construction operation.
///
/// Records the enum EnumTypeId and the variant index (0-based) for the constructed variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnumConsInfo {
    /// Type of the enum being constructed.
    pub type_id: EnumTypeId,
    /// 0-based variant index.
    pub variant_index: u32,
}

/// Side-table mapping EnumCons IrNodeIds to their metadata.
///
/// `EnumCons` nodes construct an enum variant with payload arguments;
/// this table stores the enum TypeId and variant index.
///
/// Phase-1: populated by the IR builder as EnumCons nodes are constructed.
/// Elaborators and code generators read entries to emit variant construction code.
#[derive(Default, Debug, Clone)]
pub struct EnumConsSideTable {
    /// Sparse mapping: EnumCons node id -> EnumConsInfo.
    entries: HashMap<IrNodeId, EnumConsInfo>,
}

impl EnumConsSideTable {
    /// Construct an empty enum cons side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the metadata for an EnumCons node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, info: EnumConsInfo) -> Option<EnumConsInfo> {
        self.entries.insert(id, info)
    }

    /// Look up the metadata for an EnumCons node.
    ///
    /// Returns `None` if the node was never registered.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&EnumConsInfo> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the metadata for an EnumCons node.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut EnumConsInfo> {
        self.entries.get_mut(&id)
    }

    /// Number of enum constructors registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no enum constructors are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Side-table mapping EnumDiscriminant IrNodeIds to their enum EnumTypeId.
///
/// `EnumDiscriminant` nodes extract the tag/discriminant from an enum value;
/// the EnumTypeId determines the discriminant representation and interpretation.
///
/// Phase-1: populated by the IR builder as EnumDiscriminant nodes are constructed.
/// Elaborators and code generators read entries to emit discriminant extraction code.
#[derive(Default, Debug, Clone)]
pub struct EnumDiscriminantSideTable {
    /// Sparse mapping: EnumDiscriminant node id -> EnumTypeId.
    entries: HashMap<IrNodeId, EnumTypeId>,
}

impl EnumDiscriminantSideTable {
    /// Construct an empty enum discriminant side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the EnumTypeId for an EnumDiscriminant node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, type_id: EnumTypeId) -> Option<EnumTypeId> {
        self.entries.insert(id, type_id)
    }

    /// Look up the EnumTypeId for an EnumDiscriminant node.
    ///
    /// Returns `None` if the node was never registered.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&EnumTypeId> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the EnumTypeId for an EnumDiscriminant node.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut EnumTypeId> {
        self.entries.get_mut(&id)
    }

    /// Number of enum discriminant operations registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no enum discriminant operations are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── EnumConsSideTable tests ────────────────────────────────────

    #[test]
    fn enum_cons_side_table_insert_and_get() {
        let mut table = EnumConsSideTable::new();
        let enum_cons_id = IrNodeId::new(1).unwrap();
        let info = EnumConsInfo {
            type_id: EnumTypeId(42),
            variant_index: 1,
        };

        // Insert and verify
        table.insert(enum_cons_id, info);
        let retrieved = table.get(enum_cons_id);
        assert!(retrieved.is_some());
        assert_eq!(*retrieved.unwrap(), info);
    }

    #[test]
    fn enum_cons_side_table_records_variant_index() {
        let mut table = EnumConsSideTable::new();
        let enum_cons_id = IrNodeId::new(5).unwrap();
        let info = EnumConsInfo {
            type_id: EnumTypeId(100),
            variant_index: 3,
        };

        table.insert(enum_cons_id, info);
        let retrieved = table.get(enum_cons_id).unwrap();

        assert_eq!(retrieved.type_id, EnumTypeId(100));
        assert_eq!(retrieved.variant_index, 3);
    }

    #[test]
    fn enum_cons_side_table_get_returns_none_for_missing() {
        let table = EnumConsSideTable::new();
        let unset_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unset_id), None);
    }

    #[test]
    fn enum_cons_side_table_len_tracks_inserts() {
        let mut table = EnumConsSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        for i in 1u32..=5 {
            let id = IrNodeId::new(i).unwrap();
            let info = EnumConsInfo {
                type_id: EnumTypeId(i + 100),
                variant_index: i,
            };
            table.insert(id, info);
            assert_eq!(table.len(), i as usize);
        }

        assert!(!table.is_empty());
    }

    #[test]
    fn enum_cons_side_table_insert_overwrites_previous() {
        let mut table = EnumConsSideTable::new();
        let enum_cons_id = IrNodeId::new(1).unwrap();
        let info_1 = EnumConsInfo {
            type_id: EnumTypeId(1),
            variant_index: 0,
        };
        let info_2 = EnumConsInfo {
            type_id: EnumTypeId(2),
            variant_index: 1,
        };

        table.insert(enum_cons_id, info_1);
        let previous = table.insert(enum_cons_id, info_2);

        assert_eq!(previous, Some(info_1));
        assert_eq!(*table.get(enum_cons_id).unwrap(), info_2);
    }

    #[test]
    fn enum_cons_side_table_get_mut_allows_mutation() {
        let mut table = EnumConsSideTable::new();
        let enum_cons_id = IrNodeId::new(1).unwrap();
        let info = EnumConsInfo {
            type_id: EnumTypeId(42),
            variant_index: 1,
        };

        table.insert(enum_cons_id, info);

        if let Some(info_mut) = table.get_mut(enum_cons_id) {
            info_mut.variant_index = 5;
        }

        let retrieved = table.get(enum_cons_id).unwrap();
        assert_eq!(retrieved.variant_index, 5);
    }

    #[test]
    fn enum_cons_side_table_empty_by_default() {
        let table = EnumConsSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    // ── EnumDiscriminantSideTable tests ────────────────────────────

    #[test]
    fn enum_discriminant_side_table_insert_and_get() {
        let mut table = EnumDiscriminantSideTable::new();
        let enum_discriminant_id = IrNodeId::new(1).unwrap();
        let type_id = EnumTypeId(42);

        // Insert and verify
        table.insert(enum_discriminant_id, type_id);
        let retrieved = table.get(enum_discriminant_id);
        assert!(retrieved.is_some());
        assert_eq!(*retrieved.unwrap(), type_id);
    }

    #[test]
    fn enum_discriminant_side_table_records_type() {
        let mut table = EnumDiscriminantSideTable::new();
        let enum_discriminant_id = IrNodeId::new(7).unwrap();
        let type_id = EnumTypeId(55);

        table.insert(enum_discriminant_id, type_id);
        let retrieved = table.get(enum_discriminant_id).unwrap();

        assert_eq!(*retrieved, EnumTypeId(55));
    }

    #[test]
    fn enum_discriminant_side_table_get_returns_none_for_missing() {
        let table = EnumDiscriminantSideTable::new();
        let unset_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unset_id), None);
    }

    #[test]
    fn enum_discriminant_side_table_len_tracks_inserts() {
        let mut table = EnumDiscriminantSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        for i in 1u32..=5 {
            let id = IrNodeId::new(i).unwrap();
            let type_id = EnumTypeId(i + 100);
            table.insert(id, type_id);
            assert_eq!(table.len(), i as usize);
        }

        assert!(!table.is_empty());
    }

    #[test]
    fn enum_discriminant_side_table_insert_overwrites_previous() {
        let mut table = EnumDiscriminantSideTable::new();
        let enum_discriminant_id = IrNodeId::new(1).unwrap();
        let type_id_1 = EnumTypeId(1);
        let type_id_2 = EnumTypeId(2);

        table.insert(enum_discriminant_id, type_id_1);
        let previous = table.insert(enum_discriminant_id, type_id_2);

        assert_eq!(previous, Some(type_id_1));
        assert_eq!(*table.get(enum_discriminant_id).unwrap(), type_id_2);
    }

    #[test]
    fn enum_discriminant_side_table_get_mut_allows_mutation() {
        let mut table = EnumDiscriminantSideTable::new();
        let enum_discriminant_id = IrNodeId::new(1).unwrap();
        let type_id = EnumTypeId(42);

        table.insert(enum_discriminant_id, type_id);

        if let Some(type_id_mut) = table.get_mut(enum_discriminant_id) {
            *type_id_mut = EnumTypeId(99);
        }

        assert_eq!(*table.get(enum_discriminant_id).unwrap(), EnumTypeId(99));
    }

    #[test]
    fn enum_discriminant_side_table_empty_by_default() {
        let table = EnumDiscriminantSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }
}
