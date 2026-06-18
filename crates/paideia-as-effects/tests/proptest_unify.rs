use paideia_as_effects::{EffectId, EffectRow, RowVarId, unify};
use proptest::prelude::*;

prop_compose! {
    /// Strategy to generate an arbitrary `EffectId`.
    fn arb_effect_id()(n in 1..=100u32) -> EffectId {
        EffectId::new(n).unwrap()
    }
}

prop_compose! {
    /// Strategy to generate an arbitrary `RowVarId`.
    fn arb_row_var_id()(n in 1..=100u32) -> RowVarId {
        RowVarId::new(n).unwrap()
    }
}

prop_compose! {
    /// Strategy to generate an arbitrary `EffectRow`.
    fn arb_effect_row()(
        ids in prop::collection::vec(arb_effect_id(), 0..5),
        tail in prop::option::of(arb_row_var_id())
    ) -> EffectRow {
        EffectRow::from_ids(ids, tail)
    }
}

proptest! {
    #[test]
    #[cfg_attr(miri, ignore)]
    fn unify_is_commutative(
        a in arb_effect_row(),
        b in arb_effect_row()
    ) {
        // Unification should either succeed on both sides or fail on both sides.
        let result_ab = unify(&a, &b);
        let result_ba = unify(&b, &a);

        match (&result_ab, &result_ba) {
            (Ok(_), Ok(_)) => {
                // Both succeeded; that's the commutative property.
            }
            (Err(_), Err(_)) => {
                // Both failed; that's also commutative.
            }
            _ => {
                // One succeeded and one failed; this violates commutativity.
                panic!(
                    "unify is not commutative:\n  unify({:?}, {:?}) = {:?}\n  unify({:?}, {:?}) = {:?}",
                    a, b, result_ab, b, a, result_ba
                );
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn unify_is_idempotent(a in arb_effect_row()) {
        // Unifying a row with itself should always succeed with an empty substitution.
        let result = unify(&a, &a).unwrap();
        prop_assert!(result.bindings.is_empty());
    }
}
