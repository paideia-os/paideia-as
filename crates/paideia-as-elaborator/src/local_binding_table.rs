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
//! #1233: Extended with BindingHome::EnvSlot for R14-relative closure capture access.
//!
//! Push/pop explicit scope boundaries when entering/exiting block arms.
//! Flat fallback resolves post-walk Var operands not found in current stack walk.

use paideia_as_ir::instruction::RegId;
use std::collections::{HashMap, HashSet};

/// Binding home location: register slot or R14-relative memory slot.
///
/// #1233: Captures in closure bodies reside at [r14 + offset] rather than in registers.
/// This enum allows LocalBindingTable to track both register and memory-based bindings
/// for resolution via resolve_var_operands.
/// #995: Closure bindings (fat pointers) reside in a register but are distinct from
/// scalar bindings for dispatch purposes.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BindingHome {
    /// Scalar binding in a single register.
    Reg(RegId),
    /// Enum binding: discriminant + payload register pair (#1154).
    RegPair(RegId, RegId),
    /// Closure capture: [R14 + offset] memory-relative binding (#1233).
    EnvSlot(i32),
    /// Closure binding: fat pointer (env_ptr|code_ptr pair) in a register (#995).
    Closure(RegId),
}

/// A local binding entry wrapping its home location.
///
/// #1154: Register-form enum scrutinee bindings can carry both a discriminant register
/// (reg) and an optional payload register (payload). Scalar bindings have payload=None.
/// #1233: Extended to support EnvSlot (R14-relative) bindings.
/// Public — needed by emit_enum_match.rs consumers.
#[derive(Debug, Clone, Copy)]
pub struct BindingEntry {
    /// Binding home: register(s) or R14-relative memory slot.
    pub home: BindingHome,
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
    /// Constructs a BindingEntry with home=Reg(reg) (scalar binding).
    pub fn insert(&mut self, name: String, reg: RegId) {
        let entry = BindingEntry {
            home: BindingHome::Reg(reg),
        };
        // Insert into top scope
        if let Some(top_scope) = self.scopes.last_mut() {
            top_scope.insert(name.clone(), entry);
        }
        // Insert into flat union
        self.flat.insert(name, entry);
    }

    /// Register a pair binding (discriminant + payload) for register-form enums.
    /// #1154: Constructs a BindingEntry with home=RegPair(reg, payload).
    pub fn insert_pair(&mut self, name: String, reg: RegId, payload: RegId) {
        let entry = BindingEntry {
            home: BindingHome::RegPair(reg, payload),
        };
        // Insert into top scope
        if let Some(top_scope) = self.scopes.last_mut() {
            top_scope.insert(name.clone(), entry);
        }
        // Insert into flat union
        self.flat.insert(name, entry);
    }

    /// Register a closure capture binding at [R14 + offset].
    /// #1233: Constructs a BindingEntry with home=EnvSlot(offset).
    pub fn insert_env(&mut self, name: String, offset: i32) {
        let entry = BindingEntry {
            home: BindingHome::EnvSlot(offset),
        };
        // Insert into top scope
        if let Some(top_scope) = self.scopes.last_mut() {
            top_scope.insert(name.clone(), entry);
        }
        // Insert into flat union
        self.flat.insert(name, entry);
    }

    /// Register a closure binding (fat pointer) in a register.
    /// #995: Constructs a BindingEntry with home=Closure(reg).
    /// The register holds a pointer to a 16-byte [env_ptr:8 | code_ptr:8] fat pair.
    pub fn insert_closure(&mut self, name: String, reg: RegId) {
        let entry = BindingEntry {
            home: BindingHome::Closure(reg),
        };
        // Insert into top scope
        if let Some(top_scope) = self.scopes.last_mut() {
            top_scope.insert(name.clone(), entry);
        }
        // Insert into flat union
        self.flat.insert(name, entry);
    }

    /// Look up a binding's home by walking scopes top-down; if none found, fall back to flat.
    /// #1233: Returns the full BindingHome. Primary lookup for new code paths.
    #[must_use]
    pub fn get_home(&self, name: &str) -> Option<BindingHome> {
        // Walk scopes from top (most recent) to root
        for scope in self.scopes.iter().rev() {
            if let Some(entry) = scope.get(name) {
                return Some(entry.home);
            }
        }
        // Fallback to flat union if stack-walk yields None
        self.flat.get(name).map(|entry| entry.home)
    }

    /// Look up a binding by walking scopes top-down; if none found, fall back to flat.
    /// PA10-005 §3.1: scope walk with flat fallback for post-walk resolve_var_operands.
    /// Returns the primary register (Reg variant only). External API preserved for backward-compat.
    /// Returns None for EnvSlot or RegPair — use get_home() or get_pair() instead.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<RegId> {
        match self.get_home(name) {
            Some(BindingHome::Reg(r)) => Some(r),
            _ => None,
        }
    }

    /// Look up a binding pair (reg, payload) by walking scopes top-down; if none found, fall back to flat.
    /// #1154: Returns (primary_reg, optional_payload_reg). Mirrors the scope walk of get_home().
    #[must_use]
    pub fn get_pair(&self, name: &str) -> Option<(RegId, Option<RegId>)> {
        match self.get_home(name) {
            Some(BindingHome::Reg(r)) => Some((r, None)),
            Some(BindingHome::RegPair(r, p)) => Some((r, Some(p))),
            _ => None,
        }
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
    /// Returns the primary register (reg field from BindingHome) for each binding.
    /// Skips EnvSlot bindings. For RegPair, returns the discriminant only.
    /// #995: Includes Closure bindings (returns the fat-pair register).
    pub fn iter(&self) -> impl Iterator<Item = (&String, RegId)> + '_ {
        self.flat.iter().filter_map(|(name, entry)| match entry.home {
            BindingHome::Reg(r) => Some((name, r)),
            BindingHome::RegPair(r, _) => Some((name, r)),
            BindingHome::EnvSlot(_) => None,
            BindingHome::Closure(r) => Some((name, r)),
        })
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
    /// Skips EnvSlot bindings (they don't consume registers).
    /// #995: Includes Closure bindings (fat pointer addresses held in registers).
    #[must_use]
    pub fn live_regs(&self) -> HashSet<RegId> {
        let mut s = HashSet::new();
        for scope in self.scopes.iter() {
            for e in scope.values() {
                match e.home {
                    BindingHome::Reg(r) => {
                        s.insert(r);
                    }
                    BindingHome::RegPair(r, p) => {
                        s.insert(r);
                        s.insert(p);
                    }
                    BindingHome::EnvSlot(_) => {
                        // Memory-based binding, no register consumed
                    }
                    BindingHome::Closure(r) => {
                        // Closure fat-pair address in register
                        s.insert(r);
                    }
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

    /// #1233: insert_env and get_home for closure capture bindings.
    #[test]
    fn insert_env_and_get_home() {
        let mut table = LocalBindingTable::new();
        let offset = 16i32;

        table.insert_env("cap0".to_string(), offset);
        assert_eq!(table.get_home("cap0"), Some(BindingHome::EnvSlot(offset)));
        // scalar get() returns None for EnvSlot
        assert_eq!(table.get("cap0"), None);
        // get_pair() also returns None for EnvSlot
        assert_eq!(table.get_pair("cap0"), None);
        assert!(table.contains("cap0"));
    }

    /// #1233: EnvSlot not included in iter() (memory-based, not register-based).
    #[test]
    fn iter_excludes_env_slots() {
        let mut table = LocalBindingTable::new();
        table.insert("scalar".to_string(), abi::RAX);
        table.insert_env("capture".to_string(), 8);

        let iter_regs: HashSet<RegId> = table.iter().map(|(_, r)| r).collect();
        assert_eq!(iter_regs, HashSet::from([abi::RAX]));
        // "capture" not in iter results
    }

    /// #1233: EnvSlot not included in live_regs() (doesn't consume register).
    #[test]
    fn live_regs_excludes_env_slots() {
        let mut table = LocalBindingTable::new();
        table.insert("x".to_string(), abi::RAX);
        table.insert_env("cap0".to_string(), 0);
        table.insert_env("cap1".to_string(), 8);

        let live = table.live_regs();
        assert_eq!(live, HashSet::from([abi::RAX]));
        // Neither EnvSlot appears in live_regs
    }

    /// #1233: Closure capture can shadow parameter binding.
    #[test]
    fn env_slot_shadow_with_param() {
        let mut table = LocalBindingTable::new();

        // Root: parameter x → RAX
        table.insert("x".to_string(), abi::RAX);
        assert_eq!(table.get("x"), Some(abi::RAX));

        // Nested (closure body): capture x at [r14 + 0]
        table.push_scope();
        table.insert_env("x".to_string(), 0);

        // Lookup finds EnvSlot (shadows parameter)
        assert_eq!(table.get_home("x"), Some(BindingHome::EnvSlot(0)));
        assert_eq!(table.get("x"), None); // get() returns None for EnvSlot

        // Pop back to root
        table.pop_scope();
        assert_eq!(table.get("x"), Some(abi::RAX)); // Parameter visible again
    }

    /// #1233: get_home returns full BindingHome for all variants.
    #[test]
    fn get_home_full_variant_coverage() {
        let mut table = LocalBindingTable::new();

        table.insert("scalar".to_string(), abi::RAX);
        table.insert_pair("enum_x".to_string(), abi::RCX, abi::RDX);
        table.insert_env("capture".to_string(), 16);

        assert_eq!(table.get_home("scalar"), Some(BindingHome::Reg(abi::RAX)));
        assert_eq!(table.get_home("enum_x"), Some(BindingHome::RegPair(abi::RCX, abi::RDX)));
        assert_eq!(table.get_home("capture"), Some(BindingHome::EnvSlot(16)));
        assert_eq!(table.get_home("unknown"), None);
    }

    /// #995: insert_closure and get_home for closure-binding fat pointers.
    #[test]
    fn insert_closure_and_get_home() {
        let mut table = LocalBindingTable::new();
        let reg = abi::RCX;

        table.insert_closure("f".to_string(), reg);
        assert_eq!(table.get_home("f"), Some(BindingHome::Closure(reg)));
        // scalar get() returns None for Closure
        assert_eq!(table.get("f"), None);
        assert!(table.contains("f"));
    }

    /// #995: Closure not included in get() but appears in iter() and live_regs().
    #[test]
    fn closure_live_regs_and_iter() {
        let mut table = LocalBindingTable::new();

        table.insert("scalar_x".to_string(), abi::RAX);
        table.insert_closure("f".to_string(), abi::RCX);

        // Closure binding in live_regs
        let live = table.live_regs();
        assert!(live.contains(&abi::RAX));
        assert!(live.contains(&abi::RCX));
        assert_eq!(live.len(), 2);

        // Closure binding in iter()
        let iter_regs: HashSet<RegId> = table.iter().map(|(_, r)| r).collect();
        assert_eq!(iter_regs, HashSet::from([abi::RAX, abi::RCX]));
    }

    /// #995: Closure shadow with local binding.
    #[test]
    fn closure_shadow_with_scalar() {
        let mut table = LocalBindingTable::new();

        // Root: scalar x → RAX
        table.insert("x".to_string(), abi::RAX);
        assert_eq!(table.get("x"), Some(abi::RAX));

        // Nested: closure x → RCX (shadow)
        table.push_scope();
        table.insert_closure("x".to_string(), abi::RCX);

        // Lookup finds Closure (shadows scalar)
        assert_eq!(table.get_home("x"), Some(BindingHome::Closure(abi::RCX)));
        assert_eq!(table.get("x"), None); // get() returns None for Closure

        // Pop back to root
        table.pop_scope();
        assert_eq!(table.get("x"), Some(abi::RAX)); // Scalar visible again
    }
}
