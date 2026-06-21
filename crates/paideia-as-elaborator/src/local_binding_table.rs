//! LocalBindingTable — Phase 7 m1-001 scratch register assignment for function-local bindings.
//!
//! Tracks the mapping from binding names (from let-statements) to scratch register slots
//! during function body emission. Supports the 4-slot calling-convention scratch sequence:
//! RAX(0), RCX(1), RDX(2), R8(8).

use paideia_as_ir::instruction::RegId;
use std::collections::HashMap;

/// Tracks local bindings within a function to their assigned scratch registers.
///
/// During emission of multi-statement function bodies, each `let x = expr` statement
/// gets assigned the next available scratch register from the calling-convention sequence
/// (RAX, RCX, RDX, R8). This table maps binding names to their RegId.
///
/// Bindings are scoped to the current function and cleared at function entry.
#[derive(Debug, Default, Clone)]
pub struct LocalBindingTable {
    /// Mapping from binding name to assigned scratch register.
    bindings: HashMap<String, RegId>,
}

impl LocalBindingTable {
    /// Create a new, empty LocalBindingTable.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Register a binding and its assigned scratch register.
    pub fn insert(&mut self, name: String, reg: RegId) {
        self.bindings.insert(name, reg);
    }

    /// Look up a binding by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<RegId> {
        self.bindings.get(name).copied()
    }

    /// Clear all bindings (called at function entry).
    pub fn clear(&mut self) {
        self.bindings.clear();
    }

    /// Check if a binding is registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// Iterate over all bindings.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &RegId)> {
        self.bindings.iter()
    }

    /// Return the number of registered bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Check if the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_binding_table_new_starts_empty() {
        let table = LocalBindingTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn local_binding_table_insert_and_get() {
        let mut table = LocalBindingTable::new();
        let reg = RegId(0); // RAX

        table.insert("x".to_string(), reg);
        assert_eq!(table.get("x"), Some(reg));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn local_binding_table_multiple_bindings() {
        let mut table = LocalBindingTable::new();

        table.insert("x".to_string(), RegId(0)); // RAX
        table.insert("y".to_string(), RegId(1)); // RCX
        table.insert("z".to_string(), RegId(2)); // RDX

        assert_eq!(table.get("x"), Some(RegId(0)));
        assert_eq!(table.get("y"), Some(RegId(1)));
        assert_eq!(table.get("z"), Some(RegId(2)));
        assert_eq!(table.len(), 3);
    }

    #[test]
    fn local_binding_table_clear() {
        let mut table = LocalBindingTable::new();
        table.insert("x".to_string(), RegId(0));
        table.insert("y".to_string(), RegId(1));

        assert_eq!(table.len(), 2);
        table.clear();
        assert!(table.is_empty());
        assert_eq!(table.get("x"), None);
    }

    #[test]
    fn local_binding_table_contains() {
        let mut table = LocalBindingTable::new();
        table.insert("x".to_string(), RegId(0));

        assert!(table.contains("x"));
        assert!(!table.contains("y"));
    }

    #[test]
    fn local_binding_table_default() {
        let table = LocalBindingTable::default();
        assert!(table.is_empty());
    }
}
