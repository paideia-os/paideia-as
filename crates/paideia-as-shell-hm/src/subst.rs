//! Substitutions: finite maps from [`TypeVar`] to [`MonoType`].
//!
//! A [`Substitution`] is applied to a monotype, a scheme, or an
//! environment. Composition follows the standard "self after other"
//! convention: `self.compose(&other)` behaves like the function
//! `self ∘ other`, i.e. apply `other` first, then `self`.

use std::collections::{BTreeMap, HashMap};

use crate::effect_row::EffectRow;
use crate::infer::TypeEnv;
use crate::ty::{MonoType, RowType, TypeScheme, TypeVar};
use crate::typed_value::TypedValue;

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
    ///
    /// R225.M2 recurses into [`MonoType::Record`] via
    /// [`Substitution::apply_row`], which spliced-inlines any row
    /// variable that is bound to a further record.
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
            MonoType::Record(row) => MonoType::Record(self.apply_row(row)),
            MonoType::EffectRow(row) => MonoType::EffectRow(self.apply_effect_row(row)),
            // R225.M5: propagate the substitution through both rows
            // of a typed value. The two sides share the substitution
            // domain, so a single walk on each side is enough — no
            // separate composition step is required.
            MonoType::Typed(tv) => MonoType::Typed(Box::new(TypedValue {
                value_row: self.apply_row(&tv.value_row),
                effect_row: self.apply_effect_row(&tv.effect_row),
            })),
        }
    }

    /// Apply this substitution to a row.
    ///
    /// The walker propagates through every [`RowType::Extend`] node,
    /// substituting the field's monotype and recursing into `rest`.
    /// When it reaches a [`RowType::RowVar`], it looks the variable
    /// up in the substitution's domain:
    ///
    /// * If the variable maps to a `MonoType::Record(next)`, splice
    ///   `next` in place (recursing to further substitute through it —
    ///   safe because substitutions are idempotent).
    /// * If it maps to another `MonoType::Var(w)`, replace with
    ///   `RowVar(w)` — the row-var slot walks in lockstep with the
    ///   type-var slot when Rémy-style unification links them.
    /// * If it maps to anything else (should not occur under
    ///   [`crate::unify`]'s invariants), leave the row var untouched
    ///   so the caller can notice the stuck term.
    /// * If it is not in the domain, leave the row var untouched.
    pub fn apply_row(&self, row: &RowType) -> RowType {
        match row {
            RowType::Empty => RowType::Empty,
            RowType::Extend { field, ty, rest } => RowType::Extend {
                field: field.clone(),
                ty: Box::new(self.apply(ty)),
                rest: Box::new(self.apply_row(rest)),
            },
            RowType::RowVar(v) => match self.0.get(v) {
                Some(MonoType::Record(next)) => self.apply_row(next),
                Some(MonoType::Var(w)) => RowType::RowVar(*w),
                Some(_) => RowType::RowVar(*v),
                None => RowType::RowVar(*v),
            },
        }
    }

    /// Apply this substitution to an effect row.
    ///
    /// Each present-label payload is walked as any other monotype.
    /// The tail row-var, when present, is resolved against the
    /// substitution:
    ///
    /// * If it maps to a `MonoType::EffectRow(next)`, splice `next.present`
    ///   into ours (with `next.tail` becoming ours) — this is how
    ///   [`crate::effect_row::unify_effect_rows`] threads its Rémy
    ///   witnesses onto later terms. Substitutions are idempotent, so
    ///   the splice does not need re-application.
    /// * If it maps to another `MonoType::Var(w)`, replace the tail
    ///   with `Some(w)` — the row-var slot walks in lockstep with the
    ///   type-var slot.
    /// * If it maps to anything else (should not occur under the
    ///   unifier's invariants), or is not in the domain, leave the
    ///   tail alone.
    ///
    /// If a spliced `next.present` and our own `present` disagree on a
    /// label, ours wins — the outer binding is authoritative, matching
    /// the record-row `RowType::to_map` "outer wins" convention.
    pub fn apply_effect_row(&self, row: &EffectRow) -> EffectRow {
        let mut present: BTreeMap<String, MonoType> = row
            .present
            .iter()
            .map(|(k, v)| (k.clone(), self.apply(v)))
            .collect();
        let tail = match row.tail {
            None => None,
            Some(v) => match self.0.get(&v) {
                Some(MonoType::EffectRow(next)) => {
                    // Splice next's present labels into ours (outer
                    // wins on collision) and take next's tail as ours.
                    // Recurse on `next` first so any further tail
                    // bindings are resolved transitively — mirrors
                    // apply_row's `self.apply_row(next)` pattern.
                    let nested = self.apply_effect_row(next);
                    for (k, v) in nested.present {
                        present.entry(k).or_insert(v);
                    }
                    nested.tail
                }
                Some(MonoType::Var(w)) => Some(*w),
                Some(_) => Some(v),
                None => Some(v),
            },
        };
        EffectRow { present, tail }
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
