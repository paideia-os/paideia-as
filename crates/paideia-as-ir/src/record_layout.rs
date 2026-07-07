//! Side-tables for record construction and field access.
//!
//! Provides storage for type and layout information for `RecordCons` and
//! `FieldAccess` IR nodes, keeping `IrNodeData` at 48 bytes while allowing
//! rich structural metadata via sparse side-tables.
//!
//! This module follows the side-table pattern established in `load_store.rs`
//! and `instruction.rs`: each IR node variant that requires extra metadata
//! has a dedicated HashMap-based side-table for O(1) lookups.
//!
//! Phase 6 m3-001 adds `FinalisedLayoutTable` for C-ABI natural-alignment
//! record layout computation during emission.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::node::IrNodeId;

/// A stable type identifier for records (would come from the type system in later phases).
/// For now, this is a simple wrapper around a u32.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct RecordTypeId(pub u32);

/// Layout information for a single field within a record.
///
/// Phase 6 m3-001: Records the byte offset and size of a field.
/// `offset` is the byte offset from the start of the record (aligned per field type).
/// `size` is the field size in bytes: 1 (u8), 2 (u16/i16), 4 (u32/i32), 8 (u64/i64/*T).
/// `signed` indicates whether the field is signed (for sign-extend loads).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FieldLayout {
    /// Byte offset within the record (aligned per field's natural alignment).
    pub offset: u64,
    /// Field size in bytes (1, 2, 4, or 8 in Phase 6/13).
    pub size: u8,
    /// Whether the field is signed (affects load operation: zero-extend vs sign-extend).
    #[serde(default)]
    pub signed: bool,
}

/// Complete layout for a record type.
///
/// Phase 6 m3-001: Captures the computed structure size, alignment, and per-field
/// layout information using C ABI natural-alignment rules.
///
/// Phase 17 m9-009 (nested patterns): Adds field names for pattern binding lookups.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecordLayout {
    /// Total size of the record in bytes (rounded up to struct alignment).
    pub size: u64,
    /// Alignment requirement in bytes (max of all field alignments).
    pub align: u8,
    /// Per-field layout (offset, size) in declaration order.
    pub fields: Vec<FieldLayout>,
    /// Field names in declaration order, parallel to `fields`.
    /// Used for nested pattern bindings to map names to field indices.
    /// Added in Phase 17; defaults to empty for backward compatibility.
    #[serde(default)]
    pub field_names: Vec<String>,
}

impl RecordLayout {
    /// Create a new record layout.
    #[must_use]
    pub fn new(size: u64, align: u8, fields: Vec<FieldLayout>) -> Self {
        Self {
            size,
            align,
            fields,
            field_names: Vec::new(),
        }
    }

    /// Create a new record layout with field names.
    ///
    /// The field_names vec should be parallel to the fields vec.
    #[must_use]
    pub fn with_field_names(
        size: u64,
        align: u8,
        fields: Vec<FieldLayout>,
        field_names: Vec<String>,
    ) -> Self {
        Self {
            size,
            align,
            fields,
            field_names,
        }
    }

    /// Look up the index of a field by name.
    ///
    /// Returns None if the field name is not found.
    #[must_use]
    pub fn field_index_by_name(&self, name: &str) -> Option<usize> {
        self.field_names.iter().position(|n| n == name)
    }
}

/// Side-table mapping RecordTypeId to finalised C-ABI natural-alignment layouts.
///
/// Phase 6 m3-001: Populated during emission to provide record layout metadata
/// for downstream passes (e.g., code generation, debug info).
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct FinalisedLayoutTable {
    /// Sparse mapping: RecordTypeId -> RecordLayout.
    entries: HashMap<RecordTypeId, RecordLayout>,
}

impl FinalisedLayoutTable {
    /// Construct an empty finalised layout side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the finalised layout for a RecordTypeId.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: RecordTypeId, layout: RecordLayout) -> Option<RecordLayout> {
        self.entries.insert(id, layout)
    }

    /// Look up the finalised layout for a RecordTypeId.
    ///
    /// Returns `None` if the type was never finalised.
    #[must_use]
    pub fn get(&self, id: RecordTypeId) -> Option<&RecordLayout> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the finalised layout for a RecordTypeId.
    pub fn get_mut(&mut self, id: RecordTypeId) -> Option<&mut RecordLayout> {
        self.entries.get_mut(&id)
    }

    /// Number of record types with finalised layouts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no layouts are finalised.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

crate::impl_named_side_table!(
    /// Side-table mapping RecordCons IrNodeIds to their record RecordTypeId.
    ///
    /// `RecordCons` nodes allocate and populate records; the
    /// RecordTypeId determines the record layout (field count, types,
    /// alignment).
    ///
    /// Phase-1: populated by the IR builder as RecordCons nodes are
    /// constructed. Elaborators and lowering passes read entries to
    /// determine record structure.
    pub struct RecordLayoutTable, IrNodeId => RecordTypeId
);

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

crate::impl_named_side_table!(
    /// Side-table mapping FieldAccess IrNodeIds to their metadata.
    ///
    /// `FieldAccess` nodes project a single field from a record value;
    /// this table stores the target record's TypeId and field index.
    ///
    /// Phase-1: populated by the IR builder as FieldAccess nodes are
    /// constructed. Elaborators and code generators read entries to emit
    /// field projection code.
    pub struct FieldAccessSideTable, IrNodeId => FieldAccessInfo
);

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

    // ── FinalisedLayoutTable tests ────────────────────────────────

    #[test]
    fn finalised_layout_table_empty_by_default() {
        let table = FinalisedLayoutTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn finalised_layout_table_insert_and_get() {
        let mut table = FinalisedLayoutTable::new();
        let type_id = RecordTypeId(42);
        let layout = RecordLayout::new(
            32,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false },
                FieldLayout { offset: 8, size: 8, signed: false },
            ],
        );

        table.insert(type_id, layout.clone());
        let retrieved = table.get(type_id);
        assert!(retrieved.is_some());
        assert_eq!(*retrieved.unwrap(), layout);
    }

    #[test]
    fn finalised_layout_table_get_returns_none_for_missing() {
        let table = FinalisedLayoutTable::new();
        let missing_type = RecordTypeId(999);
        assert_eq!(table.get(missing_type), None);
    }

    #[test]
    fn finalised_layout_table_insert_overwrites_previous() {
        let mut table = FinalisedLayoutTable::new();
        let type_id = RecordTypeId(1);

        let layout1 = RecordLayout::new(16, 8, vec![FieldLayout { offset: 0, size: 8, signed: false }]);
        let layout2 = RecordLayout::new(
            32,
            8,
            vec![
                FieldLayout { offset: 0, size: 8, signed: false },
                FieldLayout { offset: 8, size: 8, signed: false },
            ],
        );

        table.insert(type_id, layout1.clone());
        let previous = table.insert(type_id, layout2.clone());

        assert_eq!(previous, Some(layout1));
        assert_eq!(*table.get(type_id).unwrap(), layout2);
    }

    #[test]
    fn finalised_layout_table_len_tracks_inserts() {
        let mut table = FinalisedLayoutTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        for i in 0u32..5 {
            let type_id = RecordTypeId(i);
            let layout = RecordLayout::new(8, 8, vec![FieldLayout { offset: 0, size: 8, signed: false }]);
            table.insert(type_id, layout);
            assert_eq!(table.len(), (i + 1) as usize);
        }

        assert!(!table.is_empty());
    }

    #[test]
    fn field_layout_capability_struct() {
        // Capability: 4 × u64 → offsets [0, 8, 16, 24], size 32, align 8.
        let fields = vec![
            FieldLayout { offset: 0, size: 8, signed: false },
            FieldLayout { offset: 8, size: 8, signed: false },
            FieldLayout {
                offset: 16,
                size: 8,
                signed: false,
            },
            FieldLayout {
                offset: 24,
                size: 8,
                signed: false,
            },
        ];

        let cap_layout = RecordLayout::new(32, 8, fields.clone());
        assert_eq!(cap_layout.size, 32);
        assert_eq!(cap_layout.align, 8);
        assert_eq!(cap_layout.fields.len(), 4);
        assert_eq!(cap_layout.fields[0].offset, 0);
        assert_eq!(cap_layout.fields[1].offset, 8);
        assert_eq!(cap_layout.fields[2].offset, 16);
        assert_eq!(cap_layout.fields[3].offset, 24);
    }
}
