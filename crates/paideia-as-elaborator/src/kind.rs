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
/// - **Type variables**: `Unrestricted` (safe default).
///
/// - **All other types** (atomics, tuples, functions, etc.): `Unrestricted` (default).
///
/// # Future Extensions
///
/// When borrowed references `&T` / `&mut T` are added (phase-3+), this function
/// should derive their class based on the pointee and the borrow region.
pub fn type_kind(interner: &TypeInterner, ty: TypeId) -> LinClass {
    match interner.get(ty) {
        Type::Ptr { .. } => LinClass::Unrestricted,
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
}
