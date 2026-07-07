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

crate::impl_named_side_table!(
    /// Side-table mapping EnumCons IrNodeIds to their metadata.
    ///
    /// `EnumCons` nodes construct an enum variant with payload arguments;
    /// this table stores the enum TypeId and variant index.
    ///
    /// Phase-1: populated by the IR builder as EnumCons nodes are
    /// constructed. Elaborators and code generators read entries to emit
    /// variant construction code.
    pub struct EnumConsSideTable, IrNodeId => EnumConsInfo
);

crate::impl_named_side_table!(
    /// Side-table mapping EnumDiscriminant IrNodeIds to their enum
    /// EnumTypeId.
    ///
    /// `EnumDiscriminant` nodes extract the tag/discriminant from an enum
    /// value; the EnumTypeId determines the discriminant representation
    /// and interpretation.
    ///
    /// Phase-1: populated by the IR builder as EnumDiscriminant nodes are
    /// constructed. Elaborators and code generators read entries to emit
    /// discriminant extraction code.
    pub struct EnumDiscriminantSideTable, IrNodeId => EnumTypeId
);

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

crate::impl_named_side_table!(
    @serde
    /// Side-table mapping EnumTypeId to finalised enum layouts.
    ///
    /// PA-r17-007: Populated during emission to provide enum layout
    /// metadata for downstream passes (e.g., code generation, debug info).
    /// Serializable via serde for round-tripping through the `.paideia`
    /// note section.
    pub struct FinalisedEnumLayoutTable, EnumTypeId => EnumLayout
);

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

    // ── PatternBinding tests ───────────────────────────────────────

    #[test]
    fn pattern_binding_wildcard_is_wildcard() {
        let pb = PatternBinding::Wildcard;
        assert!(pb.is_wildcard());
    }

    #[test]
    fn pattern_binding_simple_not_wildcard() {
        let pb = PatternBinding::Simple("x".to_string());
        assert!(!pb.is_wildcard());
    }

    #[test]
    fn pattern_binding_simple_preserves_name() {
        let name = "my_var".to_string();
        let pb = PatternBinding::Simple(name.clone());
        if let PatternBinding::Simple(n) = pb {
            assert_eq!(n, name);
        } else {
            panic!("Expected Simple variant");
        }
    }

    #[test]
    fn pattern_binding_enum_variant_unit_payload() {
        let pb = PatternBinding::EnumVariant {
            variant_index: 0,
            payload_type: None,
            payload: None,
        };
        assert!(!pb.is_wildcard());
        if let PatternBinding::EnumVariant { variant_index, payload_type, payload } = pb {
            assert_eq!(variant_index, 0);
            assert_eq!(payload_type, None);
            assert_eq!(payload, None);
        } else {
            panic!("Expected EnumVariant");
        }
    }

    #[test]
    fn pattern_binding_enum_variant_with_simple_payload() {
        let inner = PatternBinding::Simple("x".to_string());
        let pb = PatternBinding::EnumVariant {
            variant_index: 1,
            payload_type: None,
            payload: Some(Box::new(inner.clone())),
        };
        if let PatternBinding::EnumVariant { variant_index, payload, .. } = pb {
            assert_eq!(variant_index, 1);
            assert_eq!(*payload.unwrap(), inner);
        } else {
            panic!("Expected EnumVariant");
        }
    }

    #[test]
    fn pattern_binding_record_empty_fields() {
        use crate::record_layout::RecordTypeId;
        let pb = PatternBinding::Record {
            type_id: RecordTypeId(100),
            fields: vec![],
        };
        assert!(!pb.is_wildcard());
        if let PatternBinding::Record { type_id, fields } = pb {
            assert_eq!(type_id, RecordTypeId(100));
            assert_eq!(fields.len(), 0);
        } else {
            panic!("Expected Record");
        }
    }

    #[test]
    fn pattern_binding_record_multiple_fields() {
        use crate::record_layout::RecordTypeId;
        let pb = PatternBinding::Record {
            type_id: RecordTypeId(42),
            fields: vec![
                ("x".to_string(), PatternBinding::Simple("x_var".to_string())),
                ("y".to_string(), PatternBinding::Wildcard),
            ],
        };
        if let PatternBinding::Record { fields, .. } = pb {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0, "x");
            assert_eq!(fields[1].0, "y");
        } else {
            panic!("Expected Record");
        }
    }

    #[test]
    fn pattern_binding_clone_equality() {
        let pb1 = PatternBinding::Simple("x".to_string());
        let pb2 = pb1.clone();
        assert_eq!(pb1, pb2);
    }

    #[test]
    fn pattern_binding_enum_variant_with_record_payload() {
        use crate::record_layout::RecordTypeId;
        let record_pb = PatternBinding::Record {
            type_id: RecordTypeId(200),
            fields: vec![("field".to_string(), PatternBinding::Simple("f".to_string()))],
        };
        let pb = PatternBinding::EnumVariant {
            variant_index: 5,
            payload_type: Some(RecordTypeId(200)),
            payload: Some(Box::new(record_pb)),
        };
        assert!(!pb.is_wildcard());
    }

    // ── MatchArmMeta tests ─────────────────────────────────────────

    #[test]
    fn match_arm_meta_default_initializes_correctly() {
        let meta = MatchArmMeta::default();
        assert_eq!(meta.variant_index, None);
        assert_eq!(meta.payload_binder, None);
        assert!(!meta.is_default);
        assert_eq!(meta.pattern_binding, None);
    }

    #[test]
    fn match_arm_meta_explicit_construction() {
        let meta = MatchArmMeta {
            variant_index: Some(3),
            payload_binder: Some("payload".to_string()),
            is_default: false,
            pattern_binding: Some(PatternBinding::Wildcard),
        };
        assert_eq!(meta.variant_index, Some(3));
        assert!(meta.payload_binder.is_some());
        assert!(!meta.is_default);
        assert!(meta.pattern_binding.is_some());
    }

    // ── MatchDispatchMeta tests ────────────────────────────────────

    #[test]
    fn match_dispatch_meta_default_construction() {
        let meta = MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 5,
            covered_arms: 5,
            density_ok: true,
        };
        assert!(meta.jump_table);
        assert_eq!(meta.min_arm, 0);
        assert_eq!(meta.range, 5);
        assert_eq!(meta.covered_arms, 5);
        assert!(meta.density_ok);
    }

    #[test]
    fn match_dispatch_meta_density_ok_true() {
        // Dense case: 5 arms covering 5 values → 5*2=10 >= 5 ✓
        let meta = MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 5,
            covered_arms: 5,
            density_ok: true,
        };
        assert!(meta.density_ok);
    }

    #[test]
    fn match_dispatch_meta_density_ok_false() {
        // Sparse case: 4 arms covering 102 values → 4*2=8 < 102 ✗
        let meta = MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 102,
            covered_arms: 4,
            density_ok: false,
        };
        assert!(!meta.density_ok);
    }

    #[test]
    fn match_dispatch_meta_copy_and_equality() {
        let meta1 = MatchDispatchMeta {
            jump_table: true,
            min_arm: 10,
            range: 20,
            covered_arms: 10,
            density_ok: true,
        };
        let meta2 = meta1; // Copy trait
        assert_eq!(meta1, meta2);
    }

    // ── MatchDispatchMetaSideTable tests ───────────────────────────

    #[test]
    fn match_dispatch_meta_side_table_insert_get() {
        let mut table = MatchDispatchMetaSideTable::new();
        let node_id = IrNodeId::new(42).expect("valid node id");
        let meta = MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 5,
            covered_arms: 5,
            density_ok: true,
        };
        table.insert(node_id, meta);
        assert_eq!(table.get(node_id), Some(&meta));
    }

    #[test]
    fn match_dispatch_meta_side_table_empty() {
        let table = MatchDispatchMetaSideTable::new();
        let node_id = IrNodeId::new(99).expect("valid node id");
        assert_eq!(table.get(node_id), None);
    }

    #[test]
    fn match_dispatch_meta_side_table_len() {
        let mut table = MatchDispatchMetaSideTable::new();
        assert_eq!(table.len(), 0);
        let node_id1 = IrNodeId::new(1).expect("valid node id");
        let meta1 = MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 5,
            covered_arms: 5,
            density_ok: true,
        };
        table.insert(node_id1, meta1);
        assert_eq!(table.len(), 1);
    }
}

/// Recursive pattern-binding tree for match arms.
///
/// Represents nested patterns such as `Point { x, y }` or `Ok(Container { field: x })`.
/// Used in conjunction with the match arm metadata to generate code for extracting
/// nested fields from enum payloads and record structures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatternBinding {
    /// Wildcard pattern `_` — matches anything, no binding.
    Wildcard,
    /// Simple identifier pattern `x` — binds the value to a single register.
    Simple(String),
    /// Enum variant pattern `Variant(payload)` or `Variant { fields }`.
    EnumVariant {
        /// 0-based variant index.
        variant_index: u32,
        /// Payload type if the variant carries a record; None for non-record payloads.
        payload_type: Option<crate::record_layout::RecordTypeId>,
        /// Nested pattern for the payload; None if payload is not decomposed.
        payload: Option<Box<PatternBinding>>,
    },
    /// Record pattern `Record { field1: pat1, field2: pat2, ... }`.
    Record {
        /// Type identifier of the record.
        type_id: crate::record_layout::RecordTypeId,
        /// Field bindings: (field_name, pattern) in declaration order.
        fields: Vec<(String, PatternBinding)>,
    },
}

impl PatternBinding {
    /// Returns true if this pattern is a wildcard (no bindings).
    #[must_use]
    pub fn is_wildcard(&self) -> bool {
        matches!(self, PatternBinding::Wildcard)
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
    /// Nested pattern binding tree for complex pattern matching.
    pub pattern_binding: Option<PatternBinding>,
}

impl Default for MatchArmMeta {
    fn default() -> Self {
        Self {
            variant_index: None,
            payload_binder: None,
            is_default: false,
            pattern_binding: None,
        }
    }
}

/// Dispatch strategy metadata for match expressions with `@jump_table`.
///
/// PA-r15-009c (#1055): populated by the elaborator when it recognises
/// the `@jump_table` attribute on a match. #1032's codegen consults
/// `density_ok` to decide between the O(1) jump-table sequence and the
/// cmp/jne cascade fallback.
///
/// Density formula: `covered_arms * 2 >= range` (>= 50% coverage over
/// the min..max integer window).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatchDispatchMeta {
    /// `@jump_table` attribute present on the source match.
    pub jump_table: bool,
    /// Minimum integer-literal pattern value across non-default arms.
    pub min_arm: i64,
    /// `max_arm - min_arm + 1` — the index window size.
    pub range: u32,
    /// Count of non-default arms whose patterns are integer literals.
    pub covered_arms: u32,
    /// `covered_arms * 2 >= range`. If false, codegen falls back to
    /// cmp/jne even when `jump_table` is true.
    pub density_ok: bool,
}

crate::impl_named_side_table!(
    /// Side-table mapping match arm IrNodeIds to their metadata.
    ///
    /// Populated during elaboration as pattern bindings are resolved.
    /// Consumed during emission to generate discriminant comparisons and
    /// payload loads.
    pub struct MatchArmMetaSideTable, IrNodeId => MatchArmMeta
);

crate::impl_named_side_table!(
    /// Side-table mapping match expression IrNodeIds to their scrutinee
    /// EnumTypeId.
    ///
    /// Records which enum type a match expression scrutinizes, enabling
    /// layout lookup during code generation.
    pub struct MatchScrutineeTable, IrNodeId => EnumTypeId
);

crate::impl_named_side_table!(
    /// Side-table mapping match IrNodeIds to their dispatch metadata.
    ///
    /// PA-r15-009c (#1055): populated during elaboration for match
    /// expressions carrying the `@jump_table` attribute. #1032's codegen
    /// consults this to decide between jump-table and cmp/jne dispatch.
    pub struct MatchDispatchMetaSideTable, IrNodeId => MatchDispatchMeta
);
