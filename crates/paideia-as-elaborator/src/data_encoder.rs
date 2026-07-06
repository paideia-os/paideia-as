//! Pure IR → data-section byte encoder.
//!
//! Extracted from `EmitWalker` during the v0.17 refactor. These functions
//! walk an `IrArena` and convert value-producing nodes (Literal, ArrayLit,
//! RecordCons) into little-endian byte sequences suitable for embedding
//! in the ELF/PE `.data` section.
//!
//! Every function here is pure: no `&self`, no mutation of shared state.
//! The population logic that consumes these bytes lives in
//! `EmitWalker::populate_data_table`.

use paideia_as_ir::{IrArena, IrKind, IrNodeId};

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

/// Encode a RecordCons node to bytes for data section initialisation.
///
/// Assumes all fields are simple literals (u64) and encodes in child order,
/// skipping the leading type-name child. Does not handle nested arrays or
/// records in this MVP.
#[must_use]
pub fn encode_record_cons(arena: &IrArena, record_id: IrNodeId) -> Option<Vec<u8>> {
    let children = arena.children(record_id);
    if children.is_empty() {
        return Some(Vec::new());
    }
    let mut bytes = Vec::new();
    for &field_id in &children[1..] {
        let field_bytes = encode_ir_value(arena, field_id)?;
        bytes.extend_from_slice(&field_bytes);
    }
    Some(bytes)
}

/// Recursively encode an IR value node to bytes.
///
/// Dispatches on node kind:
/// - `Literal`: pack as u64 little-endian.
/// - `ArrayLit`: recurse on children.
/// - `RecordCons`: recurse on field values (skip type-name).
///
/// Returns `None` for kinds that are not directly encodable (Var, App, ...).
#[must_use]
pub fn encode_ir_value(arena: &IrArena, node_id: IrNodeId) -> Option<Vec<u8>> {
    let node = arena.get(node_id)?;
    match node.kind {
        IrKind::Literal => arena.literal_values().get(node_id).map(pack_u64_le),
        IrKind::ArrayLit => encode_array_lit(arena, node_id),
        IrKind::RecordCons => encode_record_cons(arena, node_id),
        _ => None,
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
}
