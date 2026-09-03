//! Side-table: struct-decl `NodeId` → its list of [`StructAttr`] entries.
//!
//! paideia-as#1373 (v0.28-M1-004). Struct-level attributes — the first
//! being `@packed_struct` — attach here rather than growing every
//! construction/destructuring site of [`crate::ItemData::Struct`]. New
//! primitives (b2-05 `@endian` at field level goes elsewhere; future
//! struct-level primitives land here) extend the table by pushing an
//! additional [`StructAttr`] onto the struct's entry.
//!
//! Design parallels [`crate::ItemAtomicTable`] and
//! [`crate::PatternTypeHints`] — a sparse per-node side-table on the
//! [`crate::AstArena`], populated by the parser and read by later
//! phases (elaborator layout pass, IR emit).

use std::collections::HashMap;

use crate::items::StructAttr;
use crate::node_id::NodeId;

/// Maps Struct-item `NodeId` → its list of [`StructAttr`] entries.
///
/// Sparse: only struct decls that carry at least one struct-level
/// attribute at parse time have an entry. Absence is the common case
/// and is exposed as an empty slice by [`Self::get`], so callers never
/// have to distinguish "no entry" from "empty entry" — a struct with
/// no annotations reads back the same way as one whose entry has been
/// drained.
#[derive(Debug, Default)]
pub struct StructAttrTable {
    entries: HashMap<NodeId, Vec<StructAttr>>,
}

impl StructAttrTable {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Append `attr` to the struct's attribute list, creating the list
    /// on first insert. Preserves insertion order so downstream phases
    /// can rely on source order when several attributes stack on one
    /// struct.
    pub fn push(&mut self, struct_id: NodeId, attr: StructAttr) {
        self.entries.entry(struct_id).or_default().push(attr);
    }

    /// Look up the attribute list for a struct decl.
    ///
    /// Returns `None` when no attribute has ever been recorded for the
    /// struct — matching the [`crate::StructFieldAttrTable::get`]
    /// convention so both side-tables read the same way at the call
    /// site. When present, the slice preserves insertion order.
    #[must_use]
    pub fn get(&self, struct_id: NodeId) -> Option<&[StructAttr]> {
        self.entries.get(&struct_id).map(Vec::as_slice)
    }

    /// Number of struct decls with at least one recorded attribute.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff the table has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over `(struct-id, attribute-slice)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &[StructAttr])> + '_ {
        self.entries.iter().map(|(k, v)| (*k, v.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let t = StructAttrTable::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn push_and_get_single() {
        let mut t = StructAttrTable::new();
        let id = NodeId::new(5).unwrap();
        t.push(id, StructAttr::Packed { align: None });
        let attrs = t.get(id).expect("entry present");
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0], StructAttr::Packed { align: None });
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn get_absent_returns_none() {
        let t = StructAttrTable::new();
        assert!(t.get(NodeId::new(1).unwrap()).is_none());
    }

    #[test]
    fn push_preserves_insertion_order() {
        let mut t = StructAttrTable::new();
        let id = NodeId::new(3).unwrap();
        t.push(id, StructAttr::Packed { align: None });
        t.push(id, StructAttr::Packed { align: Some(4) });
        let attrs = t.get(id).expect("entry present");
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0], StructAttr::Packed { align: None });
        assert_eq!(attrs[1], StructAttr::Packed { align: Some(4) });
        assert_eq!(t.len(), 1);
    }
}
