//! Binding name side-table for top-level Let bindings.
//!
//! Maps Let node IDs to their binding identifier names (e.g., `_start`, `_anchor`).
//! This enables downstream passes (elaborator, emitter) to access the actual
//! identifier text without bloating IrNodeData.
//!
//! Pattern: m3-007 HandlerSideTable / m1-006 LoadStoreSideTable.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Side-table mapping Let node IDs → binding name.
///
/// Tracks the identifier name for top-level Let bindings,
/// allowing the elaborator to associate IR nodes with their AST binding names.
#[derive(Default, Debug, Clone)]
pub struct BindingNameTable {
    /// Sparse mapping: Let node id -> binding name.
    entries: HashMap<IrNodeId, String>,
}

impl BindingNameTable {
    /// Construct an empty binding name side-table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert or update a binding name for a Let node.
    pub fn insert(&mut self, node_id: IrNodeId, name: String) {
        self.entries.insert(node_id, name);
    }

    /// Retrieve the binding name for a Let node.
    #[must_use]
    pub fn get(&self, node_id: IrNodeId) -> Option<&str> {
        self.entries.get(&node_id).map(|s| s.as_str())
    }

    /// Check if the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of entries in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Clear all entries from the table.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Iterate over all entries (node_id, name).
    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = (&IrNodeId, &String)> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_name_table_insert_and_get() {
        let mut table = BindingNameTable::new();
        let node_id = IrNodeId::new(42).unwrap();
        table.insert(node_id, "_start".to_string());

        assert_eq!(table.get(node_id), Some("_start"));
    }

    #[test]
    fn binding_name_table_get_missing_returns_none() {
        let table = BindingNameTable::new();
        let node_id = IrNodeId::new(99).unwrap();

        assert_eq!(table.get(node_id), None);
    }

    #[test]
    fn binding_name_table_len() {
        let mut table = BindingNameTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        table.insert(IrNodeId::new(1).unwrap(), "foo".to_string());
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }

    #[test]
    fn binding_name_table_clear() {
        let mut table = BindingNameTable::new();
        table.insert(IrNodeId::new(1).unwrap(), "foo".to_string());
        table.insert(IrNodeId::new(2).unwrap(), "bar".to_string());
        assert_eq!(table.len(), 2);

        table.clear();
        assert!(table.is_empty());
    }

    #[test]
    fn binding_name_table_iter() {
        let mut table = BindingNameTable::new();
        table.insert(IrNodeId::new(1).unwrap(), "foo".to_string());
        table.insert(IrNodeId::new(2).unwrap(), "bar".to_string());

        let mut names: Vec<_> = table.iter().map(|(_, n)| n.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["bar", "foo"]);
    }
}
