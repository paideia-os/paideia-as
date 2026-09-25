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

use std::collections::BTreeMap;

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

// ---------------------------------------------------------------------
// R225.M6: pipeline composition of typed values.
//
// A shell pipeline is a left-to-right sequence of stages. The composed
// *value* is the last stage's value (the tail of the pipe is what a
// consumer sees); the composed *effect* is the union of every stage's
// effect signature (a pipeline is at least as effectful as any of its
// stages). This is the R225.M4 pipeline lowerer's typing rule expressed
// at the type-language level, and the entry point the M6 propagator
// will call to fold a chain of stages into a single [`TypedValue`].
//
// The union operation here is *not* the unifier — no substitution is
// produced, no fresh variables are minted. It composes two already-
// well-formed rows into a new one under a fixed policy:
//
//   Presents merge: LEFT wins on collisions. The earliest stage's
//     labelled type is retained. This mirrors the standard fold
//     semantics of "prior + current" and matches the SQL-relational
//     "first non-null" convention. It also captures the pipe-shape
//     intuition: an effect surfaced early in the pipeline keeps its
//     original signature even if a later stage rediscovers it under
//     a different (still-unifiable-in-principle) type.
//
//   Tail policy: LEFT's tail wins if [`Some`]; else RIGHT's; else
//     [`None`]. Preserves the M3 open-row shape when either input
//     is open, and biases towards the earliest stage that opened
//     the row — again, the "prior + current" fold intuition.
//
// The union is *not* commutative under these rules — swapping `a` and
// `b` swaps which side wins collisions and which tail is preserved.
// The pipeline fold applies it left-associatively so the leftmost
// stage's decisions propagate rightward.

/// Merge two effect rows into a union under the R225.M6 pipeline policy.
///
/// # Policy
///
/// * **Presents**: LEFT wins on label collisions — the entry from `a`
///   is kept, `b`'s entry for the same label is discarded. Labels
///   present in only one side pass through unchanged.
/// * **Tail**: `a.tail` if [`Some`]; else `b.tail`; else [`None`].
///
/// # Non-unification
///
/// This function never unifies — it never mints fresh variables and
/// never produces a substitution. Callers that need to *reconcile*
/// two typed values should call [`unify_typed_values`] instead. Union
/// is the correct operation for pipeline composition: a pipeline's
/// effect signature is the *set union* of its stages' signatures, not
/// a fixed-point solved by unification.
///
/// # Determinism
///
/// [`BTreeMap`] iteration is sorted, so the output row's `present`
/// order is stable across runs — the resulting row is byte-for-byte
/// reproducible given identical inputs.
pub fn union_effect_rows(a: &EffectRow, b: &EffectRow) -> EffectRow {
    let mut present: BTreeMap<String, crate::ty::MonoType> = a.present.clone();
    for (name, ty) in &b.present {
        // LEFT wins: only insert from `b` when the label is absent
        // in `a`. `BTreeMap::entry` expresses this without an extra
        // lookup.
        present.entry(name.clone()).or_insert_with(|| ty.clone());
    }
    let tail = a.tail.or(b.tail);
    EffectRow { present, tail }
}

/// Fold [`union_effect_rows`] left-to-right across a slice of rows.
///
/// An empty slice yields [`EffectRow::empty`] — the monoidal identity
/// under this union. A single-element slice returns a clone of that
/// element. Two or more elements are combined pairwise from the left
/// so the earliest stage's collisions and tail dominate.
pub fn compose_pipeline_effects(rows: &[EffectRow]) -> EffectRow {
    let mut acc = EffectRow::empty();
    // Handle the empty case by early return so we do not incur a
    // trivial `union_effect_rows(empty, empty)` call that would be
    // correct but wasteful.
    if rows.is_empty() {
        return acc;
    }
    // Seed the accumulator with the first row so the fold's very
    // first union preserves the LEFT-wins semantics on rows[0].
    acc = rows[0].clone();
    for row in &rows[1..] {
        acc = union_effect_rows(&acc, row);
    }
    acc
}

/// Compose a pipeline of typed values into a single [`TypedValue`].
///
/// # Rule
///
/// * `value_row` = the *last* stage's value row (a pipeline's output
///   is its tail's output), or [`RowType::Empty`] when `stages` is
///   empty.
/// * `effect_row` = [`compose_pipeline_effects`] across every stage,
///   under the LEFT-wins policy documented on [`union_effect_rows`].
///
/// # Edge cases
///
/// * Empty slice → [`TypedValue::empty`] (the monoidal identity).
/// * Single stage → an exact clone of that stage; no rewriting of
///   either half.
pub fn typed_value_pipe(stages: &[TypedValue]) -> TypedValue {
    if stages.is_empty() {
        return TypedValue::empty();
    }
    if stages.len() == 1 {
        return stages[0].clone();
    }
    let effect_rows: Vec<EffectRow> = stages.iter().map(|s| s.effect_row.clone()).collect();
    let effect_row = compose_pipeline_effects(&effect_rows);
    // Safe: length checked above.
    let value_row = stages[stages.len() - 1].value_row.clone();
    TypedValue {
        value_row,
        effect_row,
    }
}
