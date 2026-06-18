//! Linearity tracking context.
//!
//! Maintains a scope-stack-based tracking of symbol use-counts and their
//! substructural lattice classes. Used by the linearity checker to validate
//! that binding constraints are satisfied at scope exit.

use std::collections::HashMap;

use paideia_as_diagnostics::Span;
use paideia_as_ir::LinClass;

use crate::env::Symbol;

/// Records the linearity class and use-count of a single binding.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    /// The substructural lattice class of this binding.
    pub class: LinClass,
    /// The number of times this binding has been used.
    pub uses: u32,
    /// The source span where this binding was introduced.
    pub bind_span: Span,
}

/// Tracks use-counts and linearity classes across a hierarchical scope stack.
///
/// Each scope is represented as a `HashMap<Symbol, Binding>`. The context
/// supports entering and leaving scopes, binding new symbols, and recording uses.
/// Lookup follows Rust-style shadowing semantics (innermost scope wins).
#[derive(Default, Debug, Clone)]
pub struct LinearityCtx {
    scopes: Vec<HashMap<Symbol, Binding>>,
}

impl LinearityCtx {
    /// Construct a new linearity context with a single root scope.
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Push a new empty scope, enabling hierarchical binding.
    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the innermost scope and return its bindings.
    ///
    /// # Panics
    ///
    /// Panics if the scope stack would become empty (root scope must remain).
    pub fn leave_scope(&mut self) -> HashMap<Symbol, Binding> {
        assert!(self.scopes.len() > 1, "cannot leave the root scope");
        self.scopes.pop().expect("scope stack non-empty")
    }

    /// Introduce a binding in the innermost scope with use-count 0.
    ///
    /// If `sym` was already bound in this scope, it is shadowed by the new binding.
    pub fn bind(&mut self, sym: Symbol, class: LinClass, bind_span: Span) {
        self.scopes
            .last_mut()
            .expect("scope stack non-empty")
            .insert(
                sym,
                Binding {
                    class,
                    uses: 0,
                    bind_span,
                },
            );
    }

    /// Record a use of `sym`, incrementing the use-count in its binding scope.
    ///
    /// Looks up the binding starting from the innermost scope and working outward.
    /// Returns `true` if the binding was found, `false` if the symbol is unbound in
    /// all scopes (callers may emit an "unbound" diagnostic on `false`, but in
    /// practice the type checker has already done so).
    pub fn use_(&mut self, sym: Symbol) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(b) = scope.get_mut(&sym) {
                b.uses = b.uses.saturating_add(1);
                return true;
            }
        }
        false
    }

    /// Returns the current depth of the scope stack (root = 1).
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Borrow the innermost scope's bindings.
    pub fn innermost(&self) -> &HashMap<Symbol, Binding> {
        self.scopes.last().expect("non-empty scope stack")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    #[test]
    fn new_has_root_scope() {
        let ctx = LinearityCtx::new();
        assert_eq!(ctx.depth(), 1);
    }

    #[test]
    fn enter_scope_increases_depth() {
        let mut ctx = LinearityCtx::new();
        assert_eq!(ctx.depth(), 1);
        ctx.enter_scope();
        assert_eq!(ctx.depth(), 2);
        ctx.enter_scope();
        assert_eq!(ctx.depth(), 3);
    }

    #[test]
    fn bind_in_innermost_scope() {
        let mut ctx = LinearityCtx::new();
        let sym = 42;
        ctx.bind(sym, LinClass::Linear, span(100));

        let binding = ctx.innermost().get(&sym);
        assert!(binding.is_some());
        assert_eq!(binding.unwrap().class, LinClass::Linear);
        assert_eq!(binding.unwrap().uses, 0);
    }

    #[test]
    fn use_increments_count() {
        let mut ctx = LinearityCtx::new();
        let sym = 42;
        ctx.bind(sym, LinClass::Linear, span(100));

        assert!(ctx.use_(sym));
        assert_eq!(ctx.innermost().get(&sym).unwrap().uses, 1);

        assert!(ctx.use_(sym));
        assert_eq!(ctx.innermost().get(&sym).unwrap().uses, 2);
    }

    #[test]
    fn use_returns_false_for_unbound() {
        let mut ctx = LinearityCtx::new();
        assert!(!ctx.use_(999));
    }

    #[test]
    fn nested_scope_use_visible() {
        let mut ctx = LinearityCtx::new();
        let sym = 10;
        ctx.bind(sym, LinClass::Linear, span(10));

        ctx.enter_scope();
        assert!(ctx.use_(sym)); // binding in outer scope is visible
        assert_eq!(ctx.innermost().get(&sym), None); // but not in inner scope
        assert_eq!(ctx.depth(), 2);

        let inner = ctx.leave_scope();
        assert_eq!(inner.len(), 0); // inner scope had no bindings
        assert_eq!(ctx.innermost().get(&sym).unwrap().uses, 1); // outer scope's use-count incremented
    }

    #[test]
    fn shadowing_in_inner_scope() {
        let mut ctx = LinearityCtx::new();
        let sym = 10;
        let ty1 = LinClass::Linear;
        let ty2 = LinClass::Affine;

        ctx.bind(sym, ty1, span(10));
        ctx.enter_scope();
        ctx.bind(sym, ty2, span(20)); // shadow the outer binding

        let inner_binding = ctx.innermost().get(&sym).unwrap();
        assert_eq!(inner_binding.class, ty2);

        let _inner = ctx.leave_scope();
        let outer_binding = ctx.innermost().get(&sym).unwrap();
        assert_eq!(outer_binding.class, ty1);
    }

    #[test]
    #[should_panic(expected = "cannot leave the root scope")]
    fn scope_stack_underflow_panics_on_leave_alone() {
        let mut ctx = LinearityCtx::new();
        ctx.leave_scope(); // only one scope; should panic
    }
}
