//! Capability-set interner.
//!
//! Capability sets are unordered finite sets of [`CapId`]s — no tail
//! variable (capabilities are not row-polymorphic at the type level
//! per `custom-assembler.md` §5). Internally we keep the contents sorted
//! and deduplicated so that equal sets share an interned id regardless
//! of insertion order.
//!
//! Phase-1 uses `Vec<CapId>` instead of `SmallVec<[CapId; 4]>` per the
//! AC text — no `smallvec` workspace dep is added.

use core::num::NonZeroU32;
use std::collections::HashMap;

use crate::types::CapSetId;

/// Interned identifier for a single capability (e.g., `Mmio.read_cap`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct CapId(NonZeroU32);

impl CapId {
    /// Construct a `CapId` from a positive integer. Returns `None` for 0.
    #[must_use]
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// The raw integer value of this id.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl core::fmt::Display for CapId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "c{}", self.0.get())
    }
}

/// An unordered finite set of capability ids.
///
/// `caps` is kept sorted and deduplicated so two sets with the same
/// contents compare equal regardless of construction order.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct CapSet {
    caps: Vec<CapId>,
}

impl CapSet {
    /// Construct an empty set.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Construct from a possibly-unordered list. Sorts + deduplicates.
    #[must_use]
    pub fn from_ids(mut caps: Vec<CapId>) -> Self {
        caps.sort();
        caps.dedup();
        Self { caps }
    }

    /// Borrow the sorted, deduplicated contents.
    #[must_use]
    pub fn as_slice(&self) -> &[CapId] {
        &self.caps
    }

    /// `true` iff the set has no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.caps.is_empty()
    }

    /// `true` iff every cap in `self` also appears in `other`.
    ///
    /// The AC's failing case `is_subset_of(self, other) == false`
    /// triggers the `C1300` diagnostic at the use site — see
    /// [`Self::missing_caps`] for the difference set.
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.caps.iter().all(|c| other.caps.contains(c))
    }

    /// Union of two cap sets.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let mut merged: Vec<CapId> = self
            .caps
            .iter()
            .copied()
            .chain(other.caps.iter().copied())
            .collect();
        merged.sort();
        merged.dedup();
        Self { caps: merged }
    }

    /// Intersection of two cap sets.
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Self {
        let mut shared: Vec<CapId> = self
            .caps
            .iter()
            .copied()
            .filter(|c| other.caps.contains(c))
            .collect();
        shared.dedup();
        Self { caps: shared }
    }

    /// Capabilities present in `required` but absent from `self`.
    ///
    /// Used by the elaborator to construct C1300 diagnostics: when a
    /// callee requires `required` but the caller only holds `self`,
    /// `required.missing_caps(self)` lists exactly the capabilities to
    /// name in the error.
    #[must_use]
    pub fn missing_caps(&self, available: &Self) -> Vec<CapId> {
        self.caps
            .iter()
            .copied()
            .filter(|c| !available.caps.contains(c))
            .collect()
    }
}

/// Hash-consing interner for capability sets.
///
/// Issues stable [`CapSetId`]s. `CapSetId::EMPTY` (0) is pre-seeded.
pub struct CapSetInterner {
    sets: Vec<CapSet>,
    by_value: HashMap<CapSet, CapSetId>,
}

impl CapSetInterner {
    /// Construct an interner with the empty set pre-seeded at
    /// `CapSetId::EMPTY`.
    #[must_use]
    pub fn new() -> Self {
        let mut me = Self {
            sets: Vec::new(),
            by_value: HashMap::new(),
        };
        let empty = CapSet::empty();
        me.sets.push(empty.clone());
        me.by_value.insert(empty, CapSetId::EMPTY);
        me
    }

    /// Intern a cap set, returning its stable id.
    pub fn intern(&mut self, set: CapSet) -> CapSetId {
        if let Some(id) = self.by_value.get(&set) {
            return *id;
        }
        let id = CapSetId(self.sets.len() as u32);
        self.by_value.insert(set.clone(), id);
        self.sets.push(set);
        id
    }

    /// Look up a previously interned cap set.
    #[must_use]
    pub fn get(&self, id: CapSetId) -> &CapSet {
        &self.sets[id.0 as usize]
    }

    /// The canonical empty cap set's id.
    #[must_use]
    pub fn empty(&self) -> CapSetId {
        CapSetId::EMPTY
    }

    /// Number of interned cap sets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// `true` iff only the empty set has been seen.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sets.len() <= 1
    }
}

impl Default for CapSetInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cid(n: u32) -> CapId {
        CapId::new(n).unwrap()
    }

    #[test]
    fn empty_is_canonical_in_interner() {
        let mut interner = CapSetInterner::new();
        assert_eq!(interner.intern(CapSet::empty()), CapSetId::EMPTY);
        assert_eq!(interner.len(), 1);
        assert!(interner.is_empty());
    }

    #[test]
    fn sets_with_same_contents_intern_equally_regardless_of_order() {
        let a = CapSet::from_ids(vec![cid(1), cid(2), cid(3)]);
        let b = CapSet::from_ids(vec![cid(3), cid(1), cid(2)]);
        let c = CapSet::from_ids(vec![cid(2), cid(3), cid(1)]);
        assert_eq!(a, b);
        assert_eq!(b, c);
        let mut interner = CapSetInterner::new();
        assert_eq!(interner.intern(a), interner.intern(b.clone()));
        assert_eq!(interner.intern(b), interner.intern(c));
    }

    #[test]
    fn distinct_sets_get_distinct_ids() {
        let mut interner = CapSetInterner::new();
        let id1 = interner.intern(CapSet::from_ids(vec![cid(1)]));
        let id2 = interner.intern(CapSet::from_ids(vec![cid(2)]));
        assert_ne!(id1, id2);
    }

    #[test]
    fn is_subset_of_ab_in_abc() {
        let ab = CapSet::from_ids(vec![cid(1), cid(2)]);
        let abc = CapSet::from_ids(vec![cid(1), cid(2), cid(3)]);
        assert!(ab.is_subset_of(&abc));
    }

    #[test]
    fn is_subset_of_abc_in_ab_is_false() {
        let ab = CapSet::from_ids(vec![cid(1), cid(2)]);
        let abc = CapSet::from_ids(vec![cid(1), cid(2), cid(3)]);
        assert!(!abc.is_subset_of(&ab));
    }

    #[test]
    fn union_merges_and_dedups() {
        let ab = CapSet::from_ids(vec![cid(1), cid(2)]);
        let bc = CapSet::from_ids(vec![cid(2), cid(3)]);
        let abc = CapSet::from_ids(vec![cid(1), cid(2), cid(3)]);
        assert_eq!(ab.union(&bc), abc);
    }

    #[test]
    fn intersection_preserves_only_common_caps() {
        let ab = CapSet::from_ids(vec![cid(1), cid(2)]);
        let bc = CapSet::from_ids(vec![cid(2), cid(3)]);
        let b = CapSet::from_ids(vec![cid(2)]);
        assert_eq!(ab.intersection(&bc), b);
    }

    #[test]
    fn missing_caps_reports_difference_for_diagnostics() {
        // Use-site requires {1, 2, 3}; we hold {1, 3}. We're missing 2.
        let required = CapSet::from_ids(vec![cid(1), cid(2), cid(3)]);
        let held = CapSet::from_ids(vec![cid(1), cid(3)]);
        assert_eq!(required.missing_caps(&held), vec![cid(2)]);
    }

    #[test]
    fn empty_set_is_subset_of_anything() {
        let any = CapSet::from_ids(vec![cid(1), cid(2)]);
        assert!(CapSet::empty().is_subset_of(&any));
    }

    #[test]
    fn from_ids_dedups_repeated_caps() {
        let s = CapSet::from_ids(vec![cid(1), cid(1), cid(2), cid(1)]);
        assert_eq!(s.as_slice(), &[cid(1), cid(2)]);
    }

    #[test]
    fn cap_id_round_trips() {
        let id = CapId::new(7).unwrap();
        assert_eq!(id.get(), 7);
        assert_eq!(format!("{id}"), "c7");
    }

    #[test]
    fn cap_id_rejects_zero() {
        assert!(CapId::new(0).is_none());
    }
}
