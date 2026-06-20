//! Infer higher-rank kinds for generic parameters and type constructors.
//!
//! Phase-4 m9-002: Every declared generic parameter without an explicit kind
//! annotation gets `HrKind::Star`. Higher kinds (`* -> *`) emerge only via
//! `Type::Var` when explicitly constructed via type constructors — m9-006
//! monomorphisation pass.

use crate::kinds::HrKind;

/// Infer the kind for a generic parameter declaration.
///
/// Phase-4-m9-002 minimum: every generic param without an explicit kind
/// annotation gets `HrKind::Star`. Bounds don't change the kind; they
/// constrain which types fit the Star kind.
///
/// Future m9-006 extends this to infer higher kinds from context (e.g.,
/// `type T<F : * -> *>` declares F with kind `* -> *`).
pub fn infer_kind_for_generic_param(has_bounds: bool) -> HrKind {
    // Today: all generic params default to kind Star.
    // Bounds (if present) constrain the params but don't elevate their kind.
    let _ = has_bounds; // Suppress unused warning; placeholder for future refinement.
    HrKind::Star
}

/// Compute the kind of a type constructor given its arity.
///
/// For a type constructor expecting `arity` arguments, the kind is:
/// - arity 0 → `Star` (already a concrete type)
/// - arity 1 → `* -> *`
/// - arity 2 → `* -> * -> *`
/// - arity n → `* -> * -> ... -> *` (n arrows)
///
/// Example:
/// - `Vec<T>` has arity 1 → kind `* -> *`
/// - `HashMap<K, V>` has arity 2 → kind `* -> * -> *`
pub fn kind_of_type_constructor(arity: usize) -> HrKind {
    let mut k = HrKind::Star;
    for _ in 0..arity {
        k = HrKind::arrow(HrKind::Star, k);
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_kind_for_generic_param_returns_star() {
        let kind_no_bounds = infer_kind_for_generic_param(false);
        let kind_with_bounds = infer_kind_for_generic_param(true);

        assert_eq!(kind_no_bounds, HrKind::star());
        assert_eq!(kind_with_bounds, HrKind::star());
    }

    #[test]
    fn kind_of_type_constructor_arity_zero_returns_star() {
        let kind = kind_of_type_constructor(0);
        assert_eq!(kind, HrKind::star());
        assert_eq!(kind.arity(), 0);
    }

    #[test]
    fn kind_of_type_constructor_arity_one_returns_arrow() {
        let kind = kind_of_type_constructor(1);
        let expected = HrKind::arrow(HrKind::star(), HrKind::star());
        assert_eq!(kind, expected);
        assert_eq!(kind.arity(), 1);
    }

    #[test]
    fn kind_of_type_constructor_arity_two_returns_nested_arrow() {
        let kind = kind_of_type_constructor(2);
        let expected = HrKind::arrow(
            HrKind::star(),
            HrKind::arrow(HrKind::star(), HrKind::star()),
        );
        assert_eq!(kind, expected);
        assert_eq!(kind.arity(), 2);
    }

    #[test]
    fn kind_of_type_constructor_arity_three() {
        let kind = kind_of_type_constructor(3);
        assert_eq!(kind.arity(), 3);
    }
}
