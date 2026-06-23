//! LocalBindingTable — Phase 7 m1-001 scratch register assignment for function-local bindings.
//!
//! PA10-005: Nested var lookup in deep block bodies.
//! Tracks the mapping from binding names (from let-statements) to scratch register slots
//! during function body emission. Supports the 4-slot calling-convention scratch sequence:
//! RAX(0), RCX(1), RDX(2), R8(8).
//!
//! §3.1 Architecture: Implements a scope stack for nested blocks with flat fallback.
//! - `scopes`: Vec<HashMap<String, RegId>> — stack of scope levels; [0] = function-root
//! - `flat`: HashMap<String, RegId> — union of all bindings (for resolve_var_operands fallback)
//!
//! Push/pop explicit scope boundaries when entering/exiting block arms.
//! Flat fallback resolves post-walk Var operands not found in current stack walk.

use paideia_as_ir::instruction::RegId;
use std::collections::HashMap;

/// Tracks local bindings within a function to their assigned scratch registers,
/// with support for nested scopes (e.g., if/else arms, match arms).
///
/// During emission of multi-statement function bodies, each `let x = expr` statement
/// gets assigned the next available scratch register from the calling-convention sequence
/// (RAX, RCX, RDX, R8). This table maintains a scope stack to handle nested block bodies
/// and a flat union for post-walk variable resolution.
///
/// Bindings are scoped to the current function and cleared at function entry.
#[derive(Debug, Clone)]
pub struct LocalBindingTable {
    /// Stack of scopes; scopes[0] is the function root.
    /// Each scope is a HashMap from binding name to RegId.
    scopes: Vec<HashMap<String, RegId>>,

    /// Union of all bindings across all scopes (for resolve_var_operands fallback).
    /// When stack-walk returns None, fallback to flat lookup.
    flat: HashMap<String, RegId>,
}

impl Default for LocalBindingTable {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalBindingTable {
    /// Create a new, empty LocalBindingTable with root scope.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            flat: HashMap::new(),
        }
    }

    /// Register a binding and its assigned scratch register in the top scope AND flat.
    /// PA10-005 §3.1: inserts into both top scope and flat union.
    pub fn insert(&mut self, name: String, reg: RegId) {
        // Insert into top scope
        if let Some(top_scope) = self.scopes.last_mut() {
            top_scope.insert(name.clone(), reg);
        }
        // Insert into flat union
        self.flat.insert(name, reg);
    }

    /// Look up a binding by walking scopes top-down; if none found, fall back to flat.
    /// PA10-005 §3.1: scope walk with flat fallback for post-walk resolve_var_operands.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<RegId> {
        // Walk scopes from top (most recent) to root
        for scope in self.scopes.iter().rev() {
            if let Some(&reg) = scope.get(name) {
                return Some(reg);
            }
        }
        // Fallback to flat union if stack-walk yields None
        self.flat.get(name).copied()
    }

    /// Push a new scope onto the stack (entering a nested block).
    /// PA10-005 §3.1: explicit scope-boundary marker.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the top scope from the stack (exiting a nested block).
    /// PA10-005 §3.1: explicit scope-boundary cleanup (but flat is preserved).
    /// Panics if popping below root scope (guards invariant).
    pub fn pop_scope(&mut self) {
        if self.scopes.len() <= 1 {
            panic!(
                "LocalBindingTable::pop_scope: attempted to pop below root scope (len={})",
                self.scopes.len()
            );
        }
        self.scopes.pop();
    }

    /// Clear all bindings and reset to single root scope.
    /// PA10-005 §3.1: reset at function entry.
    pub fn clear(&mut self) {
        self.scopes.clear();
        self.scopes.push(HashMap::new());
        self.flat.clear();
    }

    /// Check if a binding is registered in any scope or flat.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.flat.contains_key(name)
    }

    /// Iterate over all flat bindings (backward-compat surface for len/is_empty/iter).
    pub fn iter(&self) -> impl Iterator<Item = (&String, &RegId)> {
        self.flat.iter()
    }

    /// Return the number of registered bindings (flat count).
    /// PA10-005 §3.1: flat operation for backward-compat surface.
    #[must_use]
    pub fn len(&self) -> usize {
        self.flat.len()
    }

    /// Check if the table is empty (flat operation).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.flat.is_empty()
    }

    /// Return the current scope stack depth (for debug assertions).
    /// PA10-005 §3.2: used to verify scope balance in emit_block_body_arm.
    #[must_use]
    pub fn scopes_len(&self) -> usize {
        self.scopes.len()
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

    /// PA10-005 §3.1: Push/pop balance and explicit scope management.
    #[test]
    fn push_pop_balance() {
        let mut table = LocalBindingTable::new();

        // Insert at root scope
        table.insert("x".to_string(), RegId(0));
        assert_eq!(table.get("x"), Some(RegId(0)));

        // Push nested scope
        table.push_scope();

        // Insert in nested scope
        table.insert("y".to_string(), RegId(1));
        assert_eq!(table.get("y"), Some(RegId(1)));
        assert_eq!(table.get("x"), Some(RegId(0))); // Still visible from root

        // Pop back to root
        table.pop_scope();

        // y is gone from scopes but still in flat; x remains in root scope
        assert_eq!(table.get("x"), Some(RegId(0)));
        assert_eq!(table.get("y"), Some(RegId(1))); // Fallback to flat after stack-walk fails
    }

    /// PA10-005 §3.1: Scope walk finds closest binding top-down.
    #[test]
    fn scope_walk_finds_closest() {
        let mut table = LocalBindingTable::new();

        table.insert("x".to_string(), RegId(0));
        table.push_scope();
        table.insert("x".to_string(), RegId(1)); // Shadow outer x

        // Walk should find RegId(1) (top scope) not RegId(0) (root)
        assert_eq!(table.get("x"), Some(RegId(1)));

        table.pop_scope();
        assert_eq!(table.get("x"), Some(RegId(0)));
    }

    /// PA10-005 §3.1: Shadow wins in scope walk.
    #[test]
    fn shadow_wins() {
        let mut table = LocalBindingTable::new();

        table.insert("z".to_string(), RegId(2)); // Root: z → r2

        table.push_scope();
        table.insert("z".to_string(), RegId(8)); // Nested: z → r8 (shadow)

        // Scope walk finds RegId(8), not RegId(2)
        assert_eq!(table.get("z"), Some(RegId(8)));

        table.pop_scope();
        assert_eq!(table.get("z"), Some(RegId(2)));
    }

    /// PA10-005 §3.1: Pop removes inner, flat preserves.
    #[test]
    fn pop_removes_inner_flat_preserves() {
        let mut table = LocalBindingTable::new();

        table.insert("outer".to_string(), RegId(0));
        table.push_scope();
        table.insert("inner".to_string(), RegId(1));

        // Before pop, both in flat
        assert!(table.flat.contains_key("outer"));
        assert!(table.flat.contains_key("inner"));

        table.pop_scope();

        // After pop, inner gone from scopes but flat still has it
        assert_eq!(table.get("inner"), Some(RegId(1))); // Fallback to flat
        assert_eq!(table.get("outer"), Some(RegId(0))); // Still in root scope
    }

    /// PA10-005 §3.1: Clear resets to single root scope.
    #[test]
    fn clear_resets_to_root() {
        let mut table = LocalBindingTable::new();

        table.insert("x".to_string(), RegId(0));
        table.push_scope();
        table.insert("y".to_string(), RegId(1));

        assert_eq!(table.len(), 2);
        assert_eq!(table.scopes.len(), 2);

        table.clear();

        assert!(table.is_empty());
        assert_eq!(table.scopes.len(), 1);
        assert_eq!(table.flat.len(), 0);
    }

    /// PA10-005 §3.1: Double pop panics (guards invariant).
    #[test]
    #[should_panic]
    fn double_pop_panics() {
        let mut table = LocalBindingTable::new();
        table.push_scope(); // Now 2 scopes: [root, nested]
        table.pop_scope(); // Now 1 scope: [root]
        table.pop_scope(); // Panics: already at root
    }
}
