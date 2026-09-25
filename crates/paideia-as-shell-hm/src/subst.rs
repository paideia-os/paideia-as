//! Substitutions: finite maps from [`TypeVar`] to [`MonoType`].
//!
//! A [`Substitution`] is applied to a monotype, a scheme, or an
//! environment. Composition follows the standard "self after other"
//! convention: `self.compose(&other)` behaves like the function
//! `self ∘ other`, i.e. apply `other` first, then `self`.

use std::collections::HashMap;

use crate::infer::TypeEnv;
use crate::ty::{MonoType, TypeScheme, TypeVar};

/// A finite mapping from type variables to monotypes.
///
/// Substitutions are always idempotent by construction here — the
/// [`Substitution::compose`] operator applies `self` through `other`'s
/// range before merging, and unifiers never bind a variable to a term
/// that mentions it (occurs-check in [`crate::unify::unify`]). We do
/// not re-check idempotence when inserting, since the algorithm never
/// generates a non-idempotent binding on its own; callers building a
/// substitution by hand for tests are responsible for their inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Substitution(HashMap<TypeVar, MonoType>);

impl Substitution {
    /// Empty substitution — the identity element under composition.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Alias for [`Substitution::new`], kept for readability at call
    /// sites that want to be explicit that "no substitution" is the
    /// intent (e.g. the `Var` and `Lit` cases of Algorithm W).
    pub fn empty() -> Self {
        Self::new()
    }

    /// Singleton substitution `{ var ↦ ty }`.
    ///
    /// The caller is responsible for having occurs-checked `var`
    /// against `ty`; the unifier is the only place that mints these
    /// in the M1 code path.
    pub fn singleton(var: TypeVar, ty: MonoType) -> Self {
        let mut m = HashMap::new();
        m.insert(var, ty);
        Self(m)
    }

    /// Insert or overwrite a binding.
    ///
    /// Present as an escape hatch for tests; the algorithm proper uses
    /// [`Substitution::singleton`] plus [`Substitution::compose`].
    pub fn insert(&mut self, var: TypeVar, ty: MonoType) {
        self.0.insert(var, ty);
    }

    /// Look up the mapping for `var`, if any.
    pub fn lookup(&self, var: &TypeVar) -> Option<&MonoType> {
        self.0.get(var)
    }

    /// Apply this substitution to a monotype.
    ///
    /// Variables not in the domain are returned unchanged. Because
    /// substitutions are idempotent by construction, a single pass
    /// suffices — no fixed-point iteration is needed.
    pub fn apply(&self, ty: &MonoType) -> MonoType {
        match ty {
            MonoType::Var(v) => self
                .0
                .get(v)
                .cloned()
                .unwrap_or_else(|| MonoType::Var(*v)),
            MonoType::Con(name) => MonoType::Con(name.clone()),
            MonoType::Arrow(a, b) => MonoType::Arrow(
                Box::new(self.apply(a)),
                Box::new(self.apply(b)),
            ),
        }
    }

    /// Apply this substitution to a type scheme.
    ///
    /// Quantified variables are shadowed: a binding for a quantified
    /// variable does not enter the body's substitution result. This
    /// matches the standard treatment where the outer `∀` re-binds
    /// the name inside `body`.
    pub fn apply_scheme(&self, scheme: &TypeScheme) -> TypeScheme {
        if scheme.quantified.is_empty() {
            return TypeScheme {
                quantified: Vec::new(),
                body: self.apply(&scheme.body),
            };
        }
        // Build a shadowed view without cloning the whole HashMap.
        let mut inner = self.0.clone();
        for q in &scheme.quantified {
            inner.remove(q);
        }
        let shadowed = Substitution(inner);
        TypeScheme {
            quantified: scheme.quantified.clone(),
            body: shadowed.apply(&scheme.body),
        }
    }

    /// Apply this substitution pointwise to every scheme in an
    /// environment.
    pub fn apply_env(&self, env: &TypeEnv) -> TypeEnv {
        env.map_schemes(|s| self.apply_scheme(s))
    }

    /// Compose: `self ∘ other`.
    ///
    /// Semantics: for any `ty`,
    /// `self.compose(&other).apply(&ty)` equals
    /// `self.apply(&other.apply(&ty))`. Concretely we apply `self` to
    /// each of `other`'s range terms, then union in any of `self`'s
    /// bindings whose domain variable is not already produced by
    /// `other` (i.e. `self` wins on key collision at the merged
    /// level, but its bindings do not overwrite `other`'s bindings —
    /// they augment them).
    pub fn compose(&self, other: &Substitution) -> Substitution {
        let mut out: HashMap<TypeVar, MonoType> = HashMap::new();
        // Step 1: apply self to each of other's range terms.
        for (v, ty) in &other.0 {
            out.insert(*v, self.apply(ty));
        }
        // Step 2: fold in self's bindings — self wins on collision.
        for (v, ty) in &self.0 {
            out.insert(*v, ty.clone());
        }
        Substitution(out)
    }
}
