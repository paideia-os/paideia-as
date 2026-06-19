//! Effect row schema and row-polymorphism support.
//!
//! An effect row represents a set of effects (capabilities) with optional row-polymorphism.
//! The schema is: `{fixed_effects | row_variable?}`, encoded as:
//! - `fixed`: a sorted, deduplicated vector of effect ids
//! - `tail`: an optional row variable for row-polymorphic extension
//!
//! **Closure invariant**: A row is closed iff `tail.is_none()`.
//! A closed row contains exactly the effects in `fixed` and no more.
//! An open row contains the effects in `fixed` plus those denoted by its tail variable.
//!
//! **Row-union semantics**:
//! - The union of two rows merges their fixed effect sets (sorted-deduplicated).
//! - If at least one row has no tail (closed), the union's tail is from the open row.
//! - If both rows are open (have tails), the union picks the left row's tail variable
//!   (deterministic but arbitrary); both tails become constrained equal in the unifier.
//! - Unification (`unify.rs`) generates substitution bindings to resolve tail constraints.
//! - See the `unify()` function in `unify.rs` for row unification details.

use core::num::NonZeroU32;

/// Interned identifier for a single effect name (e.g., `io`, `Mmio`).
/// Distinct from the row id; rows hold these.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct EffectId(NonZeroU32);

impl EffectId {
    /// Construct an `EffectId` from a positive integer.
    ///
    /// Returns `None` if `n == 0`.
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// The raw integer value of this id.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// Row variable for row-polymorphism (`!{io | e}`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct RowVarId(NonZeroU32);

impl RowVarId {
    /// Construct a `RowVarId` from a positive integer.
    ///
    /// Returns `None` if `n == 0`.
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// The raw integer value of this id.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// An effect row: a set of effect ids plus an optional row-variable tail.
///
/// `fixed` is kept SORTED and DEDUPLICATED so that two equal-content rows hash to the same
/// value — required for the interner to cons correctly.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct EffectRow {
    /// Fixed effects, sorted and deduplicated.
    pub fixed: Vec<EffectId>,
    /// Optional row-variable tail.
    pub tail: Option<RowVarId>,
}

impl EffectRow {
    /// Construct an empty row.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Construct from an unsorted list of ids; sorts and deduplicates.
    pub fn from_ids(mut ids: Vec<EffectId>, tail: Option<RowVarId>) -> Self {
        ids.sort();
        ids.dedup();
        Self { fixed: ids, tail }
    }

    /// `true` iff the row has no fixed effects AND no tail variable.
    pub fn is_empty(&self) -> bool {
        self.fixed.is_empty() && self.tail.is_none()
    }

    /// `true` iff this row is closed (no row variable in the tail).
    ///
    /// A closed row contains exactly the effects in `fixed` and no more.
    pub fn is_closed(&self) -> bool {
        self.tail.is_none()
    }

    /// `true` if every effect in `self.fixed` is in `other.fixed`.
    ///
    /// Row variables are ignored for the subset check; that's a deliberate
    /// phase-1 simplification (real subtyping with row variables needs
    /// unification).
    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.fixed.iter().all(|e| other.fixed.contains(e))
    }

    /// Union of fixed effects; combines sorted-deduplicated fixed sets.
    ///
    /// Tail handling policy:
    /// - If `self` has a tail, the result's tail is `self`'s (even if `other` also has a tail).
    /// - If `self` has no tail but `other` does, the result's tail is `other`'s.
    /// - If both have no tail, the result has no tail (closed).
    /// - If both have tails, both tails become constrained equal in unification.
    pub fn union(&self, other: &Self) -> Self {
        let mut merged: Vec<EffectId> = self
            .fixed
            .iter()
            .copied()
            .chain(other.fixed.iter().copied())
            .collect();
        merged.sort();
        merged.dedup();
        Self {
            fixed: merged,
            tail: self.tail.or(other.tail),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_row_is_empty() {
        assert!(EffectRow::empty().is_empty());
    }

    #[test]
    fn from_ids_sorts_and_dedups() {
        let e1 = EffectId::new(2).unwrap();
        let e2 = EffectId::new(1).unwrap();
        let e3 = EffectId::new(1).unwrap();

        let row = EffectRow::from_ids(vec![e1, e2, e3], None);

        // Should be sorted: 1, 2
        assert_eq!(row.fixed.len(), 2);
        assert_eq!(row.fixed[0].get(), 1);
        assert_eq!(row.fixed[1].get(), 2);
        assert!(row.tail.is_none());
    }

    #[test]
    fn is_subset_of_io_in_io_ipc() {
        let io = EffectId::new(1).unwrap();
        let ipc = EffectId::new(2).unwrap();

        let subset = EffectRow::from_ids(vec![io], None);
        let superset = EffectRow::from_ids(vec![io, ipc], None);

        assert!(subset.is_subset_of(&superset));
    }

    #[test]
    fn is_not_subset_of_ipc_in_io() {
        let io = EffectId::new(1).unwrap();
        let ipc = EffectId::new(2).unwrap();

        let subset = EffectRow::from_ids(vec![ipc], None);
        let superset = EffectRow::from_ids(vec![io], None);

        assert!(!subset.is_subset_of(&superset));
    }

    #[test]
    fn union_merges_and_sorts() {
        let e1 = EffectId::new(3).unwrap();
        let e2 = EffectId::new(1).unwrap();
        let e3 = EffectId::new(2).unwrap();

        let row1 = EffectRow::from_ids(vec![e1, e2], None);
        let row2 = EffectRow::from_ids(vec![e3], None);

        let union = row1.union(&row2);

        // Should be sorted: 1, 2, 3
        assert_eq!(union.fixed.len(), 3);
        assert_eq!(union.fixed[0].get(), 1);
        assert_eq!(union.fixed[1].get(), 2);
        assert_eq!(union.fixed[2].get(), 3);
    }

    #[test]
    fn is_closed_on_closed_row() {
        let e1 = EffectId::new(1).unwrap();
        let row = EffectRow::from_ids(vec![e1], None);
        assert!(row.is_closed());
    }

    #[test]
    fn is_closed_on_open_row() {
        let e1 = EffectId::new(1).unwrap();
        let r1 = RowVarId::new(1).unwrap();
        let row = EffectRow::from_ids(vec![e1], Some(r1));
        assert!(!row.is_closed());
    }

    #[test]
    fn is_closed_on_empty_row() {
        let row = EffectRow::empty();
        assert!(row.is_closed());
    }

    #[test]
    fn union_closed_with_closed_is_closed() {
        let e1 = EffectId::new(1).unwrap();
        let e2 = EffectId::new(2).unwrap();

        let row1 = EffectRow::from_ids(vec![e1], None);
        let row2 = EffectRow::from_ids(vec![e2], None);

        let union = row1.union(&row2);
        assert!(union.is_closed());
    }

    #[test]
    fn union_open_with_closed_is_open() {
        let e1 = EffectId::new(1).unwrap();
        let e2 = EffectId::new(2).unwrap();
        let r1 = RowVarId::new(1).unwrap();

        let row1 = EffectRow::from_ids(vec![e1], Some(r1));
        let row2 = EffectRow::from_ids(vec![e2], None);

        let union = row1.union(&row2);
        assert!(!union.is_closed());
        assert_eq!(union.tail, Some(r1));
    }

    #[test]
    fn union_closed_with_open_is_open() {
        let e1 = EffectId::new(1).unwrap();
        let e2 = EffectId::new(2).unwrap();
        let r1 = RowVarId::new(1).unwrap();

        let row1 = EffectRow::from_ids(vec![e1], None);
        let row2 = EffectRow::from_ids(vec![e2], Some(r1));

        let union = row1.union(&row2);
        assert!(!union.is_closed());
        assert_eq!(union.tail, Some(r1));
    }

    #[test]
    fn union_open_with_open_picks_left_tail() {
        let e1 = EffectId::new(1).unwrap();
        let e2 = EffectId::new(2).unwrap();
        let r1 = RowVarId::new(1).unwrap();
        let r2 = RowVarId::new(2).unwrap();

        let row1 = EffectRow::from_ids(vec![e1], Some(r1));
        let row2 = EffectRow::from_ids(vec![e2], Some(r2));

        let union = row1.union(&row2);
        assert!(!union.is_closed());
        assert_eq!(union.tail, Some(r1));
        // Both effects should be merged
        assert_eq!(union.fixed.len(), 2);
    }
}
