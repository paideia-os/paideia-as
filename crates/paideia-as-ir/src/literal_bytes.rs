//! Side-table for raw byte slices backing StringLiteral IR nodes.
//!
//! Maps `IrNodeId` (StringLiteral nodes) to their immutable byte payloads.
//! This enables the elaborator to intern byte sequences and the emitter to
//! deduplicate identical strings into single .rodata symbols with relocations.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Side-table mapping IrNodeId (StringLiteral nodes) → Vec<u8>.
///
/// Pattern: mirrors LiteralValueTable. Stores the raw UTF-8 (or byte-literal)
/// payload of each StringLiteral node, indexed by node ID for O(1) lookup.
///
/// The elaborator populates this table during lowering;
/// the emitter reads entries to intern bytes and emit .rodata symbols.
#[derive(Default, Debug, Clone)]
pub struct LiteralBytesTable {
    /// Sparse mapping: StringLiteral node id -> Vec<u8> payload.
    entries: HashMap<IrNodeId, Vec<u8>>,
}

impl LiteralBytesTable {
    /// Construct an empty literal bytes side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the byte payload for a StringLiteral node.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, bytes: Vec<u8>) -> Option<Vec<u8>> {
        self.entries.insert(id, bytes)
    }

    /// Look up the byte payload for a StringLiteral node.
    ///
    /// Returns `None` if the node was never registered.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<&Vec<u8>> {
        self.entries.get(&id)
    }

    /// Look up (mutable) the byte payload for a StringLiteral node.
    ///
    /// Allows downstream passes to mutate the payload (if needed).
    pub fn get_mut(&mut self, id: IrNodeId) -> Option<&mut Vec<u8>> {
        self.entries.get_mut(&id)
    }

    /// Number of string literals registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no string literals are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Borrow the underlying HashMap (read-only).
    #[must_use]
    pub fn entries(&self) -> &HashMap<IrNodeId, Vec<u8>> {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_bytes_table_insert_and_get() {
        let mut table = LiteralBytesTable::new();
        let str_id = IrNodeId::new(1).unwrap();
        let bytes = b"hello".to_vec();

        table.insert(str_id, bytes.clone());
        assert_eq!(table.get(str_id), Some(&bytes));
    }

    #[test]
    fn literal_bytes_table_distinct_ids_distinct_entries() {
        let mut table = LiteralBytesTable::new();
        let id1 = IrNodeId::new(1).unwrap();
        let id2 = IrNodeId::new(2).unwrap();
        let id3 = IrNodeId::new(3).unwrap();

        let bytes1 = b"banner".to_vec();
        let bytes2 = b"hello".to_vec();
        let bytes3 = b"world".to_vec();

        table.insert(id1, bytes1.clone());
        table.insert(id2, bytes2.clone());
        table.insert(id3, bytes3.clone());

        assert_eq!(table.get(id1), Some(&bytes1));
        assert_eq!(table.get(id2), Some(&bytes2));
        assert_eq!(table.get(id3), Some(&bytes3));
        assert_eq!(table.len(), 3);
    }

    #[test]
    fn literal_bytes_table_get_returns_none_for_missing() {
        let table = LiteralBytesTable::new();
        let unset_id = IrNodeId::new(999).unwrap();
        assert_eq!(table.get(unset_id), None);
    }

    #[test]
    fn literal_bytes_table_empty_by_default() {
        let table = LiteralBytesTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }
}
