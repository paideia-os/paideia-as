//! Robinson unification (with occurs-check) over [`MonoType`].
//!
//! Standard first-order unifier: two monotypes unify iff there exists
//! a most-general substitution that makes them syntactically equal.
//! The result of [`unify`] is that most-general unifier — every other
//! unifier of the same input pair is an instance of it.

use std::fmt;

use crate::subst::Substitution;
use crate::ty::{MonoType, TypeVar};

/// Failure modes for [`unify`].
///
/// [`UnifyError::Mismatch`] covers every structural clash (constant
/// vs. arrow, distinct constants, or an arrow-vs-anything-else
/// disagreement). [`UnifyError::OccursCheck`] fires when unifying a
/// variable with a term that mentions it — the classic
/// infinite-type rejection that makes untyped `λx. x x` illegal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnifyError {
    /// Two monotypes are not unifiable.
    Mismatch {
        /// Left operand as originally passed to [`unify`].
        a: MonoType,
        /// Right operand as originally passed to [`unify`].
        b: MonoType,
    },
    /// A type variable appeared inside the term it was being unified
    /// with. Producing the binding would yield an infinite type.
    OccursCheck {
        /// The offending type variable.
        var: TypeVar,
        /// The term it appeared inside of.
        ty: MonoType,
    },
}

impl fmt::Display for UnifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mismatch { a, b } => {
                write!(f, "type mismatch: cannot unify `{a}` with `{b}`")
            }
            Self::OccursCheck { var, ty } => {
                write!(
                    f,
                    "occurs check failed: variable `{}` occurs in `{ty}`",
                    MonoType::Var(*var)
                )
            }
        }
    }
}

impl std::error::Error for UnifyError {}

/// Compute the most-general unifier of two monotypes.
///
/// Standard structural recursion:
///
/// * `Var(v) ~ Var(v)` — empty substitution.
/// * `Var(v) ~ t`      — occurs-check `t` on `v`, then bind `v ↦ t`.
/// * `t ~ Var(v)`      — recurse the other way.
/// * `Con(a) ~ Con(b)` — empty if names match; [`UnifyError::Mismatch`]
///   otherwise.
/// * `Arrow(a1, a2) ~ Arrow(b1, b2)` — unify domains yielding `s1`,
///   then unify `s1`-applied codomains yielding `s2`, and return
///   `s2 ∘ s1`.
/// * Any other pairing — [`UnifyError::Mismatch`].
///
/// # Errors
///
/// Returns [`UnifyError::Mismatch`] on a structural clash and
/// [`UnifyError::OccursCheck`] when binding a variable would produce
/// an infinite type.
pub fn unify(a: &MonoType, b: &MonoType) -> Result<Substitution, UnifyError> {
    match (a, b) {
        (MonoType::Var(v), MonoType::Var(w)) if v == w => Ok(Substitution::empty()),
        (MonoType::Var(v), t) => bind(*v, t),
        (t, MonoType::Var(v)) => bind(*v, t),
        (MonoType::Con(x), MonoType::Con(y)) if x == y => Ok(Substitution::empty()),
        (MonoType::Arrow(a1, a2), MonoType::Arrow(b1, b2)) => {
            let s1 = unify(a1, b1)?;
            let s2 = unify(&s1.apply(a2), &s1.apply(b2))?;
            Ok(s2.compose(&s1))
        }
        _ => Err(UnifyError::Mismatch {
            a: a.clone(),
            b: b.clone(),
        }),
    }
}

/// Bind `var ↦ ty` after an occurs check.
///
/// Split out from [`unify`] so the `Var(v) ~ t` and `t ~ Var(v)`
/// branches share one implementation.
fn bind(var: TypeVar, ty: &MonoType) -> Result<Substitution, UnifyError> {
    // `Var(v) ~ Var(v)` is caught upstream, so we don't need to
    // re-check the trivial-identity case here.
    if occurs_check(var, ty) {
        return Err(UnifyError::OccursCheck {
            var,
            ty: ty.clone(),
        });
    }
    Ok(Substitution::singleton(var, ty.clone()))
}

/// True if `var` appears anywhere in `ty`.
fn occurs_check(var: TypeVar, ty: &MonoType) -> bool {
    match ty {
        MonoType::Var(v) => *v == var,
        MonoType::Con(_) => false,
        MonoType::Arrow(a, b) => occurs_check(var, a) || occurs_check(var, b),
    }
}
