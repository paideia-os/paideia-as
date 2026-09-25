//! Robinson unification (with occurs-check) over [`MonoType`],
//! extended in R225.M2 with Rémy-style row unification for records.
//!
//! Standard first-order unifier: two monotypes unify iff there exists
//! a most-general substitution that makes them syntactically equal.
//! The result of [`unify`] (and [`unify_with_fresh`]) is that
//! most-general unifier — every other unifier of the same input pair
//! is an instance of it.
//!
//! # API split
//!
//! * [`unify`] — the M1 signature. Non-record cases mint no fresh
//!   variables so the wrapper allocates a private
//!   [`crate::infer::FreshVarGen`] internally; record cases still
//!   work through the same code path but any fresh row-vars minted
//!   inside end up local to the returned substitution (a caller that
//!   later wants to compose them with its own inference state should
//!   use [`unify_with_fresh`] instead).
//! * [`unify_with_fresh`] — the M2 signature that threads the
//!   caller's fresh-var generator so newly-minted row tails do not
//!   collide with vars in the surrounding inference frame. This is
//!   what [`crate::infer::infer`] calls.

use std::collections::HashMap;
use std::fmt;

use crate::infer::FreshVarGen;
use crate::subst::Substitution;
use crate::ty::{MonoType, RowType, TypeVar};

/// Failure modes for [`unify`] / [`unify_with_fresh`].
///
/// [`UnifyError::Mismatch`] covers every structural clash (constant
/// vs. arrow, distinct constants, or an arrow-vs-anything-else
/// disagreement). [`UnifyError::OccursCheck`] fires when unifying a
/// variable with a term that mentions it — the classic
/// infinite-type rejection that makes untyped `λx. x x` illegal.
/// [`UnifyError::MissingField`] and [`UnifyError::RowMismatch`] are
/// R225.M2 additions that report record-shape failures.
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
    /// A field named in one record's row is absent from the other's
    /// closed row (no tail row-var to absorb it).
    MissingField {
        /// The field name that is missing.
        field: String,
        /// The side of the unification the field is missing from:
        /// `"left"` if it is present on the right side but absent
        /// from the left, `"right"` for the mirror image.
        side: String,
    },
    /// A row-level structural failure that is neither a missing field
    /// nor an ordinary type mismatch — e.g. the same row variable
    /// appears on both sides with non-empty extras that would force
    /// it into an infinite shape.
    RowMismatch {
        /// A short human-readable reason for the failure.
        reason: String,
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
            Self::MissingField { field, side } => {
                write!(f, "record field `{field}` missing from {side} side")
            }
            Self::RowMismatch { reason } => {
                write!(f, "row unification failed: {reason}")
            }
        }
    }
}

impl std::error::Error for UnifyError {}

/// Compute the most-general unifier of two monotypes (M1
/// backward-compatible entry point).
///
/// For non-record inputs this behaves exactly as it did in R225.M1:
/// the private [`FreshVarGen`] it allocates internally never
/// advances (no fresh vars are needed for the pure lambda subset),
/// so results are byte-for-byte identical to the M1 implementation.
///
/// For record inputs, the M2 [`unify_rows`] path is taken. Any fresh
/// row-vars it mints do not participate in an enclosing inference
/// frame — a caller inside such a frame should use
/// [`unify_with_fresh`] instead so freshness is shared.
///
/// # Errors
///
/// Returns [`UnifyError::Mismatch`] on a structural clash,
/// [`UnifyError::OccursCheck`] on an infinite-type risk,
/// [`UnifyError::MissingField`] or [`UnifyError::RowMismatch`] on a
/// record-shape failure.
pub fn unify(a: &MonoType, b: &MonoType) -> Result<Substitution, UnifyError> {
    let mut local = FreshVarGen::new();
    unify_with_fresh(a, b, &mut local)
}

/// The M2 unifier that threads the caller's fresh-var generator.
///
/// Behaves identically to [`unify`] on non-record inputs; on record
/// inputs it mints its row-tail witnesses from `fresh` so they can
/// safely compose into the caller's inference state.
///
/// # Errors
///
/// Same set as [`unify`].
pub fn unify_with_fresh(
    a: &MonoType,
    b: &MonoType,
    fresh: &mut FreshVarGen,
) -> Result<Substitution, UnifyError> {
    match (a, b) {
        (MonoType::Var(v), MonoType::Var(w)) if v == w => Ok(Substitution::empty()),
        (MonoType::Var(v), t) => bind(*v, t),
        (t, MonoType::Var(v)) => bind(*v, t),
        (MonoType::Con(x), MonoType::Con(y)) if x == y => Ok(Substitution::empty()),
        (MonoType::Arrow(a1, a2), MonoType::Arrow(b1, b2)) => {
            let s1 = unify_with_fresh(a1, b1, fresh)?;
            let s2 = unify_with_fresh(&s1.apply(a2), &s1.apply(b2), fresh)?;
            Ok(s2.compose(&s1))
        }
        (MonoType::Record(ra), MonoType::Record(rb)) => unify_rows(ra, rb, fresh),
        _ => Err(UnifyError::Mismatch {
            a: a.clone(),
            b: b.clone(),
        }),
    }
}

/// Bind `var ↦ ty` after an occurs check.
///
/// Split out from [`unify_with_fresh`] so the `Var(v) ~ t` and
/// `t ~ Var(v)` branches share one implementation. Handles record
/// terms via [`occurs_check_row`] transparently.
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
///
/// R225.M2: also recurses into [`MonoType::Record`] via
/// [`occurs_check_row`].
fn occurs_check(var: TypeVar, ty: &MonoType) -> bool {
    match ty {
        MonoType::Var(v) => *v == var,
        MonoType::Con(_) => false,
        MonoType::Arrow(a, b) => occurs_check(var, a) || occurs_check(var, b),
        MonoType::Record(row) => occurs_check_row(var, row),
    }
}

/// True if `var` appears anywhere in `row` — either as a field type's
/// free var, or as the row's tail row-var itself.
fn occurs_check_row(var: TypeVar, row: &RowType) -> bool {
    match row {
        RowType::Empty => false,
        RowType::RowVar(v) => *v == var,
        RowType::Extend { ty, rest, .. } => {
            occurs_check(var, ty) || occurs_check_row(var, rest)
        }
    }
}

/// Rémy-style unification of two rows.
///
/// The algorithm:
///
/// 1. Flatten each row to `(fields: HashMap, tail: Option<RowVar>)`.
/// 2. Partition the field sets into shared, `a`-only, and `b`-only.
/// 3. For every shared name, unify the two field types via
///    [`unify_with_fresh`], accumulating a running substitution.
/// 4. Depending on which sides carry tail row-vars:
///    * `None, None` — every extras set must be empty, else
///      [`UnifyError::MissingField`].
///    * `Some(av), None` — bind `av` to `Record({b_only})`; error if
///      `a_only` is non-empty (nowhere for those fields to go).
///    * `None, Some(bv)` — mirror image of the above.
///    * `Some(av), Some(bv)` — mint a fresh row-tail `r`, bind
///      `av ↦ Record({b_only | r})` and `bv ↦ Record({a_only | r})`
///      (the Rémy trick). The two row-vars, once distinct, become
///      structurally linked through `r`.
///
/// The occurs-check for each row-var binding walks the *other* side's
/// extras — an `av` that appears free in `b_only`'s field types would
/// produce an infinite record type.
fn unify_rows(
    a: &RowType,
    b: &RowType,
    fresh: &mut FreshVarGen,
) -> Result<Substitution, UnifyError> {
    let (a_fields, a_tail) = a.to_map();
    let (b_fields, b_tail) = b.to_map();

    // Partition the shared / left-only / right-only sets. Sorted
    // iteration on the shared list keeps unification-order
    // deterministic across runs (important for reproducible test
    // diagnostics on failure).
    let mut shared: Vec<(String, MonoType, MonoType)> = Vec::new();
    let mut a_only: HashMap<String, MonoType> = HashMap::new();
    let mut b_only: HashMap<String, MonoType> = HashMap::new();
    for (name, ty) in &a_fields {
        match b_fields.get(name) {
            Some(bty) => shared.push((name.clone(), ty.clone(), bty.clone())),
            None => {
                a_only.insert(name.clone(), ty.clone());
            }
        }
    }
    for (name, ty) in &b_fields {
        if !a_fields.contains_key(name) {
            b_only.insert(name.clone(), ty.clone());
        }
    }
    shared.sort_by(|x, y| x.0.cmp(&y.0));

    // Step 3: unify shared field types under a running substitution.
    let mut subst = Substitution::empty();
    for (_name, ta, tb) in &shared {
        let s = unify_with_fresh(&subst.apply(ta), &subst.apply(tb), fresh)?;
        subst = s.compose(&subst);
    }

    // Apply the running substitution to a_only / b_only field types
    // before we use them as row-var payloads — the shared-field
    // unification may have refined type-vars appearing in them.
    let a_only: HashMap<String, MonoType> = a_only
        .into_iter()
        .map(|(k, v)| (k, subst.apply(&v)))
        .collect();
    let b_only: HashMap<String, MonoType> = b_only
        .into_iter()
        .map(|(k, v)| (k, subst.apply(&v)))
        .collect();

    let a_only_empty = a_only.is_empty();
    let b_only_empty = b_only.is_empty();

    // Step 4: reconcile tails and extras.
    match (a_tail, b_tail) {
        (None, None) => {
            if !a_only_empty {
                let field = smallest_key(&a_only);
                return Err(UnifyError::MissingField {
                    field,
                    side: "right".to_owned(),
                });
            }
            if !b_only_empty {
                let field = smallest_key(&b_only);
                return Err(UnifyError::MissingField {
                    field,
                    side: "left".to_owned(),
                });
            }
            Ok(subst)
        }
        (Some(av), None) => {
            // `a` has a tail that can absorb `b`'s extras; `b`'s
            // closed row has no room for `a`'s extras.
            if !a_only_empty {
                let field = smallest_key(&a_only);
                return Err(UnifyError::MissingField {
                    field,
                    side: "right".to_owned(),
                });
            }
            for ty in b_only.values() {
                if occurs_check(av, ty) {
                    return Err(UnifyError::OccursCheck {
                        var: av,
                        ty: ty.clone(),
                    });
                }
            }
            let row = RowType::from_map(b_only, None);
            let s = Substitution::singleton(av, MonoType::Record(row));
            Ok(s.compose(&subst))
        }
        (None, Some(bv)) => {
            if !b_only_empty {
                let field = smallest_key(&b_only);
                return Err(UnifyError::MissingField {
                    field,
                    side: "left".to_owned(),
                });
            }
            for ty in a_only.values() {
                if occurs_check(bv, ty) {
                    return Err(UnifyError::OccursCheck {
                        var: bv,
                        ty: ty.clone(),
                    });
                }
            }
            let row = RowType::from_map(a_only, None);
            let s = Substitution::singleton(bv, MonoType::Record(row));
            Ok(s.compose(&subst))
        }
        (Some(av), Some(bv)) => {
            if av == bv {
                // Same row-var on both sides. With no extras this is
                // trivially satisfied; with extras it would force the
                // row-var into two incompatible shapes at once.
                if a_only_empty && b_only_empty {
                    return Ok(subst);
                }
                return Err(UnifyError::RowMismatch {
                    reason: format!(
                        "row variable `{}` appears on both sides with incompatible extras",
                        MonoType::Var(av)
                    ),
                });
            }
            // Rémy trick: a fresh shared tail `r` links the two.
            for ty in b_only.values() {
                if occurs_check(av, ty) {
                    return Err(UnifyError::OccursCheck {
                        var: av,
                        ty: ty.clone(),
                    });
                }
            }
            for ty in a_only.values() {
                if occurs_check(bv, ty) {
                    return Err(UnifyError::OccursCheck {
                        var: bv,
                        ty: ty.clone(),
                    });
                }
            }
            let r = fresh.fresh();
            let a_ext = RowType::from_map(b_only, Some(r));
            let b_ext = RowType::from_map(a_only, Some(r));
            let mut s = Substitution::empty();
            s.insert(av, MonoType::Record(a_ext));
            s.insert(bv, MonoType::Record(b_ext));
            Ok(s.compose(&subst))
        }
    }
}

/// Pick a deterministic representative field-name from a non-empty
/// map — used only in error messages so the surfaced `field` is
/// reproducible across runs. Callers must have verified the map is
/// non-empty.
fn smallest_key(map: &HashMap<String, MonoType>) -> String {
    map.keys().min().cloned().expect("non-empty by caller")
}
