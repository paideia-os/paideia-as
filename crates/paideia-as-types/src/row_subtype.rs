//! Row-based record subtyping for record types with a row-variable tail.
//!
//! Represents record types of the form `{ f1: T1, ..., fn: Tn | ρ }`, where
//! `ρ` is an optional row variable standing for "additional named fields
//! whose identity is not known at this use site". This is the shape needed
//! for AccessKit-style a11y nodes (see `KIND_A11Y_NODE`, threaded in a
//! later milestone): every node carries a `role: Role` and `id: NodeId`,
//! and role-specific extras live in the row tail.
//!
//! ## Subtyping rule
//!
//! `{r1 | ρ1} <: {r2 | ρ2}` iff:
//! 1. `r2 ⊆ r1` — every explicit `(field, type)` pair on the right
//!    appears on the left (width subtyping; field types are invariant
//!    at this layer — depth subtyping is layered above via [`TypeId`]
//!    comparison in the elaborator, out of scope for this module).
//! 2. `ρ1 <: ρ2` — tail row variables must match: both `None`, or both
//!    `Some(v)` with the same `RecordRowVar`. Cross-open/closed cases are
//!    rejected because we cannot decide the identity of the extras.
//!
//! ## Not related to effect rows
//!
//! Effect-row polymorphism (per `paideia-as-ir::EffectRowId` and the
//! sibling `row_poly` module in this crate, which owns its *own*
//! `RowVar`) and record-row polymorphism are theoretically analogous
//! but *deliberately separate* systems. Their row variables live in
//! disjoint namespaces and never unify: this module names its variable
//! [`RecordRowVar`] specifically to keep the two visibly distinct at
//! the call site. Do not thread [`RecordRowVar`] into effect-row code,
//! nor vice-versa, without a design change.

use core::num::NonZeroU32;
use smallvec::SmallVec;

use crate::types::TypeId;

/// Interned identifier for a record field name (e.g. `role`, `id`,
/// `label`). Niche-optimized so `Option<FieldId>` fits in 4 bytes.
///
/// The interner is out of scope for this module: callers pass whatever
/// stable u32 mapping their front end uses. Equality of `FieldId` is
/// the only field-identity predicate used here.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct FieldId(NonZeroU32);

impl FieldId {
    /// Construct a `FieldId` from a positive integer. Returns `None` for 0.
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

impl core::fmt::Display for FieldId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "f{}", self.0.get())
    }
}

/// Row variable standing for "the rest of the record's named fields".
///
/// A `RecordRowVar` is a *record-level* variable, never an effect-row variable
/// — see the module docs. Equality is by identity: two open records are
/// only subtype-comparable when their tail vars are the same instance.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Debug)]
pub struct RecordRowVar(NonZeroU32);

impl RecordRowVar {
    /// Construct a `RecordRowVar` from a positive integer. Returns `None` for 0.
    #[must_use]
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// The raw integer value of this variable.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl core::fmt::Display for RecordRowVar {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ρ{}", self.0.get())
    }
}

/// A record type: explicit fields plus an optional row-variable tail.
///
/// `fields` is kept sorted by [`FieldId`] and duplicate-free after
/// [`RowRecord::new`], so equal records share representation regardless
/// of the order fields were provided. The inline-8 [`SmallVec`] matches
/// the expected fan-out for a11y node schemas without heap allocation.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct RowRecord {
    /// Explicit `(field name, field type)` pairs, sorted by `FieldId`,
    /// deduplicated on the first-write-wins basis of the input order.
    pub fields: SmallVec<[(FieldId, TypeId); 8]>,
    /// Row variable standing for the un-named remainder, or `None` if
    /// the record is closed (no hidden extras).
    pub tail: Option<RecordRowVar>,
}

impl RowRecord {
    /// Construct a canonical record from an arbitrary field list plus a
    /// tail. Fields are sorted by `FieldId`; if the same `FieldId`
    /// appears more than once with different types, the *first*
    /// occurrence is kept — callers should reject conflicts at a higher
    /// layer (this module does not surface diagnostics).
    #[must_use]
    pub fn new(
        fields: impl IntoIterator<Item = (FieldId, TypeId)>,
        tail: Option<RecordRowVar>,
    ) -> Self {
        let mut fs: SmallVec<[(FieldId, TypeId); 8]> = fields.into_iter().collect();
        // Stable sort so equal-FieldId collisions preserve input order,
        // making dedup_by_key's "first wins" deterministic.
        fs.sort_by_key(|(id, _)| *id);
        fs.dedup_by_key(|(id, _)| *id);
        Self { fields: fs, tail }
    }

    /// Construct a closed record (no row-variable tail).
    #[must_use]
    pub fn closed(fields: impl IntoIterator<Item = (FieldId, TypeId)>) -> Self {
        Self::new(fields, None)
    }

    /// Construct an open record with the given row-variable tail.
    #[must_use]
    pub fn open(
        fields: impl IntoIterator<Item = (FieldId, TypeId)>,
        tail: RecordRowVar,
    ) -> Self {
        Self::new(fields, Some(tail))
    }

    /// Number of explicit fields (excludes anything hidden in the tail).
    #[must_use]
    pub fn arity(&self) -> usize {
        self.fields.len()
    }

    /// `true` iff the record has no explicit fields and no tail.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.tail.is_none()
    }

    /// Look up a field by id, returning its interned type if present.
    #[must_use]
    pub fn field_type(&self, id: FieldId) -> Option<TypeId> {
        // fields is sorted; binary search keeps this O(log n).
        self.fields
            .binary_search_by_key(&id, |(fid, _)| *fid)
            .ok()
            .map(|i| self.fields[i].1)
    }
}

/// Return `true` iff `t1 <: t2` under record row subtyping.
///
/// See the module documentation for the subtyping rule. Field types are
/// compared for identity (`TypeId` equality); depth subtyping is not
/// applied here — the elaborator layers it on top when needed.
#[must_use]
pub fn sub_record(t1: &RowRecord, t2: &RowRecord) -> bool {
    // Width subtyping: every explicit field on the right must appear on
    // the left with the same interned type. Both field lists are sorted
    // by FieldId, so a two-pointer merge is linear.
    let mut i = 0;
    for &(rid, rty) in &t2.fields {
        while i < t1.fields.len() && t1.fields[i].0 < rid {
            i += 1;
        }
        if i >= t1.fields.len() || t1.fields[i].0 != rid {
            return false;
        }
        if t1.fields[i].1 != rty {
            return false;
        }
    }

    // Tail rule: ρ1 <: ρ2. Two open records are comparable only when
    // their tail vars match; a closed record is comparable only with a
    // closed record. Cross cases would require deciding the identity of
    // the hidden extras, which is not this layer's job.
    match (t1.tail, t2.tail) {
        (None, None) => true,
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn fid(n: u32) -> FieldId {
        FieldId::new(n).unwrap()
    }

    fn tid(n: u32) -> TypeId {
        TypeId::new(n).unwrap()
    }

    fn rrv(n: u32) -> RecordRowVar {
        RecordRowVar::new(n).unwrap()
    }

    // ------------------------------------------------------------------
    // Unit tests — targeted checks that pin the semantics before we run
    // the property tests.
    // ------------------------------------------------------------------

    #[test]
    fn new_sorts_and_dedups_fields() {
        let r = RowRecord::new(
            vec![(fid(3), tid(30)), (fid(1), tid(10)), (fid(2), tid(20))],
            None,
        );
        assert_eq!(
            r.fields.as_slice(),
            &[(fid(1), tid(10)), (fid(2), tid(20)), (fid(3), tid(30))]
        );
    }

    #[test]
    fn new_dedup_keeps_first_occurrence() {
        // Second (fid(1), tid(99)) is dropped; first wins.
        let r = RowRecord::new(vec![(fid(1), tid(10)), (fid(1), tid(99))], None);
        assert_eq!(r.fields.as_slice(), &[(fid(1), tid(10))]);
    }

    #[test]
    fn closed_supertype_of_wider_closed_record() {
        // {role, id, label} <: {role, id}
        let wide = RowRecord::closed(vec![(fid(1), tid(1)), (fid(2), tid(2)), (fid(3), tid(3))]);
        let narrow = RowRecord::closed(vec![(fid(1), tid(1)), (fid(2), tid(2))]);
        assert!(sub_record(&wide, &narrow));
        assert!(!sub_record(&narrow, &wide));
    }

    #[test]
    fn field_type_mismatch_breaks_subtyping() {
        let a = RowRecord::closed(vec![(fid(1), tid(1))]);
        let b = RowRecord::closed(vec![(fid(1), tid(2))]);
        assert!(!sub_record(&a, &b));
        assert!(!sub_record(&b, &a));
    }

    #[test]
    fn same_open_tail_permits_width_subtyping() {
        let wide = RowRecord::open(vec![(fid(1), tid(1)), (fid(2), tid(2))], rrv(1));
        let narrow = RowRecord::open(vec![(fid(1), tid(1))], rrv(1));
        assert!(sub_record(&wide, &narrow));
    }

    #[test]
    fn distinct_open_tails_are_not_comparable() {
        let a = RowRecord::open(vec![(fid(1), tid(1))], rrv(1));
        let b = RowRecord::open(vec![(fid(1), tid(1))], rrv(2));
        assert!(!sub_record(&a, &b));
        assert!(!sub_record(&b, &a));
    }

    #[test]
    fn open_and_closed_are_not_comparable() {
        let open = RowRecord::open(vec![(fid(1), tid(1))], rrv(1));
        let closed = RowRecord::closed(vec![(fid(1), tid(1))]);
        assert!(!sub_record(&open, &closed));
        assert!(!sub_record(&closed, &open));
    }

    #[test]
    fn field_id_and_row_var_reject_zero() {
        assert!(FieldId::new(0).is_none());
        assert!(RecordRowVar::new(0).is_none());
    }

    #[test]
    fn field_type_lookup_uses_binary_search() {
        let r = RowRecord::closed(vec![
            (fid(1), tid(10)),
            (fid(5), tid(50)),
            (fid(9), tid(90)),
        ]);
        assert_eq!(r.field_type(fid(5)), Some(tid(50)));
        assert_eq!(r.field_type(fid(9)), Some(tid(90)));
        assert_eq!(r.field_type(fid(7)), None);
    }

    // ------------------------------------------------------------------
    // Property tests — reflexivity, transitivity, width monotonicity.
    // ------------------------------------------------------------------

    /// A small alphabet keeps field/type overlap likely, so records
    /// actually collide and the subtyping relation is exercised.
    const FIELD_MAX: u32 = 6;
    const TYPE_MAX: u32 = 4;
    const ROW_MAX: u32 = 3;

    prop_compose! {
        fn arb_field()(id in 1u32..=FIELD_MAX, ty in 1u32..=TYPE_MAX)
            -> (FieldId, TypeId)
        {
            (fid(id), tid(ty))
        }
    }

    prop_compose! {
        fn arb_tail()(t in prop::option::of(1u32..=ROW_MAX)) -> Option<RecordRowVar> {
            t.map(rrv)
        }
    }

    prop_compose! {
        fn arb_record()(
            fields in prop::collection::vec(arb_field(), 0..=6),
            tail in arb_tail(),
        ) -> RowRecord {
            RowRecord::new(fields, tail)
        }
    }

    proptest! {
        #[test]
        fn reflexivity(r in arb_record()) {
            prop_assert!(sub_record(&r, &r));
        }

        #[test]
        fn transitivity(a in arb_record(), b in arb_record(), c in arb_record()) {
            if sub_record(&a, &b) && sub_record(&b, &c) {
                prop_assert!(sub_record(&a, &c));
            }
        }

        /// Width-subtyping monotonicity: dropping a field from the
        /// right-hand side can only broaden the set of supertypes — so
        /// if `a <: b`, then `a` is also a subtype of `b` with any one
        /// of `b`'s fields removed.
        #[test]
        fn width_monotone_on_rhs_drop(a in arb_record(), b in arb_record(), drop_idx in 0usize..6) {
            if sub_record(&a, &b) && !b.fields.is_empty() {
                let idx = drop_idx % b.fields.len();
                let mut narrower_fields: Vec<(FieldId, TypeId)> = b.fields.iter().copied().collect();
                narrower_fields.remove(idx);
                let narrower = RowRecord::new(narrower_fields, b.tail);
                prop_assert!(sub_record(&a, &narrower));
            }
        }

        /// Adding a field to the left-hand side of a subtyping judgment
        /// preserves it, provided the added field id does not conflict
        /// with an existing entry (which would produce a differently
        /// canonicalized record). Width-subtyping monotonicity on the
        /// left.
        #[test]
        fn width_monotone_on_lhs_add(
            a in arb_record(),
            b in arb_record(),
            extra in arb_field(),
        ) {
            if sub_record(&a, &b) && a.field_type(extra.0).is_none() {
                let mut wider_fields: Vec<(FieldId, TypeId)> = a.fields.iter().copied().collect();
                wider_fields.push(extra);
                let wider = RowRecord::new(wider_fields, a.tail);
                prop_assert!(sub_record(&wider, &b));
            }
        }
    }
}
