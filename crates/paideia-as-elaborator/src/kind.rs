//! Type-to-LinClass derivation.
//!
//! Maps types to their substructural lattice class (`LinClass`) per the
//! Phase-3 specification. Raw pointers are always unrestricted (copying
//! a pointer does not move the underlying value).

use paideia_as_ir::LinClass;
use paideia_as_types::{Type, TypeId, TypeInterner};

/// Derive the substructural lattice class for a type.
///
/// # Semantics
///
/// - **Raw pointers** (`Type::Ptr { .. }`): always `Unrestricted`.
///   A raw pointer is just a number; copying it does not move the underlying value.
///   This holds regardless of the pointee's class or mutability.
///
/// - **Borrowed references** (`Type::Ref { .. }`): depends on mutability.
///   - `&T` (immutable borrow): `Affine` — at-most-once usage within borrow scope.
///   - `&mut T` (mutable borrow): `Linear` — exactly-once usage (exclusive access).
///   The pointee's class and lifetime are ignored for the borrow kind classification
///   (they affect scope and aliasing rules, threaded by m6 borrow checker).
///
/// - **Type variables**: `Unrestricted` (safe default).
///
/// - **All other types** (atomics, tuples, functions, etc.): `Unrestricted` (default).
pub fn type_kind(interner: &TypeInterner, ty: TypeId) -> LinClass {
    match interner.get(ty) {
        Type::Ptr { .. } => LinClass::Unrestricted,
        Type::Ref { mutable, .. } => {
            if *mutable {
                LinClass::Linear // &mut T = exclusive access
            } else {
                LinClass::Affine // &T = at-most-once usage (within borrow scope)
            }
        }
        Type::Var { .. } => LinClass::Unrestricted,
        // All other types default to Unrestricted for phase-3.
        _ => LinClass::Unrestricted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_kind_for_ptr_is_unrestricted() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let linear_u64_id = u64_id; // In phase-3, linear types are derived; for now, all are Unrestricted

        let ptr_id = interner.intern(Type::Ptr {
            pointee: linear_u64_id,
            mutable: false,
        });

        let kind = type_kind(&interner, ptr_id);
        assert_eq!(
            kind,
            LinClass::Unrestricted,
            "immutable raw pointer should be Unrestricted"
        );
    }

    #[test]
    fn type_kind_for_ptr_mutable_is_unrestricted() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);

        let ptr_mutable_id = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: true,
        });

        let kind = type_kind(&interner, ptr_mutable_id);
        assert_eq!(
            kind,
            LinClass::Unrestricted,
            "mutable raw pointer should be Unrestricted"
        );
    }

    #[test]
    fn type_kind_for_ref_is_affine() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);

        let ref_id = interner.intern(Type::Ref {
            pointee: u64_id,
            mutable: false,
            lifetime: 0, // 'static
        });

        let kind = type_kind(&interner, ref_id);
        assert_eq!(
            kind,
            LinClass::Affine,
            "immutable reference &T should be Affine"
        );
    }

    #[test]
    fn type_kind_for_ref_mut_is_linear() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);

        let ref_mut_id = interner.intern(Type::Ref {
            pointee: u64_id,
            mutable: true,
            lifetime: 0, // 'static
        });

        let kind = type_kind(&interner, ref_mut_id);
        assert_eq!(
            kind,
            LinClass::Linear,
            "mutable reference &mut T should be Linear"
        );
    }

    #[test]
    fn type_kind_for_ref_independent_of_pointee_class() {
        let mut interner = TypeInterner::new();
        // Create two pointees: u64 and bool (different types)
        let u64_id = interner.uint(64);
        let bool_id = interner.bool_ty();

        // Both should yield Affine when borrowed immutably
        let ref_u64_id = interner.intern(Type::Ref {
            pointee: u64_id,
            mutable: false,
            lifetime: 0,
        });
        let ref_bool_id = interner.intern(Type::Ref {
            pointee: bool_id,
            mutable: false,
            lifetime: 0,
        });

        assert_eq!(
            type_kind(&interner, ref_u64_id),
            LinClass::Affine,
            "&u64 should be Affine"
        );
        assert_eq!(
            type_kind(&interner, ref_bool_id),
            LinClass::Affine,
            "&bool should be Affine"
        );
    }

    #[test]
    fn type_kind_for_ref_lifetime_ignored() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);

        // Create two immutable references with different lifetimes
        let ref_static_id = interner.intern(Type::Ref {
            pointee: u64_id,
            mutable: false,
            lifetime: 0, // 'static
        });
        let ref_region_id = interner.intern(Type::Ref {
            pointee: u64_id,
            mutable: false,
            lifetime: 42, // m5 region id
        });

        let kind_static = type_kind(&interner, ref_static_id);
        let kind_region = type_kind(&interner, ref_region_id);

        assert_eq!(
            kind_static,
            LinClass::Affine,
            "&T with 'static should be Affine"
        );
        assert_eq!(
            kind_region,
            LinClass::Affine,
            "&T with m5 region should be Affine"
        );
        assert_eq!(
            kind_static, kind_region,
            "Affine class should be independent of lifetime"
        );
    }
}
