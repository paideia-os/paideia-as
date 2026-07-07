//! Function-pointer signature type checking (issue #980, pa-r17-002).
//!
//! Implements assignment compatibility checking for function-pointer types.
//! When a let-binding annotated with a function-pointer type receives a value,
//! the value's signature must be compatible with the annotated signature.
//!
//! Compatibility requires:
//! 1. Equal arity (number of parameters).
//! 2. Invariant parameter types (each must unify).
//! 3. Invariant return type.
//! 4. Source's effect row must be a subset of target's (widening on assignment).
//! 5. Source's capability set must be a subset of target's.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_effects::EffectInterner;
use paideia_as_types::{CapSetInterner, Subst, Type, TypeId, TypeInterner, unify};

/// Diagnostic code for function-pointer signature mismatch.
pub const T_FN_PTR_SIG_MISMATCH: u16 = 535;

/// Check function-pointer assignment compatibility.
///
/// Validates that a value with type `source` can be assigned to a binding
/// annotated with type `target`, both of which must be function-pointer types.
///
/// Returns a vector of diagnostics (typically 0 or 1 per call; short-circuits
/// on first structural mismatch to avoid cascading errors).
///
/// # Arguments
/// - `types`: The type interner (mutable for deref'ing and potential future work).
/// - `subst`: The substitution context (mutable per unify's requirement).
/// - `effects`: The effect-row interner.
/// - `caps`: The capability-set interner.
/// - `target`: The annotated LHS type (must be Type::Fn).
/// - `source`: The RHS value's type (must be Type::Fn).
/// - `span`: Source location for diagnostics.
pub fn check_fn_ptr_assignment(
    types: &mut TypeInterner,
    subst: &mut Subst,
    effects: &EffectInterner,
    caps: &CapSetInterner,
    target: TypeId,
    source: TypeId,
    span: Span,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // Dereference both types.
    let target_ty = types.get(target).clone();
    let source_ty = types.get(source).clone();

    // Extract function-pointer data; if either is not Fn, emit T0535 and return.
    let (target_params, target_ret, target_effects, target_caps) = match target_ty {
        Type::Fn {
            params: p,
            ret: r,
            effects: e,
            caps: c,
        } => (p, r, e, c),
        _ => {
            diags.push(
                Diagnostic::error(t_code(T_FN_PTR_SIG_MISMATCH))
                    .message("not a function-pointer type: target binding's annotation is not a function-pointer type")
                    .with_span(span)
                    .finish(),
            );
            return diags;
        }
    };

    let (source_params, source_ret, source_effects, source_caps) = match source_ty {
        Type::Fn {
            params: p,
            ret: r,
            effects: e,
            caps: c,
        } => (p, r, e, c),
        _ => {
            diags.push(
                Diagnostic::error(t_code(T_FN_PTR_SIG_MISMATCH))
                    .message("not a function-pointer type: assigned value is not a function-pointer type")
                    .with_span(span)
                    .finish(),
            );
            return diags;
        }
    };

    // Check arity.
    if target_params.len() != source_params.len() {
        diags.push(
            Diagnostic::error(t_code(T_FN_PTR_SIG_MISMATCH))
                .message(format!(
                    "arity mismatch: expected {} parameter(s), found {}",
                    target_params.len(),
                    source_params.len()
                ))
                .with_span(span)
                .finish(),
        );
        return diags;
    }

    // Check parameter types (invariant: each must unify).
    for (i, (target_param, source_param)) in target_params.iter().zip(&source_params).enumerate() {
        if let Err(_e) = unify(types, subst, *target_param, *source_param) {
            diags.push(
                Diagnostic::error(t_code(T_FN_PTR_SIG_MISMATCH))
                    .message(format!(
                        "parameter {} type mismatch: expected {}, found {}",
                        i,
                        describe_fn_sig_type(types, *target_param),
                        describe_fn_sig_type(types, *source_param)
                    ))
                    .with_span(span)
                    .finish(),
            );
            return diags;
        }
    }

    // Check return type (invariant).
    // Phase-1: skip check if source return type is Top (placeholder).
    let source_ret_ty = types.get(source_ret);
    if !matches!(source_ret_ty, Type::Top) {
        if let Err(_e) = unify(types, subst, target_ret, source_ret) {
            diags.push(
                Diagnostic::error(t_code(T_FN_PTR_SIG_MISMATCH))
                    .message(format!(
                        "return type mismatch: expected {}, found {}",
                        describe_fn_sig_type(types, target_ret),
                        describe_fn_sig_type(types, source_ret)
                    ))
                    .with_span(span)
                    .finish(),
            );
            return diags;
        }
    }

    // Check effect subset (source's effects must be subset of target's).
    let target_row = effects.get(target_effects);
    let source_row = effects.get(source_effects);
    if !source_row.is_subset_of(target_row) {
        let extras = source_row
            .fixed
            .iter()
            .filter(|e| !target_row.fixed.contains(e))
            .map(|e| format!("e{}", e.get()))
            .collect::<Vec<_>>();
        diags.push(
            Diagnostic::error(t_code(T_FN_PTR_SIG_MISMATCH))
                .message(format!(
                    "effect row not subset: source may perform effects not permitted by target; extra effects: {}",
                    extras.join(", ")
                ))
                .with_span(span)
                .finish(),
        );
        return diags;
    }

    // Check capability subset (source's caps must be subset of target's).
    let target_cap_set = caps.get(target_caps);
    let source_cap_set = caps.get(source_caps);
    if !source_cap_set.is_subset_of(target_cap_set) {
        let extras = source_cap_set.missing_caps(target_cap_set);
        let extra_names = extras
            .iter()
            .map(|c| format!("c{}", c.get()))
            .collect::<Vec<_>>();
        diags.push(
            Diagnostic::error(t_code(T_FN_PTR_SIG_MISMATCH))
                .message(format!(
                    "capability set not subset: source requires capabilities not held by target; extra capabilities: {}",
                    extra_names.join(", ")
                ))
                .with_span(span)
                .finish(),
        );
        return diags;
    }

    diags
}

/// Construct a diagnostic code in the T (Type) category.
fn t_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, n).expect("valid T diagnostic code")
}

/// Produce a short human-readable description of a type for diagnostics.
/// Mirrors the pattern from `check_expr::describe_type`.
fn describe_fn_sig_type(types: &TypeInterner, t: TypeId) -> String {
    match types.get(t) {
        Type::Unit => "()".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Char => "char".to_string(),
        Type::UInt(64) => "u64".to_string(),
        Type::UInt(32) => "u32".to_string(),
        Type::UInt(16) => "u16".to_string(),
        Type::UInt(8) => "u8".to_string(),
        Type::UInt(w) => format!("u{}", w),
        Type::SInt(64) => "i64".to_string(),
        Type::SInt(32) => "i32".to_string(),
        Type::SInt(16) => "i16".to_string(),
        Type::SInt(8) => "i8".to_string(),
        Type::SInt(w) => format!("i{}", w),
        Type::Float(32) => "f32".to_string(),
        Type::Float(64) => "f64".to_string(),
        Type::Float(w) => format!("f{}", w),
        Type::Top => "Top".to_string(),
        Type::Bot => "Bot".to_string(),
        Type::Var { name, .. } => format!("α{}", name),
        Type::Fn { .. } => "(args) -> ret".to_string(),
        Type::Tuple(_) => "(elts,)".to_string(),
        Type::Named { .. } => "?".to_string(),
        Type::Term => "Term".to_string(),
        Type::Ptr { pointee, mutable } => {
            if *mutable {
                format!("*mut {}", describe_fn_sig_type(types, *pointee))
            } else {
                format!("*{}", describe_fn_sig_type(types, *pointee))
            }
        }
        Type::Ref {
            pointee,
            mutable,
            lifetime: _,
        } => {
            if *mutable {
                format!("&mut {}", describe_fn_sig_type(types, *pointee))
            } else {
                format!("&{}", describe_fn_sig_type(types, *pointee))
            }
        }
        Type::Str => "&str".to_string(),
        Type::Record { .. } => "record { ... }".to_string(),
        Type::Enum { .. } => "enum { ... }".to_string(),
        _ => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;
    use paideia_as_effects::{EffectId, EffectRow};
    use paideia_as_types::{CapId, CapSet};

    fn test_span(byte_start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, 1)
    }

    /// Test 1: equal_signatures_accept
    /// Two function pointers with identical signatures should accept.
    #[test]
    fn equal_signatures_accept() {
        let mut types = TypeInterner::new();
        let effects = EffectInterner::new();
        let caps = CapSetInterner::new();
        let mut subst = Subst::new();

        // Create (u64) -> u64 type.
        let u64_ty = types.intern(Type::UInt(64));
        let fn_ty = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            fn_ty,
            fn_ty,
            test_span(0),
        );

        assert!(diags.is_empty());
    }

    /// Test 2: arity_mismatch_source_extra_param_rejects
    /// Source has extra parameter; should reject.
    #[test]
    fn arity_mismatch_source_extra_param_rejects() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));

        // Target: (u64) -> u64
        let target_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        // Source: (u64, u64) -> u64
        let source_fn = types.intern(Type::Fn {
            params: vec![u64_ty, u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn,
            source_fn,
            test_span(0),
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), T_FN_PTR_SIG_MISMATCH);
    }

    /// Test 3: arity_mismatch_source_missing_param_rejects
    /// Source has fewer parameters; should reject.
    #[test]
    fn arity_mismatch_source_missing_param_rejects() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));

        // Target: (u64) -> u64
        let target_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        // Source: () -> u64
        let source_fn = types.intern(Type::Fn {
            params: vec![],
            ret: u64_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn,
            source_fn,
            test_span(0),
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), T_FN_PTR_SIG_MISMATCH);
    }

    /// Test 4: param_type_mismatch_rejects
    /// Parameter types do not unify; should reject.
    #[test]
    fn param_type_mismatch_rejects() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));
        let u32_ty = types.intern(Type::UInt(32));

        // Target: (u64) -> u64
        let target_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        // Source: (u32) -> u64
        let source_fn = types.intern(Type::Fn {
            params: vec![u32_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn,
            source_fn,
            test_span(0),
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), T_FN_PTR_SIG_MISMATCH);
    }

    /// Test 5: return_type_mismatch_rejects
    /// Return types do not unify; should reject.
    #[test]
    fn return_type_mismatch_rejects() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));
        let u32_ty = types.intern(Type::UInt(32));

        // Target: (u64) -> u64
        let target_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        // Source: (u64) -> u32
        let source_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u32_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn,
            source_fn,
            test_span(0),
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), T_FN_PTR_SIG_MISMATCH);
    }

    /// Test 6: effect_empty_source_into_effectful_target_accepts
    /// Empty effect row (subset of any row) should accept.
    #[test]
    fn effect_empty_source_into_effectful_target_accepts() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));

        // Create an effect row with Atomic (id=1).
        let atomic = EffectId::new(1).unwrap();
        let effectful_row = effects.intern(EffectRow::from_ids(vec![atomic], None));

        // Target: (u64) -> u64 !{Atomic}
        let target_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effectful_row,
            caps: caps.empty(),
        });

        // Source: (u64) -> u64 !{}
        let source_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn,
            source_fn,
            test_span(0),
        );

        assert!(diags.is_empty());
    }

    /// Test 7: effect_rows_equal_accept
    /// Equal effect rows should accept.
    #[test]
    fn effect_rows_equal_accept() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));

        let io = EffectId::new(1).unwrap();
        let atomic = EffectId::new(2).unwrap();
        let row = effects.intern(EffectRow::from_ids(vec![io, atomic], None));

        // Both have the same effect row.
        let target_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: row,
            caps: caps.empty(),
        });

        let source_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: row,
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn,
            source_fn,
            test_span(0),
        );

        assert!(diags.is_empty());
    }

    /// Test 8: effect_narrowing_rejects
    /// Source has effects target doesn't; should reject.
    #[test]
    fn effect_narrowing_rejects() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));

        let io = EffectId::new(1).unwrap();
        let empty_row = effects.empty();
        let io_row = effects.intern(EffectRow::from_ids(vec![io], None));

        // Target: (u64) -> u64 !{}
        let target_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: empty_row,
            caps: caps.empty(),
        });

        // Source: (u64) -> u64 !{Io}
        let source_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: io_row,
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn,
            source_fn,
            test_span(0),
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), T_FN_PTR_SIG_MISMATCH);
    }

    /// Test 9: effect_strict_subset_accepts
    /// Source's effects are strict subset of target's; should accept.
    #[test]
    fn effect_strict_subset_accepts() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));

        let io = EffectId::new(1).unwrap();
        let atomic = EffectId::new(2).unwrap();

        let io_row = effects.intern(EffectRow::from_ids(vec![io], None));
        let io_atomic_row = effects.intern(EffectRow::from_ids(vec![io, atomic], None));

        // Target: (u64) -> u64 !{Io, Atomic}
        let target_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: io_atomic_row,
            caps: caps.empty(),
        });

        // Source: (u64) -> u64 !{Io}
        let source_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: io_row,
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn,
            source_fn,
            test_span(0),
        );

        assert!(diags.is_empty());
    }

    /// Test 10: effect_rows_disjoint_reject
    /// Source and target have disjoint effects; should reject.
    #[test]
    fn effect_rows_disjoint_reject() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));

        let io = EffectId::new(1).unwrap();
        let atomic = EffectId::new(2).unwrap();

        let io_row = effects.intern(EffectRow::from_ids(vec![io], None));
        let atomic_row = effects.intern(EffectRow::from_ids(vec![atomic], None));

        // Target: (u64) -> u64 !{Atomic}
        let target_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: atomic_row,
            caps: caps.empty(),
        });

        // Source: (u64) -> u64 !{Io}
        let source_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: io_row,
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn,
            source_fn,
            test_span(0),
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), T_FN_PTR_SIG_MISMATCH);
    }

    /// Test 11: cap_set_subset_accepts_and_narrowing_rejects
    /// Tests both: {X}⊆{X,Y} accepts, and {X,Y}⊆{X} rejects.
    #[test]
    fn cap_set_subset_accepts_and_narrowing_rejects() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));

        let c1 = CapId::new(1).unwrap();
        let c2 = CapId::new(2).unwrap();

        let cap_x = caps.intern(CapSet::from_ids(vec![c1]));
        let cap_xy = caps.intern(CapSet::from_ids(vec![c1, c2]));

        // Test 11a: {X} ⊆ {X,Y} should accept.
        let target_fn_1 = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: cap_xy,
        });

        let source_fn_1 = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: cap_x,
        });

        let diags_1 = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn_1,
            source_fn_1,
            test_span(0),
        );
        assert!(diags_1.is_empty());

        // Test 11b: {X,Y} ⊆ {X} should reject.
        let mut subst2 = Subst::new();
        let target_fn_2 = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: cap_x,
        });

        let source_fn_2 = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: cap_xy,
        });

        let diags_2 = check_fn_ptr_assignment(
            &mut types,
            &mut subst2,
            &effects,
            &caps,
            target_fn_2,
            source_fn_2,
            test_span(0),
        );
        assert_eq!(diags_2.len(), 1);
        assert_eq!(diags_2[0].code().number(), T_FN_PTR_SIG_MISMATCH);
    }

    /// Test 12: t0535_carries_specific_reason_note
    /// Verify that the diagnostic code is T0535 and contains a structured note.
    #[test]
    fn t0535_carries_specific_reason_note() {
        let mut types = TypeInterner::new();
        let mut effects = EffectInterner::new();
        let mut caps = CapSetInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));
        let u32_ty = types.intern(Type::UInt(32));

        // Mismatched return types.
        let target_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u64_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        let source_fn = types.intern(Type::Fn {
            params: vec![u64_ty],
            ret: u32_ty,
            effects: effects.empty(),
            caps: caps.empty(),
        });

        let diags = check_fn_ptr_assignment(
            &mut types,
            &mut subst,
            &effects,
            &caps,
            target_fn,
            source_fn,
            test_span(0),
        );

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), T_FN_PTR_SIG_MISMATCH);
        // Verify a note mentioning return type mismatch is present.
        let diag_str = format!("{:?}", diags[0]);
        assert!(diag_str.contains("return type"));
    }
}
