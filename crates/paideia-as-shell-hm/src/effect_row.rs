//! R225.M3: effect-row polymorphism.
//!
//! Effect rows are the Rémy-style row-polymorphic dual of record rows,
//! carried in the type system as a *distinct* [`MonoType`] variant so
//! records and effects never accidentally unify. Where a record row
//! encodes "at least these fields", an effect row encodes "at most (or
//! at least, in the polymorphic case) these effects" — the shape is
//! structurally the same, but the algebra lives in its own namespace.
//!
//! # Choice of representation
//!
//! R225.M2's [`crate::ty::RowType`] uses a linked-list `Extend/RowVar/
//! Empty` shape because it also has to serialise a canonical field
//! order for pretty printing. Effect rows do not compose through
//! nested `Extend` chains at the surface — every unification step
//! produces a flat set of labels plus a tail — so we pick a
//! [`BTreeMap`] representation from the start. It gives deterministic
//! ordering for free (Display and diagnostics) without an explicit
//! canonicalisation pass, and its `keys()` iterator is already sorted
//! so shared/only-left/only-right partitioning is one linear walk.
//!
//! # Rémy trick, restated for effects
//!
//! When two open rows have distinct extras on each side, we mint a
//! single fresh tail `r` and bind both original tails to
//! `EffectRow { present: <other-side extras>, tail: Some(r) }`. This
//! links the two rows structurally without committing them to either
//! closed shape — a later unification can still grow them together
//! through `r`.
//!
//! # Errors
//!
//! [`crate::unify::UnifyError::EffectRowMismatch`] fires when two
//! closed rows disagree on labels with no absorbing tail on either
//! side (both `tail == None`, both `only` sets non-empty simultaneously,
//! or one closed side missing labels the other requires). The variant
//! carries `missing` (labels the *left* lacks that the right has) and
//! `extra` (labels the left has that the right lacks) — the perspective
//! is always from the *left* operand of [`unify_effect_rows`].

use std::collections::BTreeMap;

use crate::infer::FreshVarGen;
use crate::subst::Substitution;
use crate::ty::{MonoType, TypeVar};
use crate::unify::{self, unify_with_fresh, UnifyError};

/// A flat effect row: a set of labels with monotype payloads plus an
/// optional tail row variable.
///
/// The `present` map's alphabetical key ordering is used for
/// deterministic diagnostics and reproducible Rémy-witness minting
/// order. The `tail`, when [`Some`], stands for "any further effects"
/// and is the anchor point for row-polymorphic unification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectRow {
    /// Labels present in this row, mapped to their monotype payload.
    /// Payload types are usually `Con("Unit")` for pure-tag effects
    /// but the field type is a full [`MonoType`] so parameterised
    /// effects (e.g. `throws e` for some concrete `e`) can inhabit
    /// the same row.
    pub present: BTreeMap<String, MonoType>,
    /// Optional tail row variable — [`Some`] for an open row that can
    /// absorb further labels, [`None`] for a closed row.
    pub tail: Option<TypeVar>,
}

impl EffectRow {
    /// A closed empty effect row `!{}`.
    pub fn empty() -> Self {
        Self {
            present: BTreeMap::new(),
            tail: None,
        }
    }

    /// Build a closed effect row from a list of pure-tag labels — each
    /// label is mapped to `Con("Unit")`. Convenient for test corpora
    /// that just enumerate effect names without caring about payloads.
    pub fn from_labels(names: &[&str]) -> Self {
        let mut present = BTreeMap::new();
        for name in names {
            present.insert((*name).to_owned(), MonoType::Con("Unit".to_owned()));
        }
        Self {
            present,
            tail: None,
        }
    }
}

/// Rémy-style unification for effect rows.
///
/// # Algorithm
///
/// 1. Partition the label sets into (shared, `a`-only, `b`-only).
/// 2. Pointwise-unify shared labels' payload types, accumulating a
///    running substitution.
/// 3. Apply the running substitution to the `only` payload sets before
///    they are used as row-tail bindings — a shared-label unification
///    may have refined type vars that appear in them.
/// 4. Reconcile tails and extras:
///    * `(None, None)` — extras must be empty on both sides, else
///      [`UnifyError::EffectRowMismatch`].
///    * `(Some(av), None)` — `a`'s tail absorbs `b`-only; `a`-only
///      must be empty (b has nowhere for them to go).
///    * `(None, Some(bv))` — mirror image.
///    * `(Some(av), Some(bv))` — Rémy trick: mint fresh `r`, bind
///      `av ↦ EffectRow { present: b_only, tail: Some(r) }` and
///      `bv ↦ EffectRow { present: a_only, tail: Some(r) }`.
///
/// # Errors
///
/// * [`UnifyError::EffectRowMismatch`] for closed-row disagreements.
/// * [`UnifyError::OccursCheck`] if a tail row-var would appear inside
///   a payload it is being bound to hold.
/// * [`UnifyError::Mismatch`] bubbling from a shared-label payload
///   unification failure.
pub fn unify_effect_rows(
    a: &EffectRow,
    b: &EffectRow,
    fresh: &mut FreshVarGen,
) -> Result<Substitution, UnifyError> {
    // Partition. BTreeMap keys are already sorted so `missing` /
    // `extra` diagnostic vectors below come out in deterministic
    // alphabetical order for free.
    let mut shared: Vec<(String, MonoType, MonoType)> = Vec::new();
    let mut a_only: BTreeMap<String, MonoType> = BTreeMap::new();
    let mut b_only: BTreeMap<String, MonoType> = BTreeMap::new();
    for (name, ty) in &a.present {
        match b.present.get(name) {
            Some(bty) => shared.push((name.clone(), ty.clone(), bty.clone())),
            None => {
                a_only.insert(name.clone(), ty.clone());
            }
        }
    }
    for (name, ty) in &b.present {
        if !a.present.contains_key(name) {
            b_only.insert(name.clone(), ty.clone());
        }
    }
    // shared is already in sort order because BTreeMap iteration is.

    // Step 2: unify shared-label payload types under a running subst.
    let mut subst = Substitution::empty();
    for (_name, ta, tb) in &shared {
        let s = unify_with_fresh(&subst.apply(ta), &subst.apply(tb), fresh)?;
        subst = s.compose(&subst);
    }

    // Step 3: refine `only` payload types by the running substitution
    // before they land inside a tail binding.
    let a_only: BTreeMap<String, MonoType> = a_only
        .into_iter()
        .map(|(k, v)| (k, subst.apply(&v)))
        .collect();
    let b_only: BTreeMap<String, MonoType> = b_only
        .into_iter()
        .map(|(k, v)| (k, subst.apply(&v)))
        .collect();

    let a_only_empty = a_only.is_empty();
    let b_only_empty = b_only.is_empty();

    match (a.tail, b.tail) {
        (None, None) => {
            if !a_only_empty || !b_only_empty {
                return Err(row_mismatch(&a_only, &b_only));
            }
            Ok(subst)
        }
        (Some(av), None) => {
            // b is closed; if a has extras, they have nowhere to go.
            if !a_only_empty {
                return Err(row_mismatch(&a_only, &b_only));
            }
            // Bind av to a closed row carrying b's extras.
            for ty in b_only.values() {
                if unify::occurs_check(av, ty) {
                    return Err(UnifyError::OccursCheck {
                        var: av,
                        ty: ty.clone(),
                    });
                }
            }
            let row = EffectRow {
                present: b_only,
                tail: None,
            };
            let s = Substitution::singleton(av, MonoType::EffectRow(row));
            Ok(s.compose(&subst))
        }
        (None, Some(bv)) => {
            if !b_only_empty {
                return Err(row_mismatch(&a_only, &b_only));
            }
            for ty in a_only.values() {
                if unify::occurs_check(bv, ty) {
                    return Err(UnifyError::OccursCheck {
                        var: bv,
                        ty: ty.clone(),
                    });
                }
            }
            let row = EffectRow {
                present: a_only,
                tail: None,
            };
            let s = Substitution::singleton(bv, MonoType::EffectRow(row));
            Ok(s.compose(&subst))
        }
        (Some(av), Some(bv)) => {
            if av == bv {
                // Same tail on both sides: trivially satisfied only if
                // there are no extras. Extras on the same tail would
                // force it into two incompatible shapes at once.
                if a_only_empty && b_only_empty {
                    return Ok(subst);
                }
                return Err(row_mismatch(&a_only, &b_only));
            }
            for ty in b_only.values() {
                if unify::occurs_check(av, ty) {
                    return Err(UnifyError::OccursCheck {
                        var: av,
                        ty: ty.clone(),
                    });
                }
            }
            for ty in a_only.values() {
                if unify::occurs_check(bv, ty) {
                    return Err(UnifyError::OccursCheck {
                        var: bv,
                        ty: ty.clone(),
                    });
                }
            }
            // Rémy witness: a single fresh tail links both sides.
            let r = fresh.fresh();
            let a_ext = EffectRow {
                present: b_only,
                tail: Some(r),
            };
            let b_ext = EffectRow {
                present: a_only,
                tail: Some(r),
            };
            let mut s = Substitution::empty();
            s.insert(av, MonoType::EffectRow(a_ext));
            s.insert(bv, MonoType::EffectRow(b_ext));
            Ok(s.compose(&subst))
        }
    }
}

/// Build the closed-row mismatch diagnostic from the two `only` sets.
///
/// `missing` = labels the *left* row lacks (present on the right).
/// `extra`   = labels the *left* row has that the right lacks.
///
/// Both vectors come out in sorted order because their source maps
/// are [`BTreeMap`]s.
fn row_mismatch(a_only: &BTreeMap<String, MonoType>, b_only: &BTreeMap<String, MonoType>) -> UnifyError {
    let extra: Vec<String> = a_only.keys().cloned().collect();
    let missing: Vec<String> = b_only.keys().cloned().collect();
    UnifyError::EffectRowMismatch { missing, extra }
}

