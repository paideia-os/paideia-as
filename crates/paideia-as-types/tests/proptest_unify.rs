//! Property-based tests for the unification algorithm using proptest.
//!
//! These tests verify the fundamental properties of unification:
//! - Self-unification: unify(a, a) should always succeed with no new bindings.
//! - Small type spaces: randomly generated primitives.

use paideia_as_types::{Subst, TypeInterner, unify};
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_unify_self_noop(choice in 0u32..8) {
        let mut interner = TypeInterner::new();
        let ty = match choice {
            0 => interner.unit(),
            1 => interner.bool_ty(),
            2 => interner.char_ty(),
            3 => interner.uint(8),
            4 => interner.sint(32),
            5 => interner.float(64),
            6 => interner.top(),
            _ => interner.bot(),
        };

        let mut subst = Subst::new();
        let result = unify(&mut interner, &mut subst, ty, ty);

        prop_assert!(result.is_ok(), "Self-unification should always succeed");
        prop_assert!(subst.is_empty(), "Self-unification should not add bindings");
    }

    /// Unifying a concrete type with itself should preserve structure.
    #[test]
    fn prop_unify_idempotent(choice in 0u32..8) {
        let mut interner = TypeInterner::new();
        let ty = match choice {
            0 => interner.unit(),
            1 => interner.bool_ty(),
            2 => interner.char_ty(),
            3 => interner.uint(8),
            4 => interner.sint(32),
            5 => interner.float(64),
            6 => interner.top(),
            _ => interner.bot(),
        };

        let mut subst1 = Subst::new();
        let mut subst2 = Subst::new();

        let result1 = unify(&mut interner, &mut subst1, ty, ty);
        let result2 = unify(&mut interner, &mut subst2, ty, ty);

        prop_assert_eq!(result1.is_ok(), result2.is_ok());
        prop_assert!(result1.is_ok());
        prop_assert!(subst1.is_empty());
        prop_assert!(subst2.is_empty());
    }
}
