//! Hygienic name resolution per `macros-phase1.md` §3.
//!
//! A [`HygienicEnv`] is a stack of scopes mapping [`HygienicName`]s to
//! some opaque payload (typically a `TypeId` or an `IrNodeId`). Lookup
//! compares the full hygienic name (spelling + tag set) — so a
//! macro-introduced `temp` resolves to its own binding even if the use
//! site also has a binding named `temp`.

use std::collections::HashMap;

use crate::hygiene::HygienicName;

/// Opaque value associated with a binding. Phase-1 callers use a u32
/// id (typically a `TypeId` or `IrNodeId.get()`); the resolver itself
/// is value-type-agnostic.
pub type ResolveValue = u32;

/// Stack of scopes for hygienic name lookup.
#[derive(Default, Debug, Clone)]
pub struct HygienicEnv {
    scopes: Vec<HashMap<HygienicName, ResolveValue>>,
}

impl HygienicEnv {
    /// Construct a fresh environment with one (root) scope.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Push a new scope.
    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the innermost scope. Panics if it would leave the stack
    /// empty.
    pub fn leave_scope(&mut self) {
        assert!(self.scopes.len() > 1, "cannot leave the root scope");
        self.scopes.pop();
    }

    /// Bind `name` to `value` in the innermost scope.
    pub fn bind(&mut self, name: HygienicName, value: ResolveValue) {
        self.scopes
            .last_mut()
            .expect("non-empty scope stack")
            .insert(name, value);
    }

    /// Look up `name`. Searches innermost scope outward.
    #[must_use]
    pub fn lookup(&self, name: &HygienicName) -> Option<ResolveValue> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(*v);
            }
        }
        None
    }

    /// Number of scopes currently on the stack.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hygiene::{HygienicName, MacroId};

    #[test]
    fn bind_and_lookup_unmarked() {
        let mut env = HygienicEnv::new();
        env.bind(HygienicName::unmarked("x"), 42);
        assert_eq!(env.lookup(&HygienicName::unmarked("x")), Some(42));
    }

    #[test]
    fn macro_introduced_temp_does_not_shadow_use_site_temp() {
        let mut env = HygienicEnv::new();
        let tag = MacroId::fresh();
        env.bind(HygienicName::unmarked("temp"), 1); // use-site
        env.bind(HygienicName::unmarked("temp").with_tag(tag), 2); // macro
        // Both bindings are distinct.
        assert_eq!(env.lookup(&HygienicName::unmarked("temp")), Some(1));
        assert_eq!(
            env.lookup(&HygienicName::unmarked("temp").with_tag(tag)),
            Some(2)
        );
    }

    #[test]
    fn lookup_returns_none_for_missing_name() {
        let env = HygienicEnv::new();
        assert!(env.lookup(&HygienicName::unmarked("nope")).is_none());
    }

    #[test]
    fn inner_scope_shadows_outer() {
        let mut env = HygienicEnv::new();
        env.bind(HygienicName::unmarked("x"), 1);
        env.enter_scope();
        env.bind(HygienicName::unmarked("x"), 2);
        assert_eq!(env.lookup(&HygienicName::unmarked("x")), Some(2));
        env.leave_scope();
        assert_eq!(env.lookup(&HygienicName::unmarked("x")), Some(1));
    }

    #[test]
    fn macro_arg_passes_through_to_outer_binding() {
        // §3.2 capture-by-reference: the macro receives `println` from
        // the use site; the use-site environment has `println` bound;
        // resolution succeeds at the use-site span.
        let mut env = HygienicEnv::new();
        env.bind(HygienicName::unmarked("println"), 99);
        // Macro template references the use-site `println` via the
        // metavariable substitution, which retains the unmarked tag.
        let arg = HygienicName::unmarked("println");
        assert_eq!(env.lookup(&arg), Some(99));
    }

    #[test]
    fn macro_template_introduced_println_is_unresolved_if_use_site_missing() {
        // §3.2 inverse: the template's own `println` (with macro tag)
        // does not match the use-site `println` (without). If the use
        // site has no matching binding, lookup fails.
        let env = HygienicEnv::new();
        let tag = MacroId::fresh();
        let template_println = HygienicName::unmarked("println").with_tag(tag);
        assert!(env.lookup(&template_println).is_none());
    }

    // ── Phase-2 (m2-010): reflective hygiene in name resolution ────────

    #[test]
    fn lookup_distinguishes_macro_introduced_temp_from_caller_temp_in_splice() {
        // Scenario: a macro uses `quote { temp }` which splices an identifier
        // marked with the macro's tag. This should resolve separately from
        // a use-site `temp`.
        let mut env = HygienicEnv::new();
        let splice_tag = MacroId::fresh();

        // Use-site binding for `temp`.
        env.bind(HygienicName::unmarked("temp"), 1);

        // Macro-introduced `temp` (from splice with the splice_tag).
        let macro_temp = HygienicName::unmarked("temp").with_tag(splice_tag);
        env.bind(macro_temp.clone(), 2);

        // Both bindings exist and are distinct.
        assert_eq!(env.lookup(&HygienicName::unmarked("temp")), Some(1));
        assert_eq!(env.lookup(&macro_temp), Some(2));
    }
}
