//! Type environment for phase-1 inference.
//!
//! Maintains a scope-stack-based mapping from symbols to types. Symbols in
//! phase-1 use the IR node's span byte_start as a provisional identifier
//! (replaced with proper symbol resolution in PR-37+).

use paideia_as_types::TypeId;
use std::collections::HashMap;

/// Symbol identifier — phase-1 uses the IR's interned mnemonic id as a
/// stand-in. The real symbol table arrives in PR-37+.
pub type Symbol = u32;

/// Mapping from symbols to types, with a scope-stack pattern for
/// block scoping.
///
/// Supports hierarchical scope entry/exit and type lookup following
/// Rust-style shadowing semantics (innermost binding wins).
#[derive(Default, Clone, Debug)]
pub struct TypeEnv {
    scopes: Vec<HashMap<Symbol, TypeId>>,
}

impl TypeEnv {
    /// Construct a new type environment with a single root scope.
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Look up the most recent binding for `sym`, starting from the
    /// innermost scope and working outward.
    ///
    /// Returns `Some(ty)` if a binding exists, `None` if the symbol
    /// is unbound in all scopes.
    pub fn lookup(&self, sym: Symbol) -> Option<TypeId> {
        for scope in self.scopes.iter().rev() {
            if let Some(&t) = scope.get(&sym) {
                return Some(t);
            }
        }
        None
    }

    /// Bind `sym` to type `t` in the innermost scope.
    ///
    /// If `sym` was already bound in this scope, it is shadowed by the
    /// new binding. Outer scopes are unaffected.
    pub fn bind(&mut self, sym: Symbol, t: TypeId) {
        self.scopes
            .last_mut()
            .expect("non-empty scope stack")
            .insert(sym, t);
    }

    /// Push a new empty scope, enabling hierarchical binding.
    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the innermost scope.
    ///
    /// # Panics
    ///
    /// Panics if the scope stack would become empty (root scope must
    /// remain).
    pub fn leave_scope(&mut self) {
        assert!(self.scopes.len() > 1, "cannot leave the root scope");
        self.scopes.pop();
    }

    /// Returns the current depth of the scope stack (root = 1).
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: lookup finds a binding in the innermost scope.
    #[test]
    fn lookup_in_innermost_scope() {
        let mut env = TypeEnv::new();
        let sym = 42;
        let ty = TypeId::new(100).unwrap();

        env.bind(sym, ty);
        assert_eq!(env.lookup(sym), Some(ty));
    }

    /// Test 2: outer-scope bindings are visible from inner scopes.
    #[test]
    fn outer_scope_visible() {
        let mut env = TypeEnv::new();
        let sym_outer = 1;
        let sym_inner = 2;
        let ty_outer = TypeId::new(10).unwrap();
        let ty_inner = TypeId::new(20).unwrap();

        env.bind(sym_outer, ty_outer);
        env.enter_scope();
        env.bind(sym_inner, ty_inner);

        // Both should be visible from the inner scope.
        assert_eq!(env.lookup(sym_outer), Some(ty_outer));
        assert_eq!(env.lookup(sym_inner), Some(ty_inner));
    }

    /// Test 3: enter/leave scope maintains stack invariant.
    #[test]
    fn enter_leave_scope_stack() {
        let mut env = TypeEnv::new();
        assert_eq!(env.depth(), 1);

        env.enter_scope();
        assert_eq!(env.depth(), 2);

        env.enter_scope();
        assert_eq!(env.depth(), 3);

        env.leave_scope();
        assert_eq!(env.depth(), 2);

        env.leave_scope();
        assert_eq!(env.depth(), 1);
    }

    /// Test 4: inner binding shadows outer binding with same symbol.
    #[test]
    fn bind_shadows_outer() {
        let mut env = TypeEnv::new();
        let sym = 5;
        let ty_outer = TypeId::new(50).unwrap();
        let ty_inner = TypeId::new(51).unwrap();

        env.bind(sym, ty_outer);
        assert_eq!(env.lookup(sym), Some(ty_outer));

        env.enter_scope();
        env.bind(sym, ty_inner);
        assert_eq!(env.lookup(sym), Some(ty_inner));

        env.leave_scope();
        assert_eq!(env.lookup(sym), Some(ty_outer));
    }
}
