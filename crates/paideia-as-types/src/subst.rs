//! Type substitution and unification variable environment.
//!
//! A substitution is a mapping from type variables to interned types.
//! The [`Subst`] type manages these bindings and provides operations to
//! apply and introspect the substitution.

use std::collections::HashMap;

use crate::intern::TypeInterner;
use crate::types::{TyVar, Type, TypeId};

/// Type variable substitution environment.
///
/// Maps type variables to their interned type assignments. Used during
/// unification and type inference to track variable bindings.
#[derive(Clone, Debug)]
pub struct Subst {
    /// Bindings from type variables to their interned type values.
    bindings: HashMap<TyVar, TypeId>,
}

impl Subst {
    /// Construct a new, empty substitution.
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Look up a variable's binding.
    ///
    /// Returns `Some(TypeId)` if the variable is bound, or `None` otherwise.
    pub fn get(&self, v: TyVar) -> Option<TypeId> {
        self.bindings.get(&v).copied()
    }

    /// Insert a binding from a variable to a type.
    ///
    /// If the variable was already bound, the old binding is overwritten.
    pub fn insert(&mut self, v: TyVar, t: TypeId) {
        self.bindings.insert(v, t);
    }

    /// Check if a type variable appears anywhere inside a type.
    ///
    /// Recursively walks the type structure and follows substitution
    /// bindings. Returns `true` if `v` is found, `false` otherwise.
    /// This is the occurs check used to prevent infinite types.
    pub fn occurs_in(&self, v: TyVar, t: TypeId, interner: &TypeInterner) -> bool {
        let ty = interner.get(t);
        match ty {
            Type::Var { name, .. } => {
                if *name == v.get() {
                    return true;
                }
                // Follow substitution chains.
                if let Some(target) = self.get(v) {
                    return self.occurs_in(v, target, interner);
                }
                false
            }
            Type::Tuple(xs) => xs.iter().any(|x| self.occurs_in(v, *x, interner)),
            Type::Fn { params, ret, .. } => {
                params.iter().any(|p| self.occurs_in(v, *p, interner))
                    || self.occurs_in(v, *ret, interner)
            }
            Type::Named { args, .. } => args.iter().any(|a| self.occurs_in(v, *a, interner)),
            _ => false,
        }
    }

    /// Recursively apply this substitution to a type, returning the
    /// fully-resolved type.
    ///
    /// Walks the type structure and replaces any bound variables with
    /// their targets. Re-interns the result to maintain structural
    /// identity. If no changes occur, the returned `TypeId` may be
    /// identical to the input.
    pub fn apply(&self, interner: &mut TypeInterner, t: TypeId) -> TypeId {
        let ty = interner.get(t).clone();
        match ty {
            Type::Var { name, .. } => {
                let v = TyVar::new(name).expect("name should be non-zero");
                if let Some(target) = self.get(v) {
                    self.apply(interner, target)
                } else {
                    t
                }
            }
            Type::Tuple(xs) => {
                let new: Vec<TypeId> = xs.iter().map(|x| self.apply(interner, *x)).collect();
                interner.intern(Type::Tuple(new))
            }
            Type::Fn {
                params,
                ret,
                effects,
                caps,
            } => {
                let new_params: Vec<TypeId> =
                    params.iter().map(|p| self.apply(interner, *p)).collect();
                let new_ret = self.apply(interner, ret);
                interner.intern(Type::Fn {
                    params: new_params,
                    ret: new_ret,
                    effects,
                    caps,
                })
            }
            Type::Named { name, args } => {
                let new_args: Vec<TypeId> = args.iter().map(|a| self.apply(interner, *a)).collect();
                interner.intern(Type::Named {
                    name,
                    args: new_args,
                })
            }
            _ => t,
        }
    }

    /// Number of variable bindings in this substitution.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// True if the substitution is empty (no variable bindings).
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

impl Default for Subst {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::HrKind;

    #[test]
    fn apply_resolves_var_to_concrete() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let alpha = TyVar::new(1).unwrap();

        let mut subst = Subst::new();
        subst.insert(alpha, u64_id);

        let var_type_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let resolved = subst.apply(&mut interner, var_type_id);

        assert_eq!(resolved, u64_id);
    }

    #[test]
    fn apply_walks_tuple_recursively() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let bool_id = interner.bool_ty();
        let alpha = TyVar::new(1).unwrap();
        let beta = TyVar::new(2).unwrap();

        let mut subst = Subst::new();
        subst.insert(alpha, u64_id);
        subst.insert(beta, bool_id);

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let beta_id = interner.intern(Type::Var {
            name: beta.get(),
            kind: HrKind::star(),
        });
        let tuple_id = interner.intern(Type::Tuple(vec![alpha_id, beta_id]));

        let resolved = subst.apply(&mut interner, tuple_id);
        let resolved_ty = interner.get(resolved);

        if let Type::Tuple(components) = resolved_ty {
            assert_eq!(components.len(), 2);
            assert_eq!(components[0], u64_id);
            assert_eq!(components[1], bool_id);
        } else {
            panic!("Expected Tuple type");
        }
    }

    #[test]
    fn apply_intern_consistency() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);

        let subst = Subst::new();
        let resolved = subst.apply(&mut interner, u64_id);

        // Applying empty substitution to a concrete type should be idempotent.
        assert_eq!(resolved, u64_id);
    }

    #[test]
    fn occurs_check_simple() {
        let mut interner = TypeInterner::new();
        let alpha = TyVar::new(1).unwrap();
        let subst = Subst::new();

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        assert!(subst.occurs_in(alpha, alpha_id, &interner));
    }

    #[test]
    fn occurs_check_in_tuple() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let alpha = TyVar::new(1).unwrap();

        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let tuple_id = interner.intern(Type::Tuple(vec![alpha_id, u64_id]));

        let subst = Subst::new();
        assert!(subst.occurs_in(alpha, tuple_id, &interner));
    }

    #[test]
    fn occurs_check_through_substitution() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let alpha = TyVar::new(1).unwrap();
        let beta = TyVar::new(2).unwrap();

        // Bind beta to Tuple[alpha, u64]
        let alpha_id = interner.intern(Type::Var {
            name: alpha.get(),
            kind: HrKind::star(),
        });
        let tuple_id = interner.intern(Type::Tuple(vec![alpha_id, u64_id]));

        let mut subst = Subst::new();
        subst.insert(beta, tuple_id);

        // Check if alpha appears in beta's binding transitively
        assert!(subst.occurs_in(alpha, tuple_id, &interner));
    }

    #[test]
    fn subst_get_missing() {
        let subst = Subst::new();
        let alpha = TyVar::new(1).unwrap();
        assert!(subst.get(alpha).is_none());
    }

    #[test]
    fn subst_len() {
        let mut subst = Subst::new();
        assert_eq!(subst.len(), 0);

        let mut interner = TypeInterner::new();
        let alpha = TyVar::new(1).unwrap();
        let u64_id = interner.uint(64);
        subst.insert(alpha, u64_id);

        assert_eq!(subst.len(), 1);
    }
}
