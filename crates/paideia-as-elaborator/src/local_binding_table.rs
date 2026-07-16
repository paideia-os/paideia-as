//! LocalBindingTable — Phase 7 m1-001 scratch register assignment for function-local bindings.
//!
//! PA10-005: Nested var lookup in deep block bodies.
//! Tracks the mapping from binding names (from let-statements) to scratch register slots
//! during function body emission. Supports the 4-slot calling-convention scratch sequence:
//! RAX(0), RCX(1), RDX(2), R8(8).
//!
//! §3.1 Architecture: Implements a scope stack for nested blocks with flat fallback.
//! - `scopes`: Vec<HashMap<String, BindingEntry>> — stack of scope levels; [0] = function-root
//! - `flat`: HashMap<String, BindingEntry> — union of all bindings (for resolve_var_operands fallback)
//!
//! #1154: BindingEntry now supports optional payload_reg for register-form enum pair bindings.
//!
//! Push/pop explicit scope boundaries when entering/exiting block arms.
//! Flat fallback resolves post-walk Var operands not found in current stack walk.

use paideia_as_ir::instruction::RegId;
use std::collections::{HashMap, HashSet};

/// A local binding entry with optional pair-register support.
///
/// #1154: Register-form enum scrutinee bindings can carry both a discriminant register
/// (reg) and an optional payload register (payload). Scalar bindings have payload=None.
/// Public — needed by emit_enum_match.rs consumers.
#[derive(Debug, Clone, Copy)]
pub struct BindingEntry {
    /// Primary register (discriminant for enums, sole register for scalars).
    pub reg: RegId,
    /// Optional payload register for register-form enum bindings.
    pub payload: Option<RegId>,
}

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
    /// Each scope is a HashMap from binding name to BindingEntry.
    scopes: Vec<HashMap<String, BindingEntry>>,

    /// Union of all bindings across all scopes (for resolve_var_operands fallback).
    /// When stack-walk returns None, fallback to flat lookup.
    flat: HashMap<String, BindingEntry>,
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
    /// Constructs a BindingEntry with payload=None (scalar binding).
    pub fn insert(&mut self, name: String, reg: RegId) {
        let entry = BindingEntry {
            reg,
            payload: None,
        };
        // Insert into top scope
        if let Some(top_scope) = self.scopes.last_mut() {
            top_scope.insert(name.clone(), entry);
        }
        // Insert into flat union
        self.flat.insert(name, entry);
    }

    /// Register a pair binding (discriminant + payload) for register-form enums.
    /// #1154: Constructs a BindingEntry with payload=Some(payload_reg).
    pub fn insert_pair(&mut self, name: String, reg: RegId, payload: RegId) {
        let entry = BindingEntry {
            reg,
            payload: Some(payload),
        };
        // Insert into top scope
        if let Some(top_scope) = self.scopes.last_mut() {
            top_scope.insert(name.clone(), entry);
        }
        // Insert into flat union
        self.flat.insert(name, entry);
    }

    /// Look up a binding by walking scopes top-down; if none found, fall back to flat.
    /// PA10-005 §3.1: scope walk with flat fallback for post-walk resolve_var_operands.
    /// Returns the primary register (reg field of BindingEntry). External API unchanged.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<RegId> {
        // Walk scopes from top (most recent) to root
        for scope in self.scopes.iter().rev() {
            if let Some(entry) = scope.get(name) {
                return Some(entry.reg);
            }
        }
        // Fallback to flat union if stack-walk yields None
        self.flat.get(name).map(|entry| entry.reg)
    }

    /// Look up a binding pair (reg, payload) by walking scopes top-down; if none found, fall back to flat.
    /// #1154: Returns (primary_reg, optional_payload_reg). Mirrors the scope walk of get().
    #[must_use]
    pub fn get_pair(&self, name: &str) -> Option<(RegId, Option<RegId>)> {
        // Walk scopes from top (most recent) to root
        for scope in self.scopes.iter().rev() {
            if let Some(entry) = scope.get(name) {
                return Some((entry.reg, entry.payload));
            }
        }
        // Fallback to flat union if stack-walk yields None
        self.flat.get(name).map(|entry| (entry.reg, entry.payload))
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
    /// Returns the primary register (reg field) for each binding.
    pub fn iter(&self) -> impl Iterator<Item = (&String, RegId)> + '_ {
        self.flat.iter().map(|(name, entry)| (name, entry.reg))
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

    /// #1215: Registers currently committed to any binding in the active scope stack.
    /// Does NOT include bindings in exited scopes (which are only in flat for fallback).
    /// Used by the enum-match pattern-binder pool filter so binders can't land on
    /// a register that is already live for an enclosing pair-binding's
    /// discriminant/payload.
    /// Intended as an outer-entry snapshot for pattern lowering; do not call per-leaf.
    #[must_use]
    pub fn live_regs(&self) -> HashSet<RegId> {
        let mut s = HashSet::new();
        for scope in self.scopes.iter() {
            for e in scope.values() {
                s.insert(e.reg);
                if let Some(p) = e.payload {
                    s.insert(p);
                }
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::abi;

    #[test]
    fn local_binding_table_new_starts_empty() {
        let table = LocalBindingTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn local_binding_table_insert_and_get() {
        let mut table = LocalBindingTable::new();
        let reg = abi::RAX; // RAX

        table.insert("x".to_string(), reg);
        assert_eq!(table.get("x"), Some(reg));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn local_binding_table_multiple_bindings() {
        let mut table = LocalBindingTable::new();

        table.insert("x".to_string(), abi::RAX); // RAX
        table.insert("y".to_string(), abi::RCX); // RCX
        table.insert("z".to_string(), abi::RDX); // RDX

        assert_eq!(table.get("x"), Some(abi::RAX));
        assert_eq!(table.get("y"), Some(abi::RCX));
        assert_eq!(table.get("z"), Some(abi::RDX));
        assert_eq!(table.len(), 3);
    }

    #[test]
    fn local_binding_table_clear() {
        let mut table = LocalBindingTable::new();
        table.insert("x".to_string(), abi::RAX);
        table.insert("y".to_string(), abi::RCX);

        assert_eq!(table.len(), 2);
        table.clear();
        assert!(table.is_empty());
        assert_eq!(table.get("x"), None);
    }

    #[test]
    fn local_binding_table_contains() {
        let mut table = LocalBindingTable::new();
        table.insert("x".to_string(), abi::RAX);

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
        table.insert("x".to_string(), abi::RAX);
        assert_eq!(table.get("x"), Some(abi::RAX));

        // Push nested scope
        table.push_scope();

        // Insert in nested scope
        table.insert("y".to_string(), abi::RCX);
        assert_eq!(table.get("y"), Some(abi::RCX));
        assert_eq!(table.get("x"), Some(abi::RAX)); // Still visible from root

        // Pop back to root
        table.pop_scope();

        // y is gone from scopes but still in flat; x remains in root scope
        assert_eq!(table.get("x"), Some(abi::RAX));
        assert_eq!(table.get("y"), Some(abi::RCX)); // Fallback to flat after stack-walk fails
    }

    /// PA10-005 §3.1: Scope walk finds closest binding top-down.
    #[test]
    fn scope_walk_finds_closest() {
        let mut table = LocalBindingTable::new();

        table.insert("x".to_string(), abi::RAX);
        table.push_scope();
        table.insert("x".to_string(), abi::RCX); // Shadow outer x

        // Walk should find abi::RCX (top scope) not abi::RAX (root)
        assert_eq!(table.get("x"), Some(abi::RCX));

        table.pop_scope();
        assert_eq!(table.get("x"), Some(abi::RAX));
    }

    /// PA10-005 §3.1: Shadow wins in scope walk.
    #[test]
    fn shadow_wins() {
        let mut table = LocalBindingTable::new();

        table.insert("z".to_string(), abi::RDX); // Root: z → r2

        table.push_scope();
        table.insert("z".to_string(), abi::R8); // Nested: z → r8 (shadow)

        // Scope walk finds abi::R8, not abi::RDX
        assert_eq!(table.get("z"), Some(abi::R8));

        table.pop_scope();
        assert_eq!(table.get("z"), Some(abi::RDX));
    }

    /// PA10-005 §3.1: Pop removes inner, flat preserves.
    #[test]
    fn pop_removes_inner_flat_preserves() {
        let mut table = LocalBindingTable::new();

        table.insert("outer".to_string(), abi::RAX);
        table.push_scope();
        table.insert("inner".to_string(), abi::RCX);

        // Before pop, both in flat
        assert!(table.flat.contains_key("outer"));
        assert!(table.flat.contains_key("inner"));

        table.pop_scope();

        // After pop, inner gone from scopes but flat still has it
        assert_eq!(table.get("inner"), Some(abi::RCX)); // Fallback to flat
        assert_eq!(table.get("outer"), Some(abi::RAX)); // Still in root scope
    }

    /// PA10-005 §3.1: Clear resets to single root scope.
    #[test]
    fn clear_resets_to_root() {
        let mut table = LocalBindingTable::new();

        table.insert("x".to_string(), abi::RAX);
        table.push_scope();
        table.insert("y".to_string(), abi::RCX);

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

    /// #1154: insert_pair round-trip: payload set and retrieved.
    #[test]
    fn insert_pair_round_trip() {
        let mut table = LocalBindingTable::new();
        let reg = abi::RAX;
        let payload = abi::RDX;

        table.insert_pair("enum_x".to_string(), reg, payload);
        assert_eq!(table.get_pair("enum_x"), Some((reg, Some(payload))));
        assert_eq!(table.len(), 1);
    }

    /// #1154: get_pair returns None payload for scalar insert().
    #[test]
    fn get_pair_returns_none_payload_for_scalar_insert() {
        let mut table = LocalBindingTable::new();
        let reg = abi::RCX;

        table.insert("scalar_y".to_string(), reg);
        assert_eq!(table.get_pair("scalar_y"), Some((reg, None)));
        // Single-reg API still works
        assert_eq!(table.get("scalar_y"), Some(reg));
    }

    /// #1154: Shadow with pair wins in scope walk.
    #[test]
    fn shadow_with_pair_wins() {
        let mut table = LocalBindingTable::new();

        // Root: scalar binding z → RAX
        table.insert("z".to_string(), abi::RAX);
        assert_eq!(table.get_pair("z"), Some((abi::RAX, None)));

        // Nested: pair binding z → RCX (discriminant), RDX (payload) (shadow)
        table.push_scope();
        table.insert_pair("z".to_string(), abi::RCX, abi::RDX);

        // Scope walk finds the pair, not the scalar
        assert_eq!(table.get_pair("z"), Some((abi::RCX, Some(abi::RDX))));

        // Pop back to root
        table.pop_scope();
        assert_eq!(table.get_pair("z"), Some((abi::RAX, None)));
    }

    /// #1154: Pop preserves pair in flat.
    #[test]
    fn pop_preserves_pair_in_flat() {
        let mut table = LocalBindingTable::new();

        // Root: scalar x → RAX
        table.insert("x".to_string(), abi::RAX);
        table.push_scope();

        // Nested: pair y → RCX (disc), RDX (payload)
        table.insert_pair("y".to_string(), abi::RCX, abi::RDX);
        assert_eq!(table.get_pair("y"), Some((abi::RCX, Some(abi::RDX))));

        // Pop back to root; y should still be found via flat fallback
        table.pop_scope();
        assert_eq!(table.get_pair("y"), Some((abi::RCX, Some(abi::RDX)))); // From flat
        assert_eq!(table.get("x"), Some(abi::RAX)); // Still in root scope
    }

    /// #1215: live_regs includes both registers from pair bindings.
    #[test]
    fn live_regs_includes_pair_binding_both_regs() {
        let mut table = LocalBindingTable::new();

        // Insert pair binding: x → RCX (disc), RDX (payload)
        table.insert_pair("x".to_string(), abi::RCX, abi::RDX);

        let live = table.live_regs();
        assert!(live.contains(&abi::RCX), "live_regs should contain disc register RCX");
        assert!(live.contains(&abi::RDX), "live_regs should contain payload register RDX");
        assert_eq!(live.len(), 2);
    }

    /// #1215: live_regs includes only primary register for scalar bindings.
    #[test]
    fn live_regs_includes_scalar_binding_single_reg() {
        let mut table = LocalBindingTable::new();

        // Insert scalar binding: y → RAX
        table.insert("y".to_string(), abi::RAX);

        let live = table.live_regs();
        assert!(live.contains(&abi::RAX), "live_regs should contain scalar binding's register RAX");
        assert_eq!(live.len(), 1);
    }

    /// #1220: iter() yields primary only; live_regs() is pair-aware.
    /// Locking the semantics to prevent regression where iter() was used
    /// to detect register liveness, missing pair-binding payload_regs.
    #[test]
    fn iter_yields_primary_only_live_regs_is_pair_aware() {
        let mut table = LocalBindingTable::new();
        table.push_scope();
        table.insert_pair("s".to_string(), abi::RCX, abi::RDX);
        table.insert("scalar".to_string(), abi::R8);

        let iter_regs: HashSet<RegId> = table.iter().map(|(_, r)| r).collect();
        assert_eq!(iter_regs, HashSet::from([abi::RCX, abi::R8]));
        // RDX (payload) NOT in iter — that's #1220's whole point
        assert!(!iter_regs.contains(&abi::RDX));

        let live = table.live_regs();
        assert!(live.contains(&abi::RCX));
        assert!(live.contains(&abi::RDX));  // payload IS in live_regs
        assert!(live.contains(&abi::R8));
        assert_eq!(live.len(), 3);
    }
}
