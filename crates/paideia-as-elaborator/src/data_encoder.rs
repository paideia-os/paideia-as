//! Pure IR → data-section byte encoder.
//!
//! Extracted from `EmitWalker` during the v0.17 refactor. These functions
//! walk an `IrArena` and convert value-producing nodes (Literal, ArrayLit,
//! RecordCons) into little-endian byte sequences suitable for embedding
//! in the ELF/PE `.data` section, then wire them into a `DataSideTable`
//! keyed by `IrNodeId` via [`populate_data_table`].
//!
//! No walker or emission state is required — every function here is a
//! pure transformation over the IR arena.

use paideia_as_ir::{DataEntry, DataSideTable, IrArena, IrKind, IrNodeId, RelocSpec};

/// Pack an i64 value as 8 little-endian bytes.
#[must_use]
pub fn pack_u64_le(value: i64) -> Vec<u8> {
    pack_int_le(value, 8)
}

/// PA10-006s: Pack an integer value as little-endian bytes with the specified
/// byte width.
///
/// Unused high bits are truncated for widths < 8. Width must be 1, 2, 4, or 8.
#[must_use]
pub fn pack_int_le(value: i64, width_bytes: u8) -> Vec<u8> {
    let u64_val = value as u64;
    let full_bytes = u64_val.to_le_bytes();
    full_bytes[..width_bytes as usize].to_vec()
}

/// Encode an ArrayLit node to bytes for data section initialisation.
///
/// Walks element children, recursively encodes each via `encode_ir_value`,
/// and concatenates. Returns `None` if any element is unencodable.
#[must_use]
pub fn encode_array_lit(arena: &IrArena, array_id: IrNodeId) -> Option<Vec<u8>> {
    let children = arena.children(array_id);
    let mut bytes = Vec::new();
    for &elem_id in children {
        let elem_bytes = encode_ir_value(arena, elem_id)?;
        bytes.extend_from_slice(&elem_bytes);
    }
    Some(bytes)
}

/// Issue #1157: Encode an IR value to bytes with a specified width.
///
/// For literals, uses `pack_int_le` to respect the declared width (e.g., u32 = 4 bytes).
/// For Borrow nodes (fnptr fields), emits width bytes of zeros (placeholder for relocation).
/// For composite kinds (ArrayLit, RecordCons, EnumCons), ignores width and dispatches
/// to the recursive `encode_ir_value` since composites carry their own size.
/// Returns `None` if the node is unencodable.
#[must_use]
pub fn encode_ir_value_sized(arena: &IrArena, node_id: IrNodeId, width: u8) -> Option<Vec<u8>> {
    let node = arena.get(node_id)?;
    match node.kind {
        IrKind::Literal => arena.literal_values().get(node_id).map(|v| pack_int_le(v, width)),
        IrKind::Borrow => {
            // Fnptr field: emit width bytes of zeros (placeholder for relocation)
            Some(vec![0u8; width as usize])
        }
        IrKind::ArrayLit | IrKind::RecordCons | IrKind::EnumCons | IrKind::InlineBytes => {
            // Composites carry their own size; ignore width parameter and use standard dispatch
            encode_ir_value(arena, node_id)
        }
        _ => None,
    }
}

/// Issue #1157: Encode a RecordCons node to bytes for data section initialisation.
///
/// Uses the finalised record layout from `arena.finalised_record_layouts()` to determine
/// field offsets and sizes. Allocates a buffer filled with zeros (for padding), then
/// encodes each field at its declared offset and size. Supports nested records and
/// arrays via recursive `encode_ir_value`.
///
/// Returns `None` if the record layout is not available or any field encoding fails.
#[must_use]
pub fn encode_record_cons(arena: &IrArena, record_id: IrNodeId) -> Option<Vec<u8>> {
    // Look up the RecordTypeId for this record constructor
    let type_id = arena.record_layout_table().get(record_id)?;

    // Look up the finalised layout (C ABI natural-alignment packing)
    let layout = arena.finalised_record_layouts().get(*type_id)?;

    // Pre-allocate buffer filled with zeros for padding
    let mut bytes = vec![0u8; layout.size as usize];

    // Encode each field at its declared offset and size
    let children = arena.children(record_id);
    for (i, &field_id) in children[1..].iter().enumerate() {
        // Bounds check: ensure the layout has an entry for this field index
        if i >= layout.fields.len() {
            return None; // Layout is incomplete; encoder can't proceed
        }
        let fl = &layout.fields[i];
        let field_bytes = encode_ir_value_sized(arena, field_id, fl.size)?;
        bytes[fl.offset as usize..fl.offset as usize + fl.size as usize].copy_from_slice(&field_bytes);
    }

    Some(bytes)
}

/// PA-r17-007 (#1050) + Issue #1157: Encode an EnumCons node to bytes for data section initialisation.
///
/// Issue #1157: Allocates a buffer filled with zeros, encodes the discriminant at offset 0,
/// and places the payload at `layout.payload_offset`. For record payloads, the recursive
/// call to `encode_record_cons` handles tight-pack field encoding automatically.
///
/// Payload is encoded by recursively calling encode_ir_value on payload children.
/// Returns None if the enum layout is not available or payload encoding fails.
#[must_use]
pub fn encode_enum_cons(arena: &IrArena, enum_cons_id: IrNodeId) -> Option<Vec<u8>> {
    // Get the enum constructor metadata (type_id and variant_index)
    let info = arena.enum_cons_info().get(enum_cons_id)?;

    // Look up the enum layout for this type
    // If layout is not found, this is a recoverable error - return None so the data binding
    // is skipped (no data entry created). Later, emit_enum_cons will emit a diagnostic.
    let layout = arena.enum_layout_table().get(info.type_id)?;

    // Pre-allocate buffer filled with zeros for padding
    let mut bytes = vec![0u8; layout.size as usize];

    // Encode discriminant as u64 little-endian at offset 0
    let discriminant = info.variant_index as i64;
    let disc_bytes = pack_u64_le(discriminant);
    bytes[0..disc_bytes.len()].copy_from_slice(&disc_bytes);

    // Encode payload and place at payload_offset
    let payload_children = arena.children(enum_cons_id);
    for &payload_id in payload_children {
        let payload_bytes = encode_ir_value(arena, payload_id)?;
        let offset = layout.payload_offset as usize;
        if offset + payload_bytes.len() <= bytes.len() {
            bytes[offset..offset + payload_bytes.len()].copy_from_slice(&payload_bytes);
        } else {
            // Payload extends beyond enum size; truncate to fit
            let copyable = bytes.len() - offset;
            if copyable > 0 {
                bytes[offset..].copy_from_slice(&payload_bytes[..copyable]);
            }
        }
    }

    Some(bytes)
}

/// Recursively encode an IR value node to bytes.
///
/// Dispatches on node kind:
/// - `Literal`: pack as u64 little-endian.
/// - `ArrayLit`: recurse on children.
/// - `RecordCons`: recurse on field values (skip type-name).
/// - `EnumCons`: encode discriminant + payload, padded to enum size (PA-r17-007 #1050).
/// - `InlineBytes`: return raw bytes directly (Issue #1012).
///
/// Returns `None` for kinds that are not directly encodable (Var, App, ...).
#[must_use]
pub fn encode_ir_value(arena: &IrArena, node_id: IrNodeId) -> Option<Vec<u8>> {
    let node = arena.get(node_id)?;
    match node.kind {
        IrKind::Literal => arena.literal_values().get(node_id).map(pack_u64_le),
        IrKind::ArrayLit => encode_array_lit(arena, node_id),
        IrKind::RecordCons => encode_record_cons(arena, node_id),
        IrKind::EnumCons => encode_enum_cons(arena, node_id),
        IrKind::InlineBytes => arena.literal_bytes().get(node_id).cloned(),
        _ => None,
    }
}

/// Populate a `DataSideTable` from the module-level `Let` bindings in `arena`.
///
/// Iterates over every node, filters for `IrKind::Let` with a supported RHS
/// shape (Literal / ArrayLit / RecordCons / Placeholder / StringLiteral / InlineBytes) and
/// inserts a matching [`DataEntry`] under a synthetic `data_<node_id>` symbol.
///
/// Section routing:
/// - initialised + mutable → `.data`
/// - initialised + immutable → `.rodata`
/// - Placeholder (uninit) → `.bss` (regardless of mutability)
/// - StringLiteral → `.rodata` with a relocation to the interned `__str_...` symbol
/// - InlineBytes (Issue #1012) → `.rodata` with direct byte payload
pub fn populate_data_table(arena: &IrArena, data_table: &mut DataSideTable) {
    for i in 1..=arena.len() as u32 {
        let Some(node_id) = IrNodeId::new(i) else { continue };
        let Some(node) = arena.get(node_id) else { continue };
        if node.kind != IrKind::Let {
            continue;
        }
        let Some(&rhs_id) = arena.children(node_id).first() else { continue };
        let Some(rhs_node) = arena.get(rhs_id) else { continue };

        let symbol_name = format!("data_{}", node_id.get());
        let is_mutable = arena
            .let_meta()
            .get(node_id)
            .map(|info| info.mutable)
            .unwrap_or(false);

        match rhs_node.kind {
            IrKind::Literal => {
                if let Some(value) = arena.literal_values().get(rhs_id) {
                    let bytes = pack_u64_le(value);
                    let entry = if is_mutable {
                        DataEntry::new_data(bytes, symbol_name, 8)
                    } else {
                        DataEntry::new_rodata(bytes, symbol_name, 8)
                    };
                    data_table.insert(node_id, entry);
                }
            }
            IrKind::ArrayLit => {
                if let Some(bytes) = encode_array_lit(arena, rhs_id) {
                    let entry = if is_mutable {
                        DataEntry::new_data(bytes, symbol_name, 8)
                    } else {
                        DataEntry::new_rodata(bytes, symbol_name, 8)
                    };
                    data_table.insert(node_id, entry);
                }
            }
            IrKind::RecordCons => {
                if let Some(bytes) = encode_record_cons(arena, rhs_id) {
                    let entry = if is_mutable {
                        DataEntry::new_data(bytes, symbol_name, 8)
                    } else {
                        DataEntry::new_rodata(bytes, symbol_name, 8)
                    };
                    data_table.insert(node_id, entry);
                }
            }
            IrKind::EnumCons => {
                if let Some(bytes) = encode_enum_cons(arena, rhs_id) {
                    let entry = if is_mutable {
                        DataEntry::new_data(bytes, symbol_name, 8)
                    } else {
                        DataEntry::new_rodata(bytes, symbol_name, 8)
                    };
                    data_table.insert(node_id, entry);
                }
            }
            IrKind::Placeholder => {
                // Phase 6 m5-004: all uninit → .bss regardless of mutability.
                let entry = DataEntry::new_bss(symbol_name, 8, 8);
                data_table.insert(node_id, entry);
            }
            IrKind::StringLiteral => {
                if let Some(bytes) = arena.literal_bytes().get(rhs_id) {
                    let rodata_bytes = vec![0u8; 8];
                    let reloc = RelocSpec::new(
                        0,
                        format!("__str_{:016x}", crate::string_intern::fnv1a_64(bytes)),
                    );
                    let entry = DataEntry::new_rodata_with_relocs(
                        rodata_bytes,
                        symbol_name,
                        8,
                        vec![reloc],
                    );
                    data_table.insert(node_id, entry);
                }
            }
            IrKind::InlineBytes => {
                // Issue #1012: @guid and @include_bytes produce InlineBytes in .rodata.
                // Emit the bytes directly without relocation.
                if let Some(bytes) = arena.literal_bytes().get(rhs_id) {
                    let entry = DataEntry::new_rodata(bytes.clone(), symbol_name, 8);
                    data_table.insert(node_id, entry);
                }
            }
            _ => {}
        }
    }
}

/// PA-r15-009b (#1032): Helper to collect jump table entries from arena.
///
/// Scans all Match nodes with `@jump_table` and `density_ok` set, collecting
/// rodata entries for dense jump table matches. Returns owned entries keyed
/// by their IrNodeId, ready to be inserted into a DataSideTable.
///
/// This helper avoids code duplication between the mutable-arena and
/// separate-data-table variants of jump table population.
fn collect_jump_table_entries(arena: &IrArena) -> Vec<(IrNodeId, DataEntry)> {
    let mut entries_to_insert: Vec<(IrNodeId, DataEntry)> = Vec::new();

    for i in 1..=arena.len() as u32 {
        let Some(node_id) = IrNodeId::new(i) else { continue };
        let Some(node) = arena.get(node_id) else { continue };
        if node.kind != IrKind::Match {
            continue;
        }

        // Check if this match has jump_table metadata
        let dispatch_meta = match arena.match_dispatch_meta().get(node_id) {
            Some(m) => *m,  // Clone/copy the metadata
            None => continue,
        };

        // Only synthesize if jump_table && density_ok
        if !dispatch_meta.jump_table || !dispatch_meta.density_ok {
            continue;
        }

        // Get the arm values and clone them
        let arm_values_vec: Vec<(i64, u32)> = match arena.match_jump_table_arm_values().get(node_id) {
            Some(v) => v.clone(),
            None => {
                // No arm values registered — shouldn't happen, but skip if it does
                continue;
            }
        };

        // Build a map: slot_index -> reloc target (arm body label)
        let mut relocs: Vec<RelocSpec> = Vec::new();

        // Initialize all slots to default label
        for slot_index in 0..(dispatch_meta.range as usize) {
            let offset = (slot_index * 8) as u64;
            let default_label = format!("match_default_{}", node_id.get());
            relocs.push(RelocSpec::new(offset, default_label));
        }

        // Override slots with arm-specific labels
        for (arm_value, arm_idx) in arm_values_vec {
            let slot_index = (arm_value - dispatch_meta.min_arm) as usize;
            if slot_index < dispatch_meta.range as usize {
                let offset = (slot_index * 8) as u64;
                let arm_label = format!("match_arm_{}_{}", node_id.get(), arm_idx);
                relocs[slot_index] = RelocSpec::new(offset, arm_label);
            }
        }

        // Allocate the rodata bytes: one W64 slot per entry
        let bytes = vec![0u8; (dispatch_meta.range as usize) * 8];
        let symbol_name = format!("_jt_{}", node_id.get());

        // Create the rodata entry with relocations
        let entry = DataEntry::new_rodata_with_relocs(bytes, symbol_name, 8, relocs);
        entries_to_insert.push((node_id, entry));
    }

    entries_to_insert
}

/// PA-r15-009b (#1032): Populate rodata jump tables from a mutable arena.
///
/// Takes a mutable reference to the arena, collects jump table entries,
/// and inserts them into the arena's data table. This is the primary
/// production entry point, called from cmd_build.rs via EmitWalker.
pub fn populate_jump_tables_from_mutable_arena(arena: &mut IrArena) {
    let entries = collect_jump_table_entries(arena);
    let data_table = arena.data_mut();
    for (node_id, entry) in entries {
        data_table.insert(node_id, entry);
    }
}

/// PA-r15-009b (#1032): Populate rodata jump tables from match dispatch metadata.
///
/// Iterates over all Match nodes with `@jump_table` and `density_ok` set. For each:
/// 1. Allocate a rodata buffer: `(range * 8)` bytes (one W64 slot per index in [min_arm, max_arm])
/// 2. For each arm pattern value, compute `slot_index = value - min_arm` and emit a relocation
///    pointing to the arm body label at offset `slot_index * 8`.
/// 3. For holes (unmatched values), emit relocations pointing to the default label.
/// 4. Wrap in a DataEntry with symbol name `_jt_<match_id>` and insert into data_table.
pub fn populate_jump_tables(arena: &IrArena, data_table: &mut DataSideTable) {
    let entries = collect_jump_table_entries(arena);
    for (node_id, entry) in entries {
        data_table.insert(node_id, entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_u64_le_small_value() {
        let bytes = pack_u64_le(0x0102_0304_0506_0708i64);
        assert_eq!(
            bytes,
            vec![0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );
    }

    #[test]
    fn pack_u64_le_zero() {
        let bytes = pack_u64_le(0);
        assert_eq!(bytes, vec![0; 8]);
    }

    #[test]
    fn pack_u64_le_max() {
        let bytes = pack_u64_le(-1);
        assert_eq!(bytes, vec![0xFF; 8]);
    }

    #[test]
    fn pack_int_le_widths_truncate() {
        assert_eq!(pack_int_le(0x1234_5678, 4), vec![0x78, 0x56, 0x34, 0x12]);
        assert_eq!(pack_int_le(0x1234, 2), vec![0x34, 0x12]);
        assert_eq!(pack_int_le(0x42, 1), vec![0x42]);
    }

    /// PA-r15-009b test: populate_jump_tables_from_mutable_arena creates rodata entries with relocations.
    /// Tests the shipped path that cmd_build.rs actually calls.
    #[test]
    fn populate_jump_tables_creates_rodata_with_relocs() {
        use paideia_as_diagnostics::{FileId, Span};
        use paideia_as_ir::{MatchDispatchMeta, SectionKind};

        fn span() -> Span {
            Span::new(FileId::new(1).unwrap(), 0, 1)
        }

        let mut arena = IrArena::new();

        // Allocate a Match node with arms
        let scrutinee_id = arena.alloc(IrKind::Var, span());
        let arm_body_0 = arena.alloc(IrKind::Literal, span());
        let arm_body_1 = arena.alloc(IrKind::Literal, span());
        let arm_id_0 = arena.alloc_with_children(IrKind::Action, span(), [arm_body_0]);
        let arm_id_1 = arena.alloc_with_children(IrKind::Action, span(), [arm_body_1]);
        let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id_0, arm_id_1]);

        // Register dispatch metadata: dense, jump_table enabled
        arena.match_dispatch_meta_mut().insert(
            match_id,
            MatchDispatchMeta {
                jump_table: true,
                min_arm: 0,
                range: 2,
                covered_arms: 2,
                density_ok: true,
            },
        );

        // Register per-arm values: two arms, values 0 and 1
        arena.match_jump_table_arm_values_mut().insert(
            match_id,
            vec![(0, 0), (1, 1)],
        );

        // Call populate_jump_tables_from_mutable_arena (the shipped variant)
        populate_jump_tables_from_mutable_arena(&mut arena);

        // Verify the entry was created in arena.data()
        let entry = arena.data().get(match_id).expect("jump table entry should exist in arena.data()");

        // Check basic properties
        assert_eq!(entry.section, SectionKind::Rodata, "Jump table should be rodata");
        assert_eq!(entry.symbol_name, format!("_jt_{}", match_id.get()), "Symbol name should be _jt_<id>");
        assert_eq!(entry.align, 8, "Alignment should be 8");
        assert_eq!(entry.bytes.len(), 16, "Should have 2 slots * 8 bytes = 16 bytes total");

        // Check relocations
        assert_eq!(entry.relocations.len(), 2, "Should have 2 relocations (one per arm)");

        // Relocation 0 should point to arm 0
        let reloc0 = &entry.relocations[0];
        assert_eq!(reloc0.offset, 0, "First relocation at offset 0");
        assert!(reloc0.symbol.contains("match_arm"), "Relocation should reference arm body label");

        // Relocation 1 should point to arm 1
        let reloc1 = &entry.relocations[1];
        assert_eq!(reloc1.offset, 8, "Second relocation at offset 8");
        assert!(reloc1.symbol.contains("match_arm"), "Relocation should reference arm body label");
    }

    /// Issue #1091 test: encode_enum_cons with RecordCons payload.
    /// Verifies that an EnumCons with discriminant 1 and RecordCons payload
    /// (with nested Literal fields) encodes correctly, including the record structure.
    #[test]
    fn encode_enum_cons_record_payload_produces_correct_bytes() {
        use paideia_as_diagnostics::{FileId, Span};
        use paideia_as_ir::{EnumConsInfo, EnumLayout, EnumTypeId};
        use paideia_as_ir::record_layout::{RecordTypeId, RecordLayout, FieldLayout};

        fn span() -> Span {
            Span::new(FileId::new(1).unwrap(), 0, 1)
        }

        let mut arena = IrArena::new();

        // Allocate record constructor nodes: RecordCons(type_name="Point", x=Literal(1), y=Literal(2))
        let type_name_id = arena.alloc(IrKind::Literal, span());
        let field_x_id = arena.alloc(IrKind::Literal, span());
        let field_y_id = arena.alloc(IrKind::Literal, span());

        arena.literal_values_mut().insert(field_x_id, 1);
        arena.literal_values_mut().insert(field_y_id, 2);

        let record_id = arena.alloc_with_children(
            IrKind::RecordCons,
            span(),
            [type_name_id, field_x_id, field_y_id],
        );

        // Issue #1157: Register record layout for tight-pack encoding.
        // Point{x: u32, y: u32} has:
        // - x (u32): 4 bytes at offset 0
        // - y (u32): 4 bytes at offset 4
        // - total: 8 bytes
        let record_type_id = RecordTypeId(200);
        arena.record_layout_table_mut().insert(
            record_id,
            record_type_id,
        );
        arena.finalised_record_layouts_mut().insert(
            record_type_id,
            RecordLayout::new(
                8, // size
                4, // align
                vec![
                    FieldLayout { offset: 0, size: 4, signed: false },
                    FieldLayout { offset: 4, size: 4, signed: false },
                ],
            ),
        );

        // Allocate an EnumCons node with discriminant 1 (Ok variant) and RecordCons payload
        let enum_id = arena.alloc_with_children(IrKind::EnumCons, span(), [record_id]);

        // Register the EnumConsInfo for this EnumCons
        let enum_type_id = EnumTypeId(100);
        arena.enum_cons_info_mut().insert(
            enum_id,
            EnumConsInfo {
                type_id: enum_type_id,
                variant_index: 1, // Ok = variant 1
            },
        );

        // Register an EnumLayout for the enum type
        // Issue #1157: payload_offset=8, payload_size=8 (tight record), total_size=16, align=8
        arena.enum_layout_table_mut().insert(
            enum_type_id,
            EnumLayout {
                size: 16,
                align: 8,
                discriminant_size: 8,
                payload_offset: 8,
                payload_size: 8,
            },
        );

        // Encode the enum
        let result = encode_enum_cons(&arena, enum_id);
        assert!(result.is_some(), "encode_enum_cons should succeed with valid record payload");

        let bytes = result.unwrap();

        // Expected byte layout (Issue #1157: tight-pack):
        // Offset 0-7: discriminant 1 as u64 LE = [01 00 00 00 00 00 00 00]
        // Offset 8-11: record field x (u32) = 1 as u32 LE = [01 00 00 00]
        // Offset 12-15: record field y (u32) = 2 as u32 LE = [02 00 00 00]
        let expected = vec![
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // discriminant 1
            0x01, 0x00, 0x00, 0x00,                         // field x = 1 (u32)
            0x02, 0x00, 0x00, 0x00,                         // field y = 2 (u32)
        ];

        assert_eq!(
            bytes, expected,
            "EnumCons(discriminant=1, RecordCons(x:u32=1, y:u32=2)) should encode to tight-pack bytes (16 total)"
        );
    }
}
