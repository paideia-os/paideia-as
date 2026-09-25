//! R225.M5: `TypedValue` — a cross-context algebraic object that
//! pairs a record value-row with an effect-row, unified as a single
//! two-sided constraint.
//!
//! # Motivation
//!
//! R225.M2 introduced Rémy-style row polymorphism for records; R225.M3
//! introduced the same machinery for effect rows in a disjoint
//! namespace. The two fell out of the same algebra but never met at
//! a shared point in the type language: an inferred pipeline value
//! carrying both a shape *and* an effect signature had no single
//! monotype it could inhabit. R225.M5 supplies that meeting point.
//!
//! A [`TypedValue`] is a monotype-level record of `(value_row,
//! effect_row)`. It lives inside [`crate::ty::MonoType::Typed`] and
//! unifies component-wise: value rows unify against value rows,
//! effect rows against effect rows, and the results compose into one
//! substitution. Cross-variant attempts — a bare record against a
//! typed value, say — surface as an ordinary
//! [`crate::unify::UnifyError::Mismatch`], mirroring the disjoint-
//! namespace discipline that R225.M3 established between records and
//! effect rows.
//!
//! # Compositional error model
//!
//! When the two sides disagree, both are reported together via
//! [`crate::unify::UnifyError::TypedValueMismatch`] — even if only
//! one side failed. This lets the surface-level diagnostic explain
//! *both* the shape mismatch and the effect mismatch in one pass,
//! rather than fixing the first surfaced error and re-running to
//! discover the second. The variant carries `value_err` and
//! `effect_err` as independent `Option<Box<UnifyError>>`s so a
//! consumer can pattern-match on which sides participated.
//!
//! # Fresh-var discipline
//!
//! The M1 `unify` convention — a bare signature that internally
//! allocates its own `FreshVarGen` — is preserved for
//! [`unify_typed_values`]; the `_with_fresh` twin threads the
//! caller's generator so Rémy-witness row-vars minted here do not
//! collide with the surrounding inference frame. The unifier proper
//! ([`crate::unify::unify_with_fresh`]) dispatches through the
//! `_with_fresh` route.

use crate::effect_row::{unify_effect_rows, EffectRow};
use crate::infer::FreshVarGen;
use crate::subst::Substitution;
use crate::ty::RowType;
use crate::unify::{unify_rows, UnifyError};

/// A two-sided monotype: a (possibly row-polymorphic) record value
/// paired with a (possibly row-polymorphic) effect signature.
///
/// The two rows share a substitution domain — a type variable
/// mentioned in one row can be refined by unification against the
/// other. Neither row is nested inside the other; they compose only
/// through the outer [`TypedValue`] wrapper, and are unified
/// component-wise by [`unify_typed_values`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedValue {
    /// The record-shape side — an ordinary Rémy record row.
    pub value_row: RowType,
    /// The effect signature — a flat set of labels plus an optional
    /// tail row-var, exactly as elsewhere in the crate.
    pub effect_row: EffectRow,
}

impl TypedValue {
    /// The trivial typed value: empty record, empty effect row.
    ///
    /// Useful as a monoidal identity in the pipeline lowerer that
    /// composes typed values across stages.
    pub fn empty() -> Self {
        Self {
            value_row: RowType::Empty,
            effect_row: EffectRow::empty(),
        }
    }

    /// A record-only typed value: the value row is `row`, the effect
    /// row is empty. Models a pure computation with a known shape.
    pub fn of_row(row: RowType) -> Self {
        Self {
            value_row: row,
            effect_row: EffectRow::empty(),
        }
    }

    /// A general typed value: an explicit value row paired with an
    /// explicit effect row. The catch-all constructor when both sides
    /// are known.
    pub fn with_effect(row: RowType, effect: EffectRow) -> Self {
        Self {
            value_row: row,
            effect_row: effect,
        }
    }
}

/// Unify two [`TypedValue`]s. Convenience wrapper that allocates a
/// private [`FreshVarGen`] — see [`unify_typed_values_with_fresh`]
/// for the version that threads the caller's generator.
///
/// # Errors
///
/// Returns [`UnifyError::TypedValueMismatch`] with the failing side(s)
/// populated when either the value row or the effect row (or both)
/// fail to unify. When *only* the value side fails, the effect side
/// is still attempted on the *unsubstituted* effect rows so its
/// diagnostic is meaningful in isolation; when the value side
/// succeeds, the resulting substitution is applied to both effect
/// rows before their unification, so a shared type variable refined
/// by the value pass propagates into the effect pass.
pub fn unify_typed_values(a: &TypedValue, b: &TypedValue) -> Result<Substitution, UnifyError> {
    let mut fresh = FreshVarGen::new();
    unify_typed_values_with_fresh(a, b, &mut fresh)
}

/// Unify two [`TypedValue`]s under a caller-owned [`FreshVarGen`].
///
/// # Algorithm
///
/// 1. Unify the two `value_row`s to obtain `subst_v`.
/// 2. If step 1 succeeded, apply `subst_v` to both effect rows before
///    unifying them; the resulting `subst_e` composes onto
///    `subst_v`.
/// 3. If step 1 failed, still attempt the effect-row unification on
///    the un-refined rows so the returned diagnostic can name both
///    sides that disagree — this trades a tiny loss of precision (a
///    shared var refined by the value pass would have narrowed the
///    effect diagnostic) for a richer one-shot error.
/// 4. Wrap either failure mode in
///    [`UnifyError::TypedValueMismatch`], populating `value_err` /
///    `effect_err` per which side actually failed.
///
/// # Errors
///
/// Returns [`UnifyError::TypedValueMismatch`] on either-side failure;
/// any other unifier error (occurs-check, etc.) surfacing from a row
/// unification is wrapped as the relevant `value_err` or
/// `effect_err`.
pub fn unify_typed_values_with_fresh(
    a: &TypedValue,
    b: &TypedValue,
    fresh: &mut FreshVarGen,
) -> Result<Substitution, UnifyError> {
    match unify_rows(&a.value_row, &b.value_row, fresh) {
        Ok(subst_v) => {
            // Refine both effect rows by the value-row substitution
            // before unifying them, so a shared type variable refined
            // in the value pass carries through.
            let a_eff = subst_v.apply_effect_row(&a.effect_row);
            let b_eff = subst_v.apply_effect_row(&b.effect_row);
            match unify_effect_rows(&a_eff, &b_eff, fresh) {
                Ok(subst_e) => Ok(subst_e.compose(&subst_v)),
                Err(effect_err) => Err(UnifyError::TypedValueMismatch {
                    value_err: None,
                    effect_err: Some(Box::new(effect_err)),
                }),
            }
        }
        Err(value_err) => {
            // The value side failed, but we still try the effect side
            // so the surfaced error can name both mismatches at once.
            // We use the un-refined effect rows because there is no
            // substitution to apply.
            let effect_err = unify_effect_rows(&a.effect_row, &b.effect_row, fresh).err();
            Err(UnifyError::TypedValueMismatch {
                value_err: Some(Box::new(value_err)),
                effect_err: effect_err.map(Box::new),
            })
        }
    }
}
