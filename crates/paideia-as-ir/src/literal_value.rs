//! Side-table for interned 64-bit literal values.
//!
//! Maps IrNodeId (Literal nodes) to their i64 values. This enables downstream
//! passes (e.g., elaborator) to access the numeric payload of literals without
//! bloating IrNodeData.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Side-table mapping IrNodeId (Literal nodes) → i64 value.
///
/// Pattern: m3-007 HandlerSideTable / m1-006 LoadStoreSideTable.
/// Keeps IrNodeData ≤ 48 bytes while allowing arbitrary immediate values.
#[derive(Default, Debug, Clone)]
pub struct LiteralValueTable {
    /// Sparse mapping: literal node id -> i64 value.
    entries: HashMap<IrNodeId, i64>,
}

impl LiteralValueTable {
    /// Construct an empty literal value side-table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) a literal value.
    ///
    /// Returns the previous entry if one existed.
    pub fn insert(&mut self, id: IrNodeId, value: i64) -> Option<i64> {
        self.entries.insert(id, value)
    }

    /// Look up a literal value.
    ///
    /// Returns `None` if the node was never registered.
    #[must_use]
    pub fn get(&self, id: IrNodeId) -> Option<i64> {
        self.entries.get(&id).copied()
    }

    /// Number of literal values registered in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` iff no literal values are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove a literal value entry.
    ///
    /// Returns the value if one existed.
    pub fn remove(&mut self, id: IrNodeId) -> Option<i64> {
        self.entries.remove(&id)
    }

    /// Borrow the underlying HashMap (read-only).
    #[must_use]
    pub fn entries(&self) -> &std::collections::HashMap<IrNodeId, i64> {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_value_table_insert_and_get() {
        let mut table = LiteralValueTable::new();
        let lit_id = IrNodeId::new(1).unwrap();

        table.insert(lit_id, 42);
        assert_eq!(table.get(lit_id), Some(42));
    }

    #[test]
    fn literal_value_table_get_returns_none_for_unknown() {
        let table = LiteralValueTable::new();
        let unknown_id = IrNodeId::new(999).unwrap();

        assert_eq!(table.get(unknown_id), None);
    }

    #[test]
    fn literal_value_table_remove_returns_value() {
        let mut table = LiteralValueTable::new();
        let lit_id = IrNodeId::new(1).unwrap();

        let value = 0xCAFE_F00D_DEAD_BEEFu64 as i64;
        table.insert(lit_id, value);
        assert_eq!(table.len(), 1);

        let removed = table.remove(lit_id);
        assert_eq!(removed, Some(value));
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn literal_value_table_negative_values() {
        let mut table = LiteralValueTable::new();
        let lit_id = IrNodeId::new(1).unwrap();

        table.insert(lit_id, -1);
        assert_eq!(table.get(lit_id), Some(-1));
    }
}
