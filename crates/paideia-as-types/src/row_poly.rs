//! Row-polymorphic effect signatures (v0.29-M1-001, issue #1375).
//!
//! Type-side representation of effect rows carrying an optional row variable
//! tail, written `!{E1, E2, .. | ρ}` in surface syntax. This is the
//! **types crate** face of the row story: it is deliberately independent of
//! the runtime interner/unifier in `paideia-as-effects` so that the
//! elaborator can reason about row polymorphism at the type level without
//! pulling the effects crate into the types crate's dependency graph.
//!
//! A row `!{E1, .., En | ρ}` denotes the set `{E1, .., En} ∪ ρ` where `ρ`
//! is an effect-row variable that may be instantiated with any additional
//! set of effects (possibly with a further tail).
//!
//! # Kind
//!
//! Every row inhabits the kind [`KIND_NAME`] (`"effect_row"`). This is
//! distinct from `Star` — a row is a set-of-effects, not a value type.
//!
//! # Coordination with `paideia-as-effects`
//!
//! The effects crate exports its own `EffectRow` (with a `Vec` backing and
//! interner IDs). This module intentionally mirrors the *shape* of that
//! representation while remaining self-contained: local `EffectId` and
//! `RowVar` newtypes over `NonZeroU32`, `SmallVec<[EffectId; 4]>` backing
//! for the fixed prefix (rows are typically small — 0..4 effects), and a
//! pure unification routine free of interner state. Wire-up between the
//! two representations lands in v0.29-M1-004.

use core::num::NonZeroU32;
use smallvec::SmallVec;

/// The kind name for effect rows in the type lattice.
///
/// Rows inhabit the kind `effect_row`, distinct from `Star` (the kind of
/// value types) and from `Arrow` (the kind of type constructors). Held as
/// a public constant so downstream diagnostics and pretty-printers all
/// agree on the spelling; the wire-up into the `kinds::Kind` enum lands
/// in v0.29-M1-004.
#[allow(dead_code)]
pub const KIND_NAME: &str = "effect_row";

/// Interned identifier for a single effect name (e.g. `io`, `Mmio`).
///
/// A local mirror of `paideia_as_effects::EffectId`. Kept separate so the
/// types crate does not depend on the effects crate; the wire-up in
/// v0.29-M1-004 supplies a converter between the two id spaces.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct EffectId(NonZeroU32);

impl EffectId {
    /// Construct an [`EffectId`] from a positive integer, returning `None`
    /// for zero.
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// Raw integer value of this id.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// Row-variable identifier (`ρ` in `!{E1, E2 | ρ}`).
///
/// A row variable stands for "any additional row of effects." Unification
/// binds a row variable to the difference between the two rows being
/// unified when their fixed prefixes are compatible.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct RowVar(NonZeroU32);

impl RowVar {
    /// Construct a [`RowVar`] from a positive integer, returning `None`
    /// for zero.
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// Raw integer value of this row variable.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// An effect row: a sorted, deduplicated fixed prefix plus an optional
/// row-variable tail.
///
/// - `fixed` is kept **sorted and deduplicated** so that two equal-content
///   rows compare equal and hash identically.
/// - `tail` is `None` for a **closed** row and `Some(ρ)` for an **open**
///   (row-polymorphic) row.
///
/// The `SmallVec<[EffectId; 4]>` backing reflects the empirical
/// distribution: most function signatures name zero to four effects, so
/// the fixed prefix stays inline and never allocates.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct EffectRow {
    /// Fixed effects, sorted and deduplicated.
    pub fixed: SmallVec<[EffectId; 4]>,
    /// Optional row-variable tail. `None` iff the row is closed.
    pub tail: Option<RowVar>,
}

impl EffectRow {
    /// The empty closed row `!{}`.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Construct from an arbitrary iterator of ids plus an optional tail.
    ///
    /// The input is sorted and deduplicated before being stored, so the
    /// resulting row satisfies the fixed-prefix invariant.
    pub fn from_ids<I>(ids: I, tail: Option<RowVar>) -> Self
    where
        I: IntoIterator<Item = EffectId>,
    {
        let mut fixed: SmallVec<[EffectId; 4]> = ids.into_iter().collect();
        fixed.sort();
        fixed.dedup();
        Self { fixed, tail }
    }

    /// `true` iff this row has neither fixed effects nor a tail.
    pub fn is_empty(&self) -> bool {
        self.fixed.is_empty() && self.tail.is_none()
    }

    /// `true` iff this row is **closed** (has no row-variable tail).
    pub fn is_closed(&self) -> bool {
        self.tail.is_none()
    }

    /// `true` iff this row is a **bare tail variable** — no fixed effects,
    /// only a row-variable tail (`!{ | ρ}`).
    ///
    /// A bare tail variable unifies with any row by binding its variable
    /// to that row.
    pub fn is_bare_tail(&self) -> bool {
        self.fixed.is_empty() && self.tail.is_some()
    }

    /// `true` iff every effect in `self.fixed` also appears in
    /// `other.fixed`. Tails are ignored.
    pub fn fixed_is_subset_of(&self, other: &Self) -> bool {
        // Both are sorted; we could do a merge-walk in O(n+m), but O(n·m)
        // over rows with at most a handful of elements is not the
        // bottleneck. Keep the code simple.
        self.fixed.iter().all(|e| other.fixed.contains(e))
    }

    /// The union of two rows: the union of their fixed sets, with a tail
    /// following the standard `Option::or` policy (left-wins if both).
    ///
    /// This mirrors the effects-crate policy so the two representations
    /// stay algebraically compatible. `Option::or` is associative and
    /// idempotent, so the resulting `union` is idempotent
    /// (`r ∪ r = r`) and associative (`(a ∪ b) ∪ c = a ∪ (b ∪ c)`);
    /// both are asserted by proptest below.
    pub fn union(&self, other: &Self) -> Self {
        let mut merged: SmallVec<[EffectId; 4]> = SmallVec::new();
        merged.extend(self.fixed.iter().copied());
        merged.extend(other.fixed.iter().copied());
        merged.sort();
        merged.dedup();
        Self {
            fixed: merged,
            tail: self.tail.or(other.tail),
        }
    }
}

/// Substitution produced by [`unify_rows`]: a binding of a row variable
/// to the row it stands for.
///
/// The unifier emits **at most one** binding per call: either the two
/// rows unify without introducing a substitution, or one side is a bare
/// tail variable whose binding closes the gap. Handler composition
/// (b2-08) will thread these bindings through a substitution environment.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct RowSubst {
    /// The row variable being bound.
    pub var: RowVar,
    /// The row it is bound to.
    pub row: EffectRow,
}

/// Result of a successful row-unification: an optional substitution.
///
/// - `None` — the two rows were already syntactically equal (same fixed
///   set and same tail).
/// - `Some(subst)` — one row was a bare tail variable and the unifier
///   bound it to the other row.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct RowUnification {
    /// The substitution required for the rows to unify, if any.
    pub subst: Option<RowSubst>,
}

/// Reasons two rows fail to unify under the wave-0 rule set.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, thiserror::Error)]
pub enum RowUnifyError {
    /// Both rows are closed but their fixed sets differ.
    #[error("row unification: fixed effect sets differ and no tail can absorb them")]
    FixedMismatch,
    /// Both rows carry distinct row-variable tails with distinct fixed
    /// prefixes; wave-0 does not introduce fresh row variables to reconcile
    /// them.
    #[error("row unification: incompatible tail variables")]
    TailMismatch,
}

/// Unify two effect rows.
///
/// Wave-0 rule set (per issue #1375):
///
/// - **(a) Syntactic equality.** Two rows unify with no substitution iff
///   their fixed sets are equal *and* their tails match (either both
///   `None`, or the same `RowVar`).
/// - **(b) Bare-tail absorption.** If one row is a bare tail variable
///   `!{ | ρ}` (empty fixed set + `Some(tail)`), and the other row's
///   fixed set is any set (which trivially contains the bare row's
///   empty fixed set as a subset), then `ρ` is bound to the other row.
///
/// Any other configuration is rejected. In particular, this wave does
/// **not** synthesise fresh row variables to reconcile
/// `!{E1 | ρ}` with `!{E1, E2 | σ}`; that lands with handler composition
/// wiring in v0.29-M1-004.
pub fn unify_rows(a: &EffectRow, b: &EffectRow) -> Result<RowUnification, RowUnifyError> {
    // (a) Syntactic equality — same fixed set, same tail.
    if a.fixed == b.fixed && a.tail == b.tail {
        return Ok(RowUnification { subst: None });
    }

    // (b) Bare tail variable on either side absorbs the other row.
    if a.is_bare_tail() && b.fixed_is_subset_of(b) {
        // `b.fixed_is_subset_of(b)` is trivially true; the guard is that
        // `a` is a bare tail, and by definition `a.fixed = ∅ ⊆ b.fixed`.
        let tail_a = a.tail.expect("is_bare_tail ⇒ tail is Some");
        return Ok(RowUnification {
            subst: Some(RowSubst {
                var: tail_a,
                row: b.clone(),
            }),
        });
    }
    if b.is_bare_tail() {
        let tail_b = b.tail.expect("is_bare_tail ⇒ tail is Some");
        return Ok(RowUnification {
            subst: Some(RowSubst {
                var: tail_b,
                row: a.clone(),
            }),
        });
    }

    // Diagnose the failure with the more informative variant.
    match (a.tail, b.tail) {
        (Some(_), Some(_)) => Err(RowUnifyError::TailMismatch),
        _ => Err(RowUnifyError::FixedMismatch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ---- Deterministic unit tests ------------------------------------

    fn eid(n: u32) -> EffectId {
        EffectId::new(n).unwrap()
    }

    fn rv(n: u32) -> RowVar {
        RowVar::new(n).unwrap()
    }

    #[test]
    fn empty_row_is_empty_and_closed() {
        let r = EffectRow::empty();
        assert!(r.is_empty());
        assert!(r.is_closed());
        assert!(!r.is_bare_tail());
    }

    #[test]
    fn from_ids_sorts_and_dedups() {
        let r = EffectRow::from_ids([eid(3), eid(1), eid(3), eid(2)], None);
        assert_eq!(r.fixed.as_slice(), &[eid(1), eid(2), eid(3)]);
    }

    #[test]
    fn bare_tail_detection() {
        let bare = EffectRow::from_ids([], Some(rv(1)));
        assert!(bare.is_bare_tail());
        assert!(!bare.is_closed());
    }

    #[test]
    fn union_left_tail_wins() {
        let a = EffectRow::from_ids([eid(1)], Some(rv(1)));
        let b = EffectRow::from_ids([eid(2)], Some(rv(2)));
        let u = a.union(&b);
        assert_eq!(u.fixed.as_slice(), &[eid(1), eid(2)]);
        assert_eq!(u.tail, Some(rv(1)));
    }

    #[test]
    fn unify_equal_closed_rows_no_subst() {
        let a = EffectRow::from_ids([eid(1), eid(2)], None);
        let b = EffectRow::from_ids([eid(2), eid(1)], None);
        let u = unify_rows(&a, &b).expect("equal rows unify");
        assert!(u.subst.is_none());
    }

    #[test]
    fn unify_equal_open_rows_no_subst() {
        let a = EffectRow::from_ids([eid(1)], Some(rv(3)));
        let b = EffectRow::from_ids([eid(1)], Some(rv(3)));
        let u = unify_rows(&a, &b).expect("equal open rows unify");
        assert!(u.subst.is_none());
    }

    #[test]
    fn unify_bare_tail_with_closed_binds_var() {
        let a = EffectRow::from_ids([], Some(rv(7)));
        let b = EffectRow::from_ids([eid(1), eid(2)], None);
        let u = unify_rows(&a, &b).expect("bare tail absorbs closed row");
        let s = u.subst.expect("substitution emitted");
        assert_eq!(s.var, rv(7));
        assert_eq!(s.row, b);
    }

    #[test]
    fn unify_closed_with_bare_tail_binds_var() {
        let a = EffectRow::from_ids([eid(1), eid(2)], None);
        let b = EffectRow::from_ids([], Some(rv(9)));
        let u = unify_rows(&a, &b).expect("closed absorbs bare tail on rhs");
        let s = u.subst.expect("substitution emitted");
        assert_eq!(s.var, rv(9));
        assert_eq!(s.row, a);
    }

    #[test]
    fn unify_two_closed_distinct_fails() {
        let a = EffectRow::from_ids([eid(1)], None);
        let b = EffectRow::from_ids([eid(2)], None);
        let err = unify_rows(&a, &b).unwrap_err();
        assert_eq!(err, RowUnifyError::FixedMismatch);
    }

    #[test]
    fn unify_two_open_distinct_fails() {
        // Non-bare tails with different fixed sets: wave-0 rejects.
        let a = EffectRow::from_ids([eid(1)], Some(rv(1)));
        let b = EffectRow::from_ids([eid(2)], Some(rv(2)));
        let err = unify_rows(&a, &b).unwrap_err();
        assert_eq!(err, RowUnifyError::TailMismatch);
    }

    // ---- Proptest suite ----------------------------------------------

    fn any_effect_id() -> impl Strategy<Value = EffectId> {
        (1u32..16).prop_map(|n| EffectId::new(n).unwrap())
    }

    fn any_row_var() -> impl Strategy<Value = RowVar> {
        (1u32..8).prop_map(|n| RowVar::new(n).unwrap())
    }

    fn any_row() -> impl Strategy<Value = EffectRow> {
        (
            prop::collection::vec(any_effect_id(), 0..6),
            prop::option::of(any_row_var()),
        )
            .prop_map(|(ids, tail)| EffectRow::from_ids(ids, tail))
    }

    proptest! {
        /// `r ∪ r = r` for every row `r`.
        #[test]
        fn union_is_idempotent(r in any_row()) {
            let doubled = r.union(&r);
            prop_assert_eq!(doubled, r);
        }

        /// `(a ∪ b) ∪ c = a ∪ (b ∪ c)`.
        ///
        /// Both fixed and tail policies (set union and `Option::or`) are
        /// associative, so the whole row is.
        #[test]
        fn union_is_associative(
            a in any_row(),
            b in any_row(),
            c in any_row(),
        ) {
            let left = a.union(&b).union(&c);
            let right = a.union(&b.union(&c));
            prop_assert_eq!(left, right);
        }

        /// When both sides share the same fixed set *and* the same tail
        /// variable, `unify_rows` succeeds with no substitution — the
        /// tail-elimination rule.
        #[test]
        fn tail_elimination_same_fixed_same_tail(
            ids in prop::collection::vec(any_effect_id(), 0..5),
            tv in any_row_var(),
        ) {
            let a = EffectRow::from_ids(ids.iter().copied(), Some(tv));
            let b = EffectRow::from_ids(ids.into_iter(), Some(tv));
            let u = unify_rows(&a, &b).expect("equal open rows unify");
            prop_assert!(u.subst.is_none());
        }

        /// A bare tail variable unifies with any row by binding.
        #[test]
        fn bare_tail_unifies_with_any(
            other in any_row(),
            tv in any_row_var(),
        ) {
            let bare = EffectRow::from_ids([], Some(tv));
            let u = unify_rows(&bare, &other).expect("bare tail absorbs any row");
            if bare == other {
                prop_assert!(u.subst.is_none());
            } else {
                let s = u.subst.expect("substitution emitted");
                prop_assert_eq!(s.var, tv);
                prop_assert_eq!(s.row, other);
            }
        }
    }
}
