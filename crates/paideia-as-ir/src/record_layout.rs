//! Side-tables for record construction and field access.
//!
//! Provides storage for type and layout information for `RecordCons` and
//! `FieldAccess` IR nodes, keeping `IrNodeData` at 48 bytes while allowing
//! rich structural metadata via sparse side-tables.
//!
//! This module follows the side-table pattern established in `load_store.rs`
//! and `instruction.rs`: each IR node variant that requires extra metadata
//! has a dedicated HashMap-based side-table for O(1) lookups.

use std::collections::HashMap;

use crate::node::IrNodeId;

/// A stable type identifier for records (would come from the type system in later phases).
/// For now, this is a simple wrapper around a u32.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct RecordTypeId(pub u32);

/// Side-table mapping RecordCons IrNodeIds to their record RecordTypeId.
///
/// `RecordCons` nodes allocate and populate records; the RecordTypeId determines
/// the record layout (field count, types, alignment).
///
/// Phase-1: populated by the IR builder as RecordCons nodes are constructed.
/// Elaborators and lowering passes read entries to determine record structure.
#[derive(Default, Debug, Clone)]
pub struct RecordLayoutTable {
    /// Sparse mapping: RecordCons node id -> RecordTypeId.
    entries: HashMap<IrNodeId, RecordTypeId>,
}

impl RecordLayoutTable {
    /// Construct an empty record layout side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the RecordTypeId for a RecordCons node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, type_id: RecordTypeId) -> Option<RecordTypeId> {
        self.entries.insert(id, type_id)
    }

    /// Look up the RecordTypeId for a RecordCons node.
    ///
    /// Returns `None` if the node was never registered.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&RecordTypeId> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the RecordTypeId for a RecordCons node.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut RecordTypeId> {
        self.entries.get_mut(&id)
    }

    /// Number of record constructors registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no record constructors are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Metadata for a field access operation.
///
/// Records the record RecordTypeId and the field index (0-based) for projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldAccessInfo {
    /// Type of the record being accessed.
    pub type_id: RecordTypeId,
    /// 0-based field index.
    pub field_index: u32,
}

/// Side-table mapping FieldAccess IrNodeIds to their metadata.
///
/// `FieldAccess` nodes project a single field from a record value;
/// this table stores the target record's TypeId and field index.
///
/// Phase-1: populated by the IR builder as FieldAccess nodes are constructed.
/// Elaborators and code generators read entries to emit field projection code.
#[derive(Default, Debug, Clone)]
pub struct FieldAccessSideTable {
    /// Sparse mapping: FieldAccess node id -> FieldAccessInfo.
    entries: HashMap<IrNodeId, FieldAccessInfo>,
}

impl FieldAccessSideTable {
    /// Construct an empty field access side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the metadata for a FieldAccess node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, info: FieldAccessInfo) -> Option<FieldAccessInfo> {
        self.entries.insert(id, info)
    }

    /// Look up the metadata for a FieldAccess node.
    ///
    /// Returns `None` if the node was never registered.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&FieldAccessInfo> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the metadata for a FieldAccess node.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut FieldAccessInfo> {
        self.entries.get_mut(&id)
    }

    /// Number of field access operations registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no field access operations are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RecordLayoutTable tests ────────────────────────────────────

    #[test]
    fn record_layout_table_insert_and_get() {
        let mut table = RecordLayoutTable::new();
        let record_cons_id = IrNodeId::new(1).unwrap();
        let type_id = RecordTypeId(42);

        // Insert and verify
        table.insert(record_cons_id, type_id);
        let retrieved = table.get(record_cons_id);
        assert!(retrieved.is_some());
        assert_eq!(*retrieved.unwrap(), type_id);
    }

    #[test]
    fn record_layout_table_get_returns_none_for_missing() {
        let table = RecordLayoutTable::new();
        let unset_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unset_id), None);
    }

    #[test]
    fn record_layout_table_len_tracks_inserts() {
        let mut table = RecordLayoutTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        for i in 1u32..=5 {
            let id = IrNodeId::new(i).unwrap();
            let type_id = RecordTypeId(i + 100);
            table.insert(id, type_id);
            assert_eq!(table.len(), i as usize);
        }

        assert!(!table.is_empty());
    }

    #[test]
    fn record_layout_table_insert_overwrites_previous() {
        let mut table = RecordLayoutTable::new();
        let record_cons_id = IrNodeId::new(1).unwrap();
        let type_id_1 = RecordTypeId(1);
        let type_id_2 = RecordTypeId(2);

        table.insert(record_cons_id, type_id_1);
        let previous = table.insert(record_cons_id, type_id_2);

        assert_eq!(previous, Some(type_id_1));
        assert_eq!(*table.get(record_cons_id).unwrap(), type_id_2);
    }

    #[test]
    fn record_layout_table_get_mut_allows_mutation() {
        let mut table = RecordLayoutTable::new();
        let record_cons_id = IrNodeId::new(1).unwrap();
        let type_id = RecordTypeId(42);

        table.insert(record_cons_id, type_id);

        if let Some(type_id_mut) = table.get_mut(record_cons_id) {
            *type_id_mut = RecordTypeId(99);
        }

        assert_eq!(*table.get(record_cons_id).unwrap(), RecordTypeId(99));
    }

    #[test]
    fn record_layout_table_empty_by_default() {
        let table = RecordLayoutTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    // ── FieldAccessSideTable tests ────────────────────────────────

    #[test]
    fn field_access_side_table_insert_and_get() {
        let mut table = FieldAccessSideTable::new();
        let field_access_id = IrNodeId::new(1).unwrap();
        let info = FieldAccessInfo {
            type_id: RecordTypeId(42),
            field_index: 2,
        };

        // Insert and verify
        table.insert(field_access_id, info);
        let retrieved = table.get(field_access_id);
        assert!(retrieved.is_some());
        assert_eq!(*retrieved.unwrap(), info);
    }

    #[test]
    fn field_access_side_table_get_returns_none_for_missing() {
        let table = FieldAccessSideTable::new();
        let unset_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unset_id), None);
    }

    #[test]
    fn field_access_side_table_len_tracks_inserts() {
        let mut table = FieldAccessSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        for i in 1u32..=5 {
            let id = IrNodeId::new(i).unwrap();
            let info = FieldAccessInfo {
                type_id: RecordTypeId(i + 100),
                field_index: i,
            };
            table.insert(id, info);
            assert_eq!(table.len(), i as usize);
        }

        assert!(!table.is_empty());
    }

    #[test]
    fn field_access_side_table_insert_overwrites_previous() {
        let mut table = FieldAccessSideTable::new();
        let field_access_id = IrNodeId::new(1).unwrap();
        let info_1 = FieldAccessInfo {
            type_id: RecordTypeId(1),
            field_index: 0,
        };
        let info_2 = FieldAccessInfo {
            type_id: RecordTypeId(2),
            field_index: 1,
        };

        table.insert(field_access_id, info_1);
        let previous = table.insert(field_access_id, info_2);

        assert_eq!(previous, Some(info_1));
        assert_eq!(*table.get(field_access_id).unwrap(), info_2);
    }

    #[test]
    fn field_access_side_table_get_mut_allows_mutation() {
        let mut table = FieldAccessSideTable::new();
        let field_access_id = IrNodeId::new(1).unwrap();
        let info = FieldAccessInfo {
            type_id: RecordTypeId(42),
            field_index: 2,
        };

        table.insert(field_access_id, info);

        if let Some(info_mut) = table.get_mut(field_access_id) {
            info_mut.field_index = 5;
        }

        let retrieved = table.get(field_access_id).unwrap();
        assert_eq!(retrieved.field_index, 5);
    }

    #[test]
    fn field_access_side_table_empty_by_default() {
        let table = FieldAccessSideTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }
}
