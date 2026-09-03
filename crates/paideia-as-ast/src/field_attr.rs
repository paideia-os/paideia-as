//! Side-table: struct-field name `NodeId` → `Vec<FieldAttr>`.
//!
//! paideia-as#1372 (v0.28-M1-003). Struct-body fields may carry per-field
//! attributes such as `@endian(be|le)`:
//!
//! ```text
//! struct Header {
//!     @endian(be) magic: u32,
//!     length: u16,
//! }
//! ```
//!
//! Rather than growing every construction / destructuring site of
//! [`crate::ItemData::Struct`] (many downstream files add
//! `field_attrs: vec![]`), we keep field-level attributes on this sparse
//! side-table keyed by the field-name `NodeId` that the parser allocates
//! for each field. The parser inserts into it immediately after allocating
//! the field-name node. Later elaborator phases (`@endian` byte-swap
//! insertion, `@packed_struct` layout — b2-06) read from it.
//!
//! Design parallels [`crate::ItemAtomicTable`] and
//! [`crate::PatternTypeHints`] — all are sparse per-node side-tables on
//! the AST arena.
//!
//! The `FieldAttr` enum is intentionally a single-variant append point:
//! wave-0 batch-2 issue b2-06 (`@packed_struct`) is a *struct-level*
//! attribute, so it does not extend `FieldAttr`, but any future per-field
//! attribute (`@bitfield`, `@align`, …) appends here.

use std::collections::HashMap;

use crate::node_id::NodeId;

/// Byte-order for a `@endian(be|le)` field attribute (paideia-as#1372).
///
/// `Be` — big-endian; the elaborator inserts a byte-swap on load/store so
/// the in-memory representation is most-significant-byte-first regardless
/// of host order.
///
/// `Le` — little-endian; on the x86_64 target this is a no-op, but it is
/// still meaningful as an intent annotation for code review and for future
/// cross-target retargeting.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Endianness {
    /// Big-endian (most-significant byte at lowest address).
    Be,
    /// Little-endian (least-significant byte at lowest address).
    Le,
}

/// Attribute attached to a single struct-body field.
///
/// Phase-1 landing carries a single variant, `Endian(Endianness)`. Future
/// per-field attributes (e.g. `@bitfield(width)`) append additional
/// variants here rather than growing the AST field tuple.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum FieldAttr {
    /// `@endian(be|le)` — byte-order override for an integral scalar field.
    /// Semantics (byte-swap on load/store for `Be` on little-endian host)
    /// are elaborator-side and deferred to a later milestone; parser-only
    /// here.
    Endian(Endianness),
}

/// Maps struct-field-name `NodeId` → `Vec<FieldAttr>`.
///
/// Sparse: only fields that carry at least one attribute at parse time
/// have an entry. Absence is the common case.
#[derive(Debug, Default)]
pub struct StructFieldAttrTable {
    entries: HashMap<NodeId, Vec<FieldAttr>>,
}

impl StructFieldAttrTable {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Append an attribute to the field-name's attribute list. Multiple
    /// attributes on the same field accumulate in insertion order.
    pub fn push(&mut self, field_name: NodeId, attr: FieldAttr) {
        self.entries.entry(field_name).or_default().push(attr);
    }

    /// Look up the attribute list for a field. Returns `None` for fields
    /// without any attribute (the common case).
    #[must_use]
    pub fn get(&self, field_name: NodeId) -> Option<&[FieldAttr]> {
        self.entries.get(&field_name).map(Vec::as_slice)
    }

    /// Number of fields with at least one attribute.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over `(field-id, &[FieldAttr])` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &[FieldAttr])> + '_ {
        self.entries.iter().map(|(k, v)| (*k, v.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let t = StructFieldAttrTable::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn push_and_get() {
        let mut t = StructFieldAttrTable::new();
        let id = NodeId::new(7).unwrap();
        t.push(id, FieldAttr::Endian(Endianness::Be));
        assert_eq!(t.len(), 1);
        assert_eq!(t.get(id), Some(&[FieldAttr::Endian(Endianness::Be)][..]));
    }

    #[test]
    fn get_absent_returns_none() {
        let t = StructFieldAttrTable::new();
        assert!(t.get(NodeId::new(1).unwrap()).is_none());
    }

    #[test]
    fn push_accumulates_on_same_field() {
        let mut t = StructFieldAttrTable::new();
        let id = NodeId::new(3).unwrap();
        t.push(id, FieldAttr::Endian(Endianness::Le));
        t.push(id, FieldAttr::Endian(Endianness::Be));
        let got = t.get(id).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], FieldAttr::Endian(Endianness::Le));
        assert_eq!(got[1], FieldAttr::Endian(Endianness::Be));
        // Only one entry: same field id.
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn endianness_be_and_le_are_distinct() {
        assert_ne!(Endianness::Be, Endianness::Le);
    }
}
