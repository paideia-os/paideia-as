//! Module-qualified field reference side-table.
//!
//! Populated for FieldAccess IR nodes whose receiver is a module name (not
//! a struct-typed binding). Value = the bare field symbol name (e.g.
//! "_current_tcb" for `Runqueue._current_tcb`). Enables emit_field_access to
//! resolve the write to a RIP-relative store against a bare exported symbol,
//! bypassing the struct-layout path that requires FieldAccessInfo.
//!
//! Issue #1182 (R15.M4): unblocks cross-module field assignment in switch.pdx.

use crate::node::IrNodeId;
use std::collections::HashMap;

/// Side-table mapping FieldAccess node IDs → module field name.
///
/// Tracks the field name for cross-module field references where the receiver
/// is a module name (not a struct-typed binding). Allows the emitter to emit
/// a RIP-relative store against the bare exported symbol.
#[derive(Default, Debug, Clone)]
pub struct ModuleFieldRefTable {
    /// Sparse mapping: FieldAccess node id -> field name.
    entries: HashMap<IrNodeId, String>,
}

impl ModuleFieldRefTable {
    /// Construct an empty module-field-ref side-table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert or update a field name for a FieldAccess node.
    pub fn insert(&mut self, node_id: IrNodeId, name: String) {
        self.entries.insert(node_id, name);
    }

    /// Retrieve the field name for a FieldAccess node.
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
    fn module_field_ref_table_insert_and_get() {
        let mut table = ModuleFieldRefTable::new();
        let node_id = IrNodeId::new(42).unwrap();
        table.insert(node_id, "_current_tcb".to_string());

        assert_eq!(table.get(node_id), Some("_current_tcb"));
    }

    #[test]
    fn module_field_ref_table_get_missing_returns_none() {
        let table = ModuleFieldRefTable::new();
        let node_id = IrNodeId::new(99).unwrap();

        assert_eq!(table.get(node_id), None);
    }

    #[test]
    fn module_field_ref_table_len() {
        let mut table = ModuleFieldRefTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        table.insert(IrNodeId::new(1).unwrap(), "foo".to_string());
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }

    #[test]
    fn module_field_ref_table_clear() {
        let mut table = ModuleFieldRefTable::new();
        table.insert(IrNodeId::new(1).unwrap(), "foo".to_string());
        table.insert(IrNodeId::new(2).unwrap(), "bar".to_string());
        assert_eq!(table.len(), 2);

        table.clear();
        assert!(table.is_empty());
    }

    #[test]
    fn module_field_ref_table_iter() {
        let mut table = ModuleFieldRefTable::new();
        table.insert(IrNodeId::new(1).unwrap(), "foo".to_string());
        table.insert(IrNodeId::new(2).unwrap(), "bar".to_string());

        let mut names: Vec<_> = table.iter().map(|(_, n)| n.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["bar", "foo"]);
    }
}
