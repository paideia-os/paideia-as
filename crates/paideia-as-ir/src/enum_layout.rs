//! Side-tables for enum construction and discriminant extraction.
//!
//! Provides storage for type and variant information for `EnumCons` and
//! `EnumDiscriminant` IR nodes, keeping `IrNodeData` at 48 bytes while
//! allowing rich structural metadata via sparse side-tables.
//!
//! This module follows the side-table pattern established in `load_store.rs`
//! and `instruction.rs`: each IR node variant that requires extra metadata
//! has a dedicated HashMap-based side-table for O(1) lookups.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::node::IrNodeId;

/// A stable type identifier for enums (would come from the type system in later phases).
/// For now, this is a simple wrapper around a u32.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
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

/// Layout information for an enum type.
///
/// PA-r17-007: Captures the computed structure size, alignment, and discriminant/payload
/// information for enum types. Enums are laid out with:
/// - Discriminant (8 bytes) at offset 0
/// - Payload at offset 8 with max variant payload size
/// - Total size: 8 + max_payload_size, aligned to 8
///
/// Supports two emission forms:
/// - Register form (≤16 bytes): discriminant in RAX, payload in RDX
/// - Stack form (>16 bytes): [rsp+0] = disc, [rsp+8] = payload
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnumLayout {
    /// Total size of the enum in bytes (8 + payload_size, aligned to 8).
    pub size: u64,
    /// Alignment requirement in bytes (always 8 per AC).
    pub align: u8,
    /// Discriminant size in bytes (always 8 per AC).
    pub discriminant_size: u8,
    /// Byte offset of payload within the enum (always 8 per AC).
    pub payload_offset: u64,
    /// Maximum variant payload size in bytes.
    pub payload_size: u64,
}

impl EnumLayout {
    /// Create a new enum layout.
    ///
    /// Given the maximum variant payload size, computes:
    /// - size = 8 + payload_size
    /// - align = 8
    /// - discriminant_size = 8
    /// - payload_offset = 8
    #[must_use]
    pub fn new(payload_size: u64) -> Self {
        Self {
            size: 8 + payload_size,
            align: 8,
            discriminant_size: 8,
            payload_offset: 8,
            payload_size,
        }
    }
}

/// Side-table mapping EnumTypeId to finalised enum layouts.
///
/// PA-r17-007: Populated during emission to provide enum layout metadata
/// for downstream passes (e.g., code generation, debug info).
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct FinalisedEnumLayoutTable {
    /// Sparse mapping: EnumTypeId -> EnumLayout.
    entries: HashMap<EnumTypeId, EnumLayout>,
}

impl FinalisedEnumLayoutTable {
    /// Construct an empty finalised enum layout side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the finalised layout for an EnumTypeId.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: EnumTypeId, layout: EnumLayout) -> Option<EnumLayout> {
        self.entries.insert(id, layout)
    }

    /// Look up the finalised layout for an EnumTypeId.
    ///
    /// Returns `None` if the type was never finalised.
    #[must_use]
    pub fn get(&self, id: EnumTypeId) -> Option<&EnumLayout> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the finalised layout for an EnumTypeId.
    pub fn get_mut(&mut self, id: EnumTypeId) -> Option<&mut EnumLayout> {
        self.entries.get_mut(&id)
    }

    /// Number of enum types with finalised layouts.
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

    // ── EnumLayout tests ───────────────────────────────────────────

    #[test]
    fn enum_layout_new_zero_payload() {
        let layout = EnumLayout::new(0);
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 8);
        assert_eq!(layout.discriminant_size, 8);
        assert_eq!(layout.payload_offset, 8);
        assert_eq!(layout.payload_size, 0);
    }

    #[test]
    fn enum_layout_new_8_byte_payload() {
        let layout = EnumLayout::new(8);
        assert_eq!(layout.size, 16);
        assert_eq!(layout.align, 8);
        assert_eq!(layout.discriminant_size, 8);
        assert_eq!(layout.payload_offset, 8);
        assert_eq!(layout.payload_size, 8);
    }

    #[test]
    fn enum_layout_new_16_byte_payload() {
        let layout = EnumLayout::new(16);
        assert_eq!(layout.size, 24);
        assert_eq!(layout.align, 8);
        assert_eq!(layout.discriminant_size, 8);
        assert_eq!(layout.payload_offset, 8);
        assert_eq!(layout.payload_size, 16);
    }

    // ── FinalisedEnumLayoutTable tests ─────────────────────────────

    #[test]
    fn finalised_enum_layout_table_empty_by_default() {
        let table = FinalisedEnumLayoutTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn finalised_enum_layout_table_insert_and_get() {
        let mut table = FinalisedEnumLayoutTable::new();
        let type_id = EnumTypeId(42);
        let layout = EnumLayout::new(8);

        table.insert(type_id, layout.clone());
        let retrieved = table.get(type_id);
        assert!(retrieved.is_some());
        assert_eq!(*retrieved.unwrap(), layout);
    }

    #[test]
    fn finalised_enum_layout_table_get_returns_none_for_missing() {
        let table = FinalisedEnumLayoutTable::new();
        let missing_type = EnumTypeId(999);
        assert_eq!(table.get(missing_type), None);
    }

    #[test]
    fn finalised_enum_layout_table_insert_overwrites_previous() {
        let mut table = FinalisedEnumLayoutTable::new();
        let type_id = EnumTypeId(1);

        let layout1 = EnumLayout::new(0);
        let layout2 = EnumLayout::new(16);

        table.insert(type_id, layout1.clone());
        let previous = table.insert(type_id, layout2.clone());

        assert_eq!(previous, Some(layout1));
        assert_eq!(*table.get(type_id).unwrap(), layout2);
    }

    #[test]
    fn finalised_enum_layout_table_len_tracks_inserts() {
        let mut table = FinalisedEnumLayoutTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        for i in 0u32..5 {
            let type_id = EnumTypeId(i);
            let layout = EnumLayout::new(8);
            table.insert(type_id, layout);
            assert_eq!(table.len(), (i + 1) as usize);
        }

        assert!(!table.is_empty());
    }

    #[test]
    fn finalised_enum_layout_table_get_mut_allows_mutation() {
        let mut table = FinalisedEnumLayoutTable::new();
        let type_id = EnumTypeId(42);
        let layout = EnumLayout::new(8);

        table.insert(type_id, layout);

        if let Some(layout_mut) = table.get_mut(type_id) {
            // Verify we can mutate the layout (even though layout structure is fixed,
            // testing the accessor works).
            assert_eq!(layout_mut.payload_size, 8);
        }

        let retrieved = table.get(type_id).unwrap();
        assert_eq!(retrieved.payload_size, 8);
    }
}

/// Metadata for a match arm pattern.
///
/// Records information about a match arm including the variant being matched,
/// any payload binder, and whether this is a default/wildcard arm.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchArmMeta {
    /// 0-based variant index if matching a specific variant, None for wildcard/default.
    pub variant_index: Option<u32>,
    /// Name of payload binder if arm has pattern like `Ok(x)`, None otherwise.
    pub payload_binder: Option<String>,
    /// `true` iff this is a default/wildcard arm.
    pub is_default: bool,
}

/// Side-table mapping match arm IrNodeIds to their metadata.
///
/// Populated during elaboration as pattern bindings are resolved.
/// Consumed during emission to generate discriminant comparisons and payload loads.
#[derive(Default, Debug, Clone)]
pub struct MatchArmMetaSideTable {
    /// Sparse mapping: match arm node id -> MatchArmMeta.
    entries: HashMap<IrNodeId, MatchArmMeta>,
}

impl MatchArmMetaSideTable {
    /// Construct an empty match arm metadata side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the metadata for a match arm node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, meta: MatchArmMeta) -> Option<MatchArmMeta> {
        self.entries.insert(id, meta)
    }

    /// Look up the metadata for a match arm node.
    ///
    /// Returns `None` if the node was never registered.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&MatchArmMeta> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the metadata for a match arm node.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut MatchArmMeta> {
        self.entries.get_mut(&id)
    }

    /// Number of match arms registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no match arms are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = (&IrNodeId, &MatchArmMeta)> {
        self.entries.iter()
    }
}

/// Side-table mapping match expression IrNodeIds to their scrutinee EnumTypeId.
///
/// Records which enum type a match expression scrutinizes, enabling layout lookup
/// during code generation.
#[derive(Default, Debug, Clone)]
pub struct MatchScrutineeTable {
    /// Sparse mapping: match node id -> EnumTypeId of scrutinee.
    entries: HashMap<IrNodeId, EnumTypeId>,
}

impl MatchScrutineeTable {
    /// Construct an empty match scrutinee side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the scrutinee type for a match node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, type_id: EnumTypeId) -> Option<EnumTypeId> {
        self.entries.insert(id, type_id)
    }

    /// Look up the scrutinee type for a match node.
    ///
    /// Returns `None` if the node was never registered.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&EnumTypeId> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the scrutinee type for a match node.
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut EnumTypeId> {
        self.entries.get_mut(&id)
    }

    /// Number of match expressions registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no match expressions are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = (&IrNodeId, &EnumTypeId)> {
        self.entries.iter()
    }
}
