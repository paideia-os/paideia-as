//! Hindley-Milner style unification algorithm.
//!
//! Unifies two interned types by solving a constraint over the
//! substitution. Phase-1 unification:
//! - Ignores effect rows and capability sets (elaborator checks separately).
//! - Does not support row polymorphism or higher-rank types.
//! - Uses the occurs check to prevent infinite types.

use paideia_as_diagnostics::{Category, DiagnosticCode, Severity};
use thiserror::Error;

use crate::intern::TypeInterner;
use crate::kinds::HrKind;
use crate::subst::Subst;
use crate::types::{EnumPayload, TyVar, Type, TypeId};

/// Error type for unification failures.
///
/// Each variant maps to a stable diagnostic code per `design/toolchain/diagnostics.md` §2.
#[derive(Debug, Clone, Error)]
pub enum UnifyError {
    /// Kind mismatch: attempt to unify structurally different types (e.g., Fn vs Tuple).
    /// Diagnostic code T0504 / T0500 family.
    #[error("kind mismatch: cannot unify {a:?} with {b:?}")]
    KindMismatch {
        /// First type (for display).
        a: String,
        /// Second type (for display).
        b: String,
    },

    /// Occurs check failure: type variable appears in its own binding target.
    /// Diagnostic code T0503.
    #[error("occurs check failed: {var} appears in {ty:?}")]
    OccursCheck {
        /// The type variable.
        var: TyVar,
        /// The type it would bind to.
        ty: String,
    },

    /// Arity mismatch: function or tuple has different component counts.
    /// Diagnostic code T0500.
    #[error("arity mismatch")]
    ArityMismatch,
}

impl UnifyError {
    /// Return the stable diagnostic code for this error.
    pub fn code(&self) -> DiagnosticCode {
        let n = match self {
            Self::KindMismatch { .. } => 504,
            Self::OccursCheck { .. } => 503,
            Self::ArityMismatch => 500,
        };
        DiagnosticCode::new(Category::T, Severity::Error, n).expect("valid T diagnostic code")
    }
}

/// Unify two interned types under a substitution.
///
/// Implements the standard Hindley-Milner algorithm:
/// 1. If either type is a variable, attempt to bind it (with occurs check).
/// 2. If both are structurally identical (same constructor, same args), succeed.
/// 3. Otherwise, fail with a kind mismatch.
///
/// On success, mutates `subst` in place with new bindings. On failure,
/// leaves `subst` unchanged.
///
/// Phase-1 ignores effect rows and capability sets in function types;
/// the elaborator checks those separately.
pub fn unify(
    interner: &mut TypeInterner,
    subst: &mut Subst,
    a: TypeId,
    b: TypeId,
) -> Result<(), UnifyError> {
    let a_ty = interner.get(a).clone();
    let b_ty = interner.get(b).clone();
    match (a_ty, b_ty) {
        // Variable cases: bind the variable.
        (Type::Var { name, kind }, other) => bind(interner, subst, name, kind, b, &other),
        (other, Type::Var { name, kind }) => bind(interner, subst, name, kind, a, &other),
        // Primitive types: succeed if identical.
        (Type::Unit, Type::Unit)
        | (Type::Bool, Type::Bool)
        | (Type::Char, Type::Char)
        | (Type::Top, Type::Top)
        | (Type::Bot, Type::Bot)
        | (Type::Term, Type::Term) => Ok(()),
        (Type::UInt(b1), Type::UInt(b2)) if b1 == b2 => Ok(()),
        (Type::SInt(b1), Type::SInt(b2)) if b1 == b2 => Ok(()),
        (Type::Float(b1), Type::Float(b2)) if b1 == b2 => Ok(()),
        // Tuples: unify component-wise.
        (Type::Tuple(xs), Type::Tuple(ys)) => {
            if xs.len() != ys.len() {
                return Err(UnifyError::ArityMismatch);
            }
            for (x, y) in xs.iter().zip(ys.iter()) {
                unify(interner, subst, *x, *y)?;
            }
            Ok(())
        }
        // Functions: unify params and return type.
        // Phase-1: effects and caps are ignored; elaborator checks them.
        (
            Type::Fn {
                params: p1,
                ret: r1,
                ..
            },
            Type::Fn {
                params: p2,
                ret: r2,
                ..
            },
        ) => {
            if p1.len() != p2.len() {
                return Err(UnifyError::ArityMismatch);
            }
            for (x, y) in p1.iter().zip(p2.iter()) {
                unify(interner, subst, *x, *y)?;
            }
            unify(interner, subst, r1, r2)?;
            Ok(())
        }
        // Named types: unify if names match and args agree.
        (Type::Named { name: n1, args: a1 }, Type::Named { name: n2, args: a2 })
            if n1 == n2 && a1.len() == a2.len() =>
        {
            for (x, y) in a1.iter().zip(a2.iter()) {
                unify(interner, subst, *x, *y)?;
            }
            Ok(())
        }
        // Pointer types: unify if mutability matches, then unify pointees.
        (
            Type::Ptr {
                pointee: p1,
                mutable: m1,
            },
            Type::Ptr {
                pointee: p2,
                mutable: m2,
            },
        ) => {
            if m1 != m2 {
                return Err(UnifyError::KindMismatch {
                    a: format!("*{}", if m1 { "mut " } else { "" }),
                    b: format!("*{}", if m2 { "mut " } else { "" }),
                });
            }
            unify(interner, subst, p1, p2)
        }
        // Record types: unify if field names and types match in order.
        (Type::Record { fields: f1 }, Type::Record { fields: f2 }) => {
            if f1.len() != f2.len() {
                return Err(UnifyError::ArityMismatch);
            }
            for ((n1, t1), (n2, t2)) in f1.iter().zip(f2.iter()) {
                if n1 != n2 {
                    return Err(UnifyError::KindMismatch {
                        a: format!("field with symbol {}", n1),
                        b: format!("field with symbol {}", n2),
                    });
                }
                unify(interner, subst, *t1, *t2)?;
            }
            Ok(())
        }
        // Enum types: unify if variant names and payload shapes/types match in order.
        (Type::Enum { variants: v1 }, Type::Enum { variants: v2 }) => {
            if v1.len() != v2.len() {
                return Err(UnifyError::ArityMismatch);
            }
            for ((n1, p1), (n2, p2)) in v1.iter().zip(v2.iter()) {
                if n1 != n2 {
                    return Err(UnifyError::KindMismatch {
                        a: format!("variant with symbol {}", n1),
                        b: format!("variant with symbol {}", n2),
                    });
                }
                unify_payload(interner, subst, p1, p2)?;
            }
            Ok(())
        }
        // Default: kind mismatch.
        (ta, tb) => Err(UnifyError::KindMismatch {
            a: format!("{ta:?}"),
            b: format!("{tb:?}"),
        }),
    }
}

/// Attempt to bind a type variable.
///
/// If the variable is already bound to itself, this is a no-op (α ~ α).
/// Otherwise, performs a kind check (m9-002), an occurs check, and inserts the binding.
///
/// **Kind checking (m9-002):** Two type variables can only unify if they have
/// the same kind. For example, a variable declared with kind `*` can bind to
/// a concrete type or another `*`-kinded variable, but NOT to a `* -> *` constructor.
fn bind(
    interner: &mut TypeInterner,
    subst: &mut Subst,
    var_name: u32,
    var_kind: HrKind,
    t_id: TypeId,
    t_view: &Type,
) -> Result<(), UnifyError> {
    // α ~ α: no-op.
    if let Type::Var {
        name: other_name,
        kind: other_kind,
    } = t_view
    {
        if *other_name == var_name {
            return Ok(());
        }
        // m9-002: Two type variables unify only if they have the same kind.
        if var_kind != *other_kind {
            return Err(UnifyError::KindMismatch {
                a: format!("type variable α{} with kind {:?}", var_name, var_kind),
                b: format!("type variable α{} with kind {:?}", other_name, other_kind),
            });
        }
    }

    // Occurs check.
    let v = TyVar::new(var_name).expect("var_name should be non-zero");
    if subst.occurs_in(v, t_id, interner) {
        return Err(UnifyError::OccursCheck {
            var: v,
            ty: format!("{t_view:?}"),
        });
    }
    subst.insert(v, t_id);
    Ok(())
}

/// Unify two enum variant payloads.
///
/// Both must have the same shape (Unit, Tuple, or Record) and if they have
/// types, those types must unify component-wise.
fn unify_payload(
    interner: &mut TypeInterner,
    subst: &mut Subst,
    p1: &EnumPayload,
    p2: &EnumPayload,
) -> Result<(), UnifyError> {
    match (p1, p2) {
        // Both unit: unify succeeds
        (EnumPayload::Unit, EnumPayload::Unit) => Ok(()),
        // Both tuples: unify component-wise
        (EnumPayload::Tuple(t1), EnumPayload::Tuple(t2)) => {
            if t1.len() != t2.len() {
                return Err(UnifyError::ArityMismatch);
            }
            for (ty1, ty2) in t1.iter().zip(t2.iter()) {
                unify(interner, subst, *ty1, *ty2)?;
            }
            Ok(())
        }
        // Both records: unify field-wise
        (EnumPayload::Record(f1), EnumPayload::Record(f2)) => {
            if f1.len() != f2.len() {
                return Err(UnifyError::ArityMismatch);
            }
            for ((n1, t1), (n2, t2)) in f1.iter().zip(f2.iter()) {
                if n1 != n2 {
                    return Err(UnifyError::KindMismatch {
                        a: format!("record field with symbol {}", n1),
                        b: format!("record field with symbol {}", n2),
                    });
                }
                unify(interner, subst, *t1, *t2)?;
            }
            Ok(())
        }
        // Payload shape mismatch
        (p1, p2) => Err(UnifyError::KindMismatch {
            a: format!("{p1:?}"),
            b: format!("{p2:?}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::HrKind;
    use crate::types::CapSetId;
    use paideia_as_ir::EffectRowId;

    #[test]
    fn unify_var_with_concrete_extends_subst() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let alpha = TyVar::new(1).unwrap();
        let u64_id = interner.uint(64);

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let result = unify(&mut interner, &mut subst, alpha_id, u64_id);

        assert!(result.is_ok());
        assert_eq!(subst.get(alpha), Some(u64_id));
    }

    #[test]
    fn unify_var_with_self_is_noop() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let alpha = TyVar::new(1).unwrap();

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let result = unify(&mut interner, &mut subst, alpha_id, alpha_id);

        assert!(result.is_ok());
        assert!(subst.is_empty());
    }

    #[test]
    fn unify_concrete_eq() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);

        let result = unify(&mut interner, &mut subst, u64_id, u64_id);
        assert!(result.is_ok());
    }

    #[test]
    fn unify_kind_mismatch_fn_vs_tuple() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);

        let fn_type = interner.intern(Type::Fn {
            params: vec![u64_id],
            ret: u64_id,
            effects: EffectRowId::EMPTY,
            caps: CapSetId::EMPTY,
        });

        let tuple_type = interner.intern(Type::Tuple(vec![u64_id, u64_id]));

        let result = unify(&mut interner, &mut subst, fn_type, tuple_type);
        assert!(result.is_err());

        if let Err(UnifyError::KindMismatch { .. }) = result {
            // Check diagnostic code
            assert_eq!(result.unwrap_err().code().to_string(), "T0504");
        } else {
            panic!("Expected KindMismatch");
        }
    }

    #[test]
    fn unify_occurs_check_alpha_in_tuple() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let alpha = TyVar::new(1).unwrap();
        let u64_id = interner.uint(64);

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let tuple_type = interner.intern(Type::Tuple(vec![alpha_id, u64_id]));

        let result = unify(&mut interner, &mut subst, alpha_id, tuple_type);
        assert!(result.is_err());

        if let Err(UnifyError::OccursCheck { var, .. }) = result {
            assert_eq!(var, alpha);
            let code = UnifyError::OccursCheck {
                var,
                ty: String::new(),
            }
            .code();
            assert_eq!(code.to_string(), "T0503");
        } else {
            panic!("Expected OccursCheck");
        }
    }

    #[test]
    fn unify_arity_mismatch() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);

        let fn1 = interner.intern(Type::Fn {
            params: vec![u64_id],
            ret: u64_id,
            effects: EffectRowId::EMPTY,
            caps: CapSetId::EMPTY,
        });

        let fn2 = interner.intern(Type::Fn {
            params: vec![u64_id, u64_id],
            ret: u64_id,
            effects: EffectRowId::EMPTY,
            caps: CapSetId::EMPTY,
        });

        let result = unify(&mut interner, &mut subst, fn1, fn2);
        assert!(result.is_err());

        if let Err(UnifyError::ArityMismatch) = result {
            let code = UnifyError::ArityMismatch.code();
            assert_eq!(code.to_string(), "T0500");
        } else {
            panic!("Expected ArityMismatch");
        }
    }

    #[test]
    fn unify_tuple_componentwise() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);
        let bool_id = interner.bool_ty();

        let alpha = TyVar::new(1).unwrap();
        let beta = TyVar::new(2).unwrap();

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let beta_id = interner.intern(Type::Var {
            name: beta.get(),
            kind: HrKind::star(),
        });

        let tuple1 = interner.intern(Type::Tuple(vec![alpha_id, u64_id]));
        let tuple2 = interner.intern(Type::Tuple(vec![bool_id, beta_id]));

        let result = unify(&mut interner, &mut subst, tuple1, tuple2);
        assert!(result.is_ok());

        assert_eq!(subst.get(alpha), Some(bool_id));
        assert_eq!(subst.get(beta), Some(u64_id));
    }

    #[test]
    fn unify_propagates_through_fn() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);
        let bool_id = interner.bool_ty();

        let alpha = TyVar::new(1).unwrap();
        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });

        let fn1 = interner.intern(Type::Fn {
            params: vec![alpha_id],
            ret: bool_id,
            effects: EffectRowId::EMPTY,
            caps: CapSetId::EMPTY,
        });

        let fn2 = interner.intern(Type::Fn {
            params: vec![u64_id],
            ret: bool_id,
            effects: EffectRowId::EMPTY,
            caps: CapSetId::EMPTY,
        });

        let result = unify(&mut interner, &mut subst, fn1, fn2);
        assert!(result.is_ok());
        assert_eq!(subst.get(alpha), Some(u64_id));
    }

    #[test]
    fn occurs_check_through_substitution() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let alpha = TyVar::new(1).unwrap();
        let beta = TyVar::new(2).unwrap();
        let u64_id = interner.uint(64);

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });

        // Bind beta to Tuple[alpha, u64]
        let tuple_id = interner.intern(Type::Tuple(vec![alpha_id, u64_id]));
        subst.insert(beta, tuple_id);

        // Now check that alpha indeed appears in beta's transitive binding
        assert!(subst.occurs_in(alpha, tuple_id, &interner));
    }

    #[test]
    fn unify_named_types_match_by_name() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();

        let named1 = interner.intern(Type::Named {
            name: 42,
            args: vec![],
        });

        let named2 = interner.intern(Type::Named {
            name: 42,
            args: vec![],
        });

        let result = unify(&mut interner, &mut subst, named1, named2);
        assert!(result.is_ok());
    }

    #[test]
    fn unify_named_types_differ_by_name() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();

        let named1 = interner.intern(Type::Named {
            name: 42,
            args: vec![],
        });

        let named2 = interner.intern(Type::Named {
            name: 43,
            args: vec![],
        });

        let result = unify(&mut interner, &mut subst, named1, named2);
        assert!(result.is_err());
    }

    #[test]
    fn unify_ptr_same_pointee_succeeds() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);

        let ptr1 = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: false,
        });
        let ptr2 = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: false,
        });

        let result = unify(&mut interner, &mut subst, ptr1, ptr2);
        assert!(result.is_ok(), "*u64 should unify with *u64: {:?}", result);
    }

    #[test]
    fn unify_ptr_different_mutability_fails() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);

        let ptr_immutable = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: false,
        });
        let ptr_mutable = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: true,
        });

        let result = unify(&mut interner, &mut subst, ptr_immutable, ptr_mutable);
        assert!(result.is_err(), "*u64 should not unify with *mut u64");

        if let Err(UnifyError::KindMismatch { .. }) = result {
            // Expected error type
        } else {
            panic!("Expected KindMismatch error");
        }
    }

    #[test]
    fn unify_var_with_ptr_binds_var() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let alpha = TyVar::new(1).unwrap();
        let u64_id = interner.uint(64);

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let ptr_u64 = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: false,
        });

        let result = unify(&mut interner, &mut subst, alpha_id, ptr_u64);
        assert!(result.is_ok(), "Var should unify with Ptr: {:?}", result);
        assert_eq!(
            subst.get(alpha),
            Some(ptr_u64),
            "Var should be bound to Ptr type"
        );
    }

    #[test]
    fn unify_record_same_fields_succeeds() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);
        let bool_id = interner.bool_ty();

        let record1 = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u64_id), (2, bool_id)],
        });
        let record2 = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u64_id), (2, bool_id)],
        });

        let result = unify(&mut interner, &mut subst, record1, record2);
        assert!(
            result.is_ok(),
            "Records with same fields should unify: {:?}",
            result
        );
    }

    #[test]
    fn unify_record_different_field_name_fails() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);
        let bool_id = interner.bool_ty();

        let record1 = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u64_id), (2, bool_id)],
        });
        let record2 = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u64_id), (3, bool_id)],
        });

        let result = unify(&mut interner, &mut subst, record1, record2);
        assert!(
            result.is_err(),
            "Records with different field names should not unify"
        );

        if let Err(UnifyError::KindMismatch { .. }) = result {
            // Expected error type
        } else {
            panic!("Expected KindMismatch error, got {:?}", result);
        }
    }

    #[test]
    fn unify_record_different_arity_fails() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);
        let bool_id = interner.bool_ty();

        let record1 = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u64_id)],
        });
        let record2 = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u64_id), (2, bool_id)],
        });

        let result = unify(&mut interner, &mut subst, record1, record2);
        assert!(
            result.is_err(),
            "Records with different field counts should not unify"
        );

        if let Err(UnifyError::ArityMismatch) = result {
            // Expected error type
        } else {
            panic!("Expected ArityMismatch error, got {:?}", result);
        }
    }

    #[test]
    fn unify_enum_same_variants_succeeds() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);

        let enum1 = interner.intern(Type::Enum {
            variants: smallvec::smallvec![
                (1, EnumPayload::Unit),
                (2, EnumPayload::Tuple(smallvec::smallvec![u64_id])),
            ],
        });
        let enum2 = interner.intern(Type::Enum {
            variants: smallvec::smallvec![
                (1, EnumPayload::Unit),
                (2, EnumPayload::Tuple(smallvec::smallvec![u64_id])),
            ],
        });

        let result = unify(&mut interner, &mut subst, enum1, enum2);
        assert!(
            result.is_ok(),
            "Enums with same variants should unify: {:?}",
            result
        );
    }

    #[test]
    fn unify_enum_different_variant_name_fails() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let _u64_id = interner.uint(64);

        let enum1 = interner.intern(Type::Enum {
            variants: smallvec::smallvec![(1, EnumPayload::Unit), (2, EnumPayload::Unit)],
        });
        let enum2 = interner.intern(Type::Enum {
            variants: smallvec::smallvec![(1, EnumPayload::Unit), (3, EnumPayload::Unit)],
        });

        let result = unify(&mut interner, &mut subst, enum1, enum2);
        assert!(
            result.is_err(),
            "Enums with different variant names should not unify"
        );

        if let Err(UnifyError::KindMismatch { .. }) = result {
            // Expected error type
        } else {
            panic!("Expected KindMismatch error, got {:?}", result);
        }
    }

    #[test]
    fn unify_enum_different_payload_fails() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let u64_id = interner.uint(64);

        let enum1 = interner.intern(Type::Enum {
            variants: smallvec::smallvec![(1, EnumPayload::Unit)],
        });
        let enum2 = interner.intern(Type::Enum {
            variants: smallvec::smallvec![(1, EnumPayload::Tuple(smallvec::smallvec![u64_id]))],
        });

        let result = unify(&mut interner, &mut subst, enum1, enum2);
        assert!(
            result.is_err(),
            "Enums with different payload shapes should not unify"
        );

        if let Err(UnifyError::KindMismatch { .. }) = result {
            // Expected error type
        } else {
            panic!("Expected KindMismatch error, got {:?}", result);
        }
    }

    // m9-002 Higher-rank kind checking tests

    #[test]
    fn type_var_unifies_with_concrete_type() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let alpha = TyVar::new(1).unwrap();
        let u64_id = interner.uint(64);

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let result = unify(&mut interner, &mut subst, alpha_id, u64_id);

        assert!(
            result.is_ok(),
            "Type variable with kind * should unify with concrete type: {:?}",
            result
        );
        assert_eq!(subst.get(alpha), Some(u64_id));
    }

    #[test]
    fn two_type_vars_with_same_kind_unify() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let alpha = TyVar::new(1).unwrap();
        let beta = TyVar::new(2).unwrap();

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let beta_id = interner.intern(Type::Var {
            name: beta.get(),
            kind: HrKind::star(),
        });

        let result = unify(&mut interner, &mut subst, alpha_id, beta_id);

        assert!(
            result.is_ok(),
            "Two type variables with the same kind should unify: {:?}",
            result
        );
    }

    #[test]
    fn two_type_vars_with_different_kinds_fail() {
        let mut interner = TypeInterner::new();
        let mut subst = Subst::new();
        let alpha = TyVar::new(1).unwrap();
        let beta = TyVar::new(2).unwrap();

        // alpha has kind * (Star)
        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });

        // beta has kind * -> * (Arrow)
        let beta_id = interner.intern(Type::Var {
            name: beta.get(),
            kind: HrKind::arrow(HrKind::star(), HrKind::star()),
        });

        let result = unify(&mut interner, &mut subst, alpha_id, beta_id);

        assert!(
            result.is_err(),
            "Two type variables with different kinds should not unify"
        );

        if let Err(UnifyError::KindMismatch { .. }) = result {
            // Expected error type
        } else {
            panic!("Expected KindMismatch error, got {:?}", result);
        }
    }
}
