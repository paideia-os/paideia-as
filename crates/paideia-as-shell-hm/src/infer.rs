//! Algorithm W: the Damas-Milner type inference driver.
//!
//! The three surface operations are:
//!
//! * [`instantiate`] — replace a scheme's quantified vars with fresh
//!   monotype variables. Called at every `Var` reference.
//! * [`generalize`]  — quantify a monotype over exactly the free
//!   variables that are *not* free in the surrounding environment.
//!   Called at every `Let` binding.
//! * [`infer`]       — walk an [`Expr`] and produce a
//!   `(Substitution, MonoType)` pair.
//!
//! The two-substitution shape (each recursive call returns its own
//! substitution which the caller composes onto the running one) is
//! the classical Milner 1978 presentation; the R225.M2+ milestones
//! will extend it with row / effect bookkeeping without reshaping the
//! driver.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::expr::{Expr, Lit};
use crate::subst::Substitution;
use crate::ty::{MonoType, RowType, TypeScheme, TypeVar};
use crate::unify::{self, UnifyError};

/// A term-variable environment — mapping identifiers to type schemes.
///
/// Persistent by construction: [`TypeEnv::extend`] returns a new
/// environment rather than mutating the receiver. That keeps the
/// lexical-scoping story simple; the recursion in [`infer`] never
/// has to unwind a mutation on the way back up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeEnv(HashMap<String, TypeScheme>);

impl TypeEnv {
    /// The empty environment.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Return a new environment that binds `name` to `scheme`,
    /// shadowing any previous binding.
    pub fn extend(&self, name: String, scheme: TypeScheme) -> Self {
        let mut m = self.0.clone();
        m.insert(name, scheme);
        Self(m)
    }

    /// Look up the scheme bound to `name`, if any.
    pub fn lookup(&self, name: &str) -> Option<&TypeScheme> {
        self.0.get(name)
    }

    /// Free type variables of the environment: the union of the free
    /// variables of every scheme in scope.
    pub fn free_vars(&self) -> HashSet<TypeVar> {
        let mut out = HashSet::new();
        for scheme in self.0.values() {
            out.extend(scheme.free_vars());
        }
        out
    }

    /// Map a function over every scheme; used by
    /// [`Substitution::apply_env`].
    pub(crate) fn map_schemes<F>(&self, mut f: F) -> Self
    where
        F: FnMut(&TypeScheme) -> TypeScheme,
    {
        let mut m = HashMap::with_capacity(self.0.len());
        for (k, v) in &self.0 {
            m.insert(k.clone(), f(v));
        }
        Self(m)
    }
}

/// Monotonic counter that mints fresh [`TypeVar`] values.
///
/// A [`FreshVarGen`] is threaded through every recursive call of
/// [`infer`] so a fresh variable never collides with a previous one
/// in the same inference run. Distinct runs may reuse ids — the
/// generator is not meant to be a global registry.
#[derive(Clone, Debug, Default)]
pub struct FreshVarGen {
    next: u32,
}

impl FreshVarGen {
    /// Build a generator starting at id `0`.
    pub fn new() -> Self {
        Self { next: 0 }
    }

    /// Mint a fresh type variable and advance the counter.
    pub fn fresh(&mut self) -> TypeVar {
        let v = TypeVar(self.next);
        self.next = self
            .next
            .checked_add(1)
            .expect("paideia-as-shell-hm: FreshVarGen counter overflowed u32 in one inference run");
        v
    }
}

/// Replace each of a scheme's quantified variables with a fresh
/// monotype variable, and return the resulting monotype.
///
/// This is the "monomorphising" operation performed at every `Var`
/// reference — each use site of a polymorphic identifier gets its
/// own fresh copy of the scheme's body, so that constraints picked
/// up at one use site do not leak into another.
pub fn instantiate(scheme: &TypeScheme, fresh: &mut FreshVarGen) -> MonoType {
    if scheme.quantified.is_empty() {
        return scheme.body.clone();
    }
    let mut renaming = Substitution::empty();
    for q in &scheme.quantified {
        renaming.insert(*q, MonoType::Var(fresh.fresh()));
    }
    renaming.apply(&scheme.body)
}

/// Generalise a monotype into a scheme by quantifying exactly the
/// variables that are free in the monotype but not free in the
/// environment.
///
/// Variables free in the environment are *not* quantifiable, because
/// they are still under active refinement by outer scope; quantifying
/// them would break the invariant that the same variable has the
/// same meaning at every use site in its scope.
pub fn generalize(env: &TypeEnv, ty: &MonoType) -> TypeScheme {
    let env_fv = env.free_vars();
    let mut quantified = Vec::new();
    // Iterate the monotype in a fixed left-to-right order so the
    // resulting scheme's `quantified` list is reproducible across
    // runs. `MonoType::free_vars` returns a `HashSet`, so we recurse
    // over the tree ourselves.
    collect_free_in_order(ty, &env_fv, &mut quantified, &mut HashSet::new());
    TypeScheme {
        quantified,
        body: ty.clone(),
    }
}

fn collect_free_in_order(
    ty: &MonoType,
    env_fv: &HashSet<TypeVar>,
    out: &mut Vec<TypeVar>,
    seen: &mut HashSet<TypeVar>,
) {
    match ty {
        MonoType::Var(v) => {
            if !env_fv.contains(v) && !seen.contains(v) {
                seen.insert(*v);
                out.push(*v);
            }
        }
        MonoType::Con(_) => {}
        MonoType::Arrow(a, b) => {
            collect_free_in_order(a, env_fv, out, seen);
            collect_free_in_order(b, env_fv, out, seen);
        }
        MonoType::Record(row) => {
            collect_free_in_order_row(row, env_fv, out, seen);
        }
        MonoType::EffectRow(row) => {
            // Deterministic left-to-right order: iterate payloads in
            // BTreeMap key order (alphabetic), then the tail row-var
            // if any. Keeps generalisation reproducible when the body
            // contains an effect-row-shaped type.
            for ty in row.present.values() {
                collect_free_in_order(ty, env_fv, out, seen);
            }
            if let Some(v) = row.tail {
                if !env_fv.contains(&v) && !seen.contains(&v) {
                    seen.insert(v);
                    out.push(v);
                }
            }
        }
        MonoType::Typed(tv) => {
            // R225.M5: value-row first (matching Record's walker
            // shape), then the effect-row half in its BTreeMap key
            // order. The union preserves left-to-right determinism
            // for generalisation reproducibility.
            collect_free_in_order_row(&tv.value_row, env_fv, out, seen);
            for ty in tv.effect_row.present.values() {
                collect_free_in_order(ty, env_fv, out, seen);
            }
            if let Some(v) = tv.effect_row.tail {
                if !env_fv.contains(&v) && !seen.contains(&v) {
                    seen.insert(v);
                    out.push(v);
                }
            }
        }
    }
}

fn collect_free_in_order_row(
    row: &RowType,
    env_fv: &HashSet<TypeVar>,
    out: &mut Vec<TypeVar>,
    seen: &mut HashSet<TypeVar>,
) {
    match row {
        RowType::Empty => {}
        RowType::RowVar(v) => {
            if !env_fv.contains(v) && !seen.contains(v) {
                seen.insert(*v);
                out.push(*v);
            }
        }
        RowType::Extend { ty, rest, .. } => {
            collect_free_in_order(ty, env_fv, out, seen);
            collect_free_in_order_row(rest, env_fv, out, seen);
        }
    }
}

/// Failure modes for [`infer`].
///
/// [`InferError::UnboundVar`] fires when a `Var` reference names an
/// identifier that the environment does not bind. [`InferError::UnifyError`]
/// wraps every unifier failure that bubbles up from a sub-expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InferError {
    /// A term variable was referenced with no binding in scope.
    UnboundVar(String),
    /// Unification failed inside a sub-expression.
    UnifyError(UnifyError),
}

impl fmt::Display for InferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnboundVar(name) => write!(f, "unbound variable `{name}`"),
            Self::UnifyError(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for InferError {}

impl From<UnifyError> for InferError {
    fn from(e: UnifyError) -> Self {
        Self::UnifyError(e)
    }
}

/// Algorithm W.
///
/// Returns a substitution refined by inferring `expr` under `env`,
/// paired with the inferred monotype of `expr` (already refined by
/// the returned substitution).
///
/// # Errors
///
/// Returns [`InferError::UnboundVar`] if the expression references a
/// term variable not bound in `env`, or [`InferError::UnifyError`] if
/// any unification along the way fails.
pub fn infer(
    env: &TypeEnv,
    expr: &Expr,
    fresh: &mut FreshVarGen,
) -> Result<(Substitution, MonoType), InferError> {
    match expr {
        // ── Var ────────────────────────────────────────────────
        //
        // Look up the identifier's scheme, instantiate it at fresh
        // variables, and return the monotype with an empty
        // substitution.
        Expr::Var(name) => {
            let scheme = env
                .lookup(name)
                .ok_or_else(|| InferError::UnboundVar(name.clone()))?;
            let ty = instantiate(scheme, fresh);
            Ok((Substitution::empty(), ty))
        }

        // ── Lit ────────────────────────────────────────────────
        //
        // Literals have a ground type; no refinement needed.
        Expr::Lit(Lit::Int) => Ok((Substitution::empty(), MonoType::Con("Int".to_owned()))),
        Expr::Lit(Lit::Str) => Ok((Substitution::empty(), MonoType::Con("Str".to_owned()))),

        // ── Lam ────────────────────────────────────────────────
        //
        // Introduce a fresh variable `a` for the parameter, infer
        // the body under the extended environment, and return
        // `s1.apply(a) -> t1` refined by `s1`.
        Expr::Lam(param, body) => {
            let a = fresh.fresh();
            let scheme = TypeScheme {
                quantified: Vec::new(),
                body: MonoType::Var(a),
            };
            let env1 = env.extend(param.clone(), scheme);
            let (s1, t1) = infer(&env1, body, fresh)?;
            let arrow = MonoType::Arrow(
                Box::new(s1.apply(&MonoType::Var(a))),
                Box::new(t1),
            );
            Ok((s1, arrow))
        }

        // ── App ────────────────────────────────────────────────
        //
        // Infer the function, then infer the argument under the
        // substitution learned from the function. Unify the
        // (substituted) function type with `ta -> r` for a fresh
        // result variable `r`, and return the composed
        // substitutions with the refined result.
        Expr::App(f, arg) => {
            let (s1, tf) = infer(env, f, fresh)?;
            let env1 = s1.apply_env(env);
            let (s2, ta) = infer(&env1, arg, fresh)?;
            let r = fresh.fresh();
            let s3 = unify::unify_with_fresh(
                &s2.apply(&tf),
                &MonoType::Arrow(Box::new(ta), Box::new(MonoType::Var(r))),
                fresh,
            )?;
            let composed = s3.compose(&s2.compose(&s1));
            let result = s3.apply(&MonoType::Var(r));
            Ok((composed, result))
        }

        // ── Let ────────────────────────────────────────────────
        //
        // Infer the bound expression, generalise its type under the
        // (substituted) environment, extend the environment with
        // the resulting scheme, and infer the body.
        Expr::Let(name, val, body) => {
            let (s1, tv) = infer(env, val, fresh)?;
            let env1 = s1.apply_env(env);
            let scheme = generalize(&env1, &tv);
            let env2 = env1.extend(name.clone(), scheme);
            let (s2, tb) = infer(&env2, body, fresh)?;
            let composed = s2.compose(&s1);
            Ok((composed, tb))
        }

        // ── RecordLit ──────────────────────────────────────────
        //
        // Infer each field's expression under the running
        // substitution, apply the substitution to previously-inferred
        // field types as we go (so a later field can constrain an
        // earlier one), then package them into a closed row.
        //
        // Iteration follows sorted field-name order so the running
        // substitution / environment updates are deterministic
        // between runs.
        Expr::RecordLit(fields) => {
            let mut names: Vec<&String> = fields.keys().collect();
            names.sort();

            let mut subst = Substitution::empty();
            let mut env_cur = env.clone();
            let mut typed_fields: HashMap<String, MonoType> = HashMap::new();
            for name in names {
                let e = &fields[name];
                let (s, t) = infer(&env_cur, e, fresh)?;
                subst = s.compose(&subst);
                env_cur = s.apply_env(&env_cur);
                for prev in typed_fields.values_mut() {
                    *prev = s.apply(prev);
                }
                typed_fields.insert(name.clone(), t);
            }
            let row = RowType::from_map(typed_fields, None);
            Ok((subst, MonoType::Record(row)))
        }

        // ── Field ──────────────────────────────────────────────
        //
        // Infer the receiver, then unify its refined type with a
        // record shape `{name: fresh_field | fresh_tail}`. The row
        // variable `fresh_tail` makes the projection accept any
        // record that at least carries `name` (row-polymorphic
        // access), and `fresh_field` names the field's type — the
        // substitution learned from unification refines it into the
        // access's result type.
        Expr::Field(receiver, name) => {
            let (s1, tr) = infer(env, receiver, fresh)?;
            let field_ty = MonoType::Var(fresh.fresh());
            let rest_row = fresh.fresh();
            let expected = MonoType::Record(RowType::Extend {
                field: name.clone(),
                ty: Box::new(field_ty.clone()),
                rest: Box::new(RowType::RowVar(rest_row)),
            });
            let s2 = unify::unify_with_fresh(&s1.apply(&tr), &expected, fresh)?;
            let composed = s2.compose(&s1);
            let result = s2.apply(&field_ty);
            Ok((composed, result))
        }
    }
}
