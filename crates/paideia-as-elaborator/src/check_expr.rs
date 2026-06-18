//! Bottom-up type inference for primary expressions (IR phase-1).
//!
//! Implements inference for the constrained subset of IR nodes handled in
//! phase-1: `IrKind::Literal`, `IrKind::Var`, `IrKind::App`, `IrKind::Let`.
//! All other IR kinds fall through to `Type::Top` with no diagnostic.
//!
//! The inference walker is invoked per-node via [`infer_node`], collecting
//! diagnostics for unbound variables and later unification failures. Since
//! the IR (in PR-32/PR-36) does not yet carry child node pointers, phase-1
//! inference operates as a visitor over hand-constructed test IR instances;
//! full integration arrives in PR-37+.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_ir::{IrArena, IrKind, IrNodeId};
use paideia_as_types::{Subst, TyVar, Type, TypeId, TypeInterner, unify};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::env::{Symbol, TypeEnv};

/// Result of inferring a single IR node.
///
/// Pairs the inferred type with any diagnostics emitted during inference
/// (unification failures, unbound variables, etc.). The returned type is a
/// best-effort fresh variable if inference fails, allowing the caller to
/// continue propagating types.
#[derive(Debug)]
pub struct InferOutcome {
    /// The inferred type (or a fresh variable on error).
    pub ty: TypeId,
    /// Diagnostics emitted during inference.
    pub diagnostics: Vec<Diagnostic>,
}

/// Global counter for generating fresh type variables.
///
/// Each call to [`fresh_var`] increments this counter and produces a unique
/// `TyVar` identifier.
static NEXT_VAR: AtomicU32 = AtomicU32::new(1);

/// Generate a fresh type variable.
///
/// Returns a unique `Type::Var` that is distinct from all other fresh
/// variables and is ready for unification.
fn fresh_var(types: &mut TypeInterner) -> TypeId {
    let id = NEXT_VAR.fetch_add(1, Ordering::Relaxed);
    let v = TyVar::new(id).expect("counter wraps only after 2^32 calls");
    types.intern(Type::Var(v))
}

/// Construct a diagnostic code in the T (Type) category.
///
/// All phase-1 type errors are T-category (range 0500–0899).
/// Returns a new diagnostic code with the given number and Error severity.
fn t_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, n).expect("valid T diagnostic code")
}

/// Infer the type of a primary-expression IR node.
///
/// Phase-1 handles exactly four IR kinds:
/// - `Literal` — yields a fresh type variable.
/// - `Var` — looks up the symbol (node span byte_start) in the env; emits
///   T0501 ("use of unbound name") if absent.
/// - `App` — returns a fresh type variable (full inference requires child
///   pointers in the IR, arriving in PR-37+).
/// - `Let` — binds the span byte_start as a symbol to a fresh type var in
///   the env; returns unit.
///
/// Any other IR kind returns `Type::Top` with no diagnostic — they are
/// deferred to later passes.
pub fn infer_node(
    ir: &IrArena,
    types: &mut TypeInterner,
    env: &mut TypeEnv,
    _subst: &mut Subst,
    id: IrNodeId,
) -> InferOutcome {
    let mut diags = Vec::new();
    let node = ir[id];

    let ty = match node.kind {
        IrKind::Literal => {
            // Literal yields a fresh type variable.
            fresh_var(types)
        }
        IrKind::Var => {
            // Use span byte_start as a stand-in symbol id.
            let sym: Symbol = node.span.byte_start();
            env.lookup(sym).unwrap_or_else(|| {
                // Emit T0501 "use of unbound name".
                diags.push(
                    Diagnostic::error(t_code(501))
                        .message("use of unbound name")
                        .with_span(node.span)
                        .finish(),
                );
                types.top()
            })
        }
        IrKind::App => {
            // Phase-1: children are not yet in the IR. Return a fresh
            // type variable so the caller can continue. Full inference
            // will arrive when child pointers are wired.
            fresh_var(types)
        }
        IrKind::Let => {
            // Phase-1: bind the span byte_start as a symbol to a fresh
            // type var. Return unit. Full let-handling requires reading
            // the bound expression's NodeId (PR-37+).
            let sym: Symbol = node.span.byte_start();
            let value_ty = fresh_var(types);
            env.bind(sym, value_ty);
            types.unit()
        }
        _ => {
            // All other IR kinds fall through to Top (no diagnostic).
            types.top()
        }
    };

    InferOutcome {
        ty,
        diagnostics: diags,
    }
}

/// Check that a value type unifies with an annotation.
///
/// If unification succeeds, returns an empty diagnostic vector. On failure,
/// emits T0501 ("type mismatch") and returns a vec with one diagnostic.
/// The substitution is updated in-place on success; left unchanged on failure.
pub fn check_annotation(
    types: &mut TypeInterner,
    subst: &mut Subst,
    annotated_ty: TypeId,
    value_ty: TypeId,
    span: Span,
) -> Vec<Diagnostic> {
    match unify(types, subst, annotated_ty, value_ty) {
        Ok(()) => Vec::new(),
        Err(e) => vec![
            Diagnostic::error(e.code())
                .message(format!(
                    "type mismatch: expected {}, found {}",
                    describe_type(types, annotated_ty),
                    describe_type(types, value_ty),
                ))
                .with_span(span)
                .finish(),
        ],
    }
}

/// Produce a short human-readable description of a type for diagnostics.
fn describe_type(types: &TypeInterner, t: TypeId) -> String {
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
        Type::Var(v) => format!("α{}", v.get()),
        Type::Fn { .. } => "(args) -> ret".to_string(),
        Type::Tuple(_) => "(elts,)".to_string(),
        Type::Named { .. } => "?".to_string(),
        _ => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    /// Helper: construct a simple Span for testing.
    fn test_span(byte_start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, 1)
    }

    /// Test 1: infer_literal_returns_fresh_var — successive calls yield
    /// distinct type variables.
    #[test]
    fn infer_literal_returns_fresh_var() {
        let mut types = TypeInterner::new();
        let mut env = TypeEnv::new();
        let mut subst = Subst::new();

        let mut ir = IrArena::new();
        let span1 = test_span(0);
        let span2 = test_span(1);

        let id1 = ir.alloc(IrKind::Literal, span1);
        let id2 = ir.alloc(IrKind::Literal, span2);

        let outcome1 = infer_node(&ir, &mut types, &mut env, &mut subst, id1);
        let outcome2 = infer_node(&ir, &mut types, &mut env, &mut subst, id2);

        // Both should have no diagnostics.
        assert!(outcome1.diagnostics.is_empty());
        assert!(outcome2.diagnostics.is_empty());

        // Types should be distinct (different fresh variables).
        assert_ne!(outcome1.ty, outcome2.ty);
    }

    /// Test 2: infer_var_unbound_emits_t0501 — unbound variable produces
    /// T0501 diagnostic.
    #[test]
    fn infer_var_unbound_emits_t0501() {
        let mut types = TypeInterner::new();
        let mut env = TypeEnv::new();
        let mut subst = Subst::new();

        let mut ir = IrArena::new();
        let span = test_span(10);
        let id = ir.alloc(IrKind::Var, span);

        let outcome = infer_node(&ir, &mut types, &mut env, &mut subst, id);

        // Should emit one diagnostic with code T0501.
        assert_eq!(outcome.diagnostics.len(), 1);
        assert_eq!(outcome.diagnostics[0].code(), t_code(501));
        assert_eq!(outcome.diagnostics[0].primary_span(), Some(span));

        // Type should be Top (error recovery).
        assert_eq!(outcome.ty, types.top());
    }

    /// Test 3: infer_var_bound_returns_binding — bound variable looks up
    /// its type.
    #[test]
    fn infer_var_bound_returns_binding() {
        let mut types = TypeInterner::new();
        let mut env = TypeEnv::new();
        let mut subst = Subst::new();

        let mut ir = IrArena::new();

        // Bind symbol 5 to u64.
        let u64_ty = types.intern(Type::UInt(64));
        env.bind(5, u64_ty);

        // Create a Var node with byte_start = 5.
        let span = test_span(5);
        let id = ir.alloc(IrKind::Var, span);

        let outcome = infer_node(&ir, &mut types, &mut env, &mut subst, id);

        // Should have no diagnostics.
        assert!(outcome.diagnostics.is_empty());

        // Type should match the binding.
        assert_eq!(outcome.ty, u64_ty);
    }

    /// Test 4: infer_let_binds_in_env — Let updates env so a subsequent
    /// Var lookup succeeds.
    #[test]
    fn infer_let_binds_in_env() {
        let mut types = TypeInterner::new();
        let mut env = TypeEnv::new();
        let mut subst = Subst::new();

        let mut ir = IrArena::new();

        // Create a Let node with byte_start = 20.
        let span_let = test_span(20);
        let id_let = ir.alloc(IrKind::Let, span_let);

        // Infer the Let (binds symbol 20 to a fresh var).
        let outcome_let = infer_node(&ir, &mut types, &mut env, &mut subst, id_let);
        assert!(outcome_let.diagnostics.is_empty());

        // Type should be unit.
        assert_eq!(outcome_let.ty, types.unit());

        // Now create a Var with the same byte_start and verify lookup succeeds.
        let span_var = test_span(20);
        let id_var = ir.alloc(IrKind::Var, span_var);

        let outcome_var = infer_node(&ir, &mut types, &mut env, &mut subst, id_var);

        // Should have no diagnostics.
        assert!(outcome_var.diagnostics.is_empty());

        // Type should be a fresh variable (not unit, not Top). The fresh var
        // should be distinct from unit().
        assert_ne!(outcome_var.ty, types.unit());
        assert_ne!(outcome_var.ty, types.top());
    }

    /// Test 5: check_annotation_matching_types_no_diagnostic — unifying
    /// matching types produces no diagnostic.
    #[test]
    fn check_annotation_matching_types_no_diagnostic() {
        let mut types = TypeInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));
        let span = test_span(0);

        let diags = check_annotation(&mut types, &mut subst, u64_ty, u64_ty, span);

        assert!(diags.is_empty());
    }

    /// Test 6: check_annotation_mismatch_emits_diagnostic — unifying mismatched
    /// types emits a diagnostic with the unification error code (T0504 for kind mismatch).
    #[test]
    fn check_annotation_mismatch_emits_t0501() {
        let mut types = TypeInterner::new();
        let mut subst = Subst::new();

        let u64_ty = types.intern(Type::UInt(64));
        let bool_ty = types.bool_ty();
        let span = test_span(0);

        let diags = check_annotation(&mut types, &mut subst, u64_ty, bool_ty, span);

        assert_eq!(diags.len(), 1);
        // Kind mismatch (u64 vs bool) produces T0504 from unification.
        assert_eq!(diags[0].code(), t_code(504));
        assert_eq!(diags[0].primary_span(), Some(span));
    }

    /// Test 7: check_annotation_named_string_vs_u64 — Named type (fake
    /// "String") vs u64 emits a diagnostic (T0504 for kind mismatch).
    #[test]
    fn check_annotation_named_string_vs_u64() {
        let mut types = TypeInterner::new();
        let mut subst = Subst::new();

        let string_ty = types.intern(Type::Named {
            name: 10,
            args: vec![],
        });
        let u64_ty = types.intern(Type::UInt(64));
        let span = test_span(0);

        let diags = check_annotation(&mut types, &mut subst, string_ty, u64_ty, span);

        assert_eq!(diags.len(), 1);
        // Kind mismatch (Named vs UInt) produces T0504 from unification.
        assert_eq!(diags[0].code(), t_code(504));
    }
}
