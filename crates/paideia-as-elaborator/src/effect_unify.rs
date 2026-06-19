//! Effect-row unification at call sites.
//!
//! When a function declared with an effect row `!{...}` is called, the
//! caller's inferred row must match the declared row. Phase-1 delegates
//! to [`paideia_as_effects::unify`] (the row unifier from PR-30) and
//! wraps the result with the canonical diagnostic codes:
//!
//! - **F1105** — Inferred row does not match the declared row.
//! - **F1102** — Handler installation order error (handler depends on
//!   an effect not yet handled by an outer `with H handle E { … }`).

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_effects::{
    EffectInterner, EffectRow, RowDiff, RowVarId, Substitution, UnifyError, unify,
};

/// Diagnostic code for declared-vs-inferred row mismatch.
pub const F_ROW_MISMATCH: u16 = 1105;

/// Diagnostic code for handler-installation-order errors.
pub const F_HANDLER_ORDER: u16 = 1102;

/// Result of running [`unify_call_row`].
#[derive(Debug, Clone)]
pub struct CallUnifyOutcome {
    /// Substitution to apply at the call site (fresh-instantiation of
    /// row variables).
    pub subst: Substitution,
    /// Diagnostics emitted on mismatch.
    pub diagnostics: Vec<Diagnostic>,
}

/// Unify the caller's inferred row with the callee's declared row.
///
/// Phase-1: delegates to `paideia_as_effects::unify`. On `Mismatch`,
/// emits one **F1105** with the span of the call site. Row-polymorphic
/// callees with tail variables yield a substitution that maps the
/// callee's tail to the leftover caller effects (per the row unifier's
/// algorithm); the substitution is opaque to this function.
///
/// `instantiate_with_fresh` is the caller's responsibility — pass in a
/// **fresh** [`EffectRow`] that uses a unique [`RowVarId`] for the
/// callee's declared row before invoking this function so different
/// call sites don't collide on the same row variable.
#[must_use]
pub fn unify_call_row(declared: &EffectRow, inferred: &EffectRow, span: Span) -> CallUnifyOutcome {
    match unify(declared, inferred) {
        Ok(subst) => CallUnifyOutcome {
            subst,
            diagnostics: Vec::new(),
        },
        Err(UnifyError::Mismatch) => CallUnifyOutcome {
            subst: Substitution::new(),
            diagnostics: vec![row_mismatch_diag(declared, inferred, span)],
        },
    }
}

fn row_mismatch_diag(declared: &EffectRow, inferred: &EffectRow, span: Span) -> Diagnostic {
    let diff = RowDiff {
        expected: declared,
        got: inferred,
        name_for: None,
    };
    let diff_rendering = diff.render();
    let message = format!("effect-row mismatch at call site:\n{}", diff_rendering);
    Diagnostic::error(f_code(F_ROW_MISMATCH))
        .message(message)
        .with_span(span)
        .finish()
}

/// Instantiate a row variable in `declared` with a **fresh**
/// [`RowVarId`]. Returns the instantiated row.
///
/// Helper for callers walking IR who need to keep tail-variable ids
/// unique across call sites.
#[must_use]
pub fn instantiate_fresh_tail(declared: &EffectRow, fresh: RowVarId) -> EffectRow {
    EffectRow {
        fixed: declared.fixed.clone(),
        tail: declared.tail.map(|_| fresh),
    }
}

/// Combined call-site instantiation + unification.
///
/// At a call site, the callee's declared row (possibly polymorphic)
/// must be instantiated with a fresh row variable before unification
/// against the caller's inferred row. This function does both in one
/// step.
///
/// Returns the unification outcome, whose substitution maps the fresh
/// row variable to whatever the caller's row demands.
#[must_use]
pub fn call_site_instantiate_and_unify(
    callee_decl_row: &EffectRow,
    caller_inferred_row: &EffectRow,
    interner: &mut EffectInterner,
    span: Span,
) -> CallUnifyOutcome {
    let fresh = interner.fresh_row_var();
    let instantiated = instantiate_fresh_tail(callee_decl_row, fresh);
    unify_call_row(&instantiated, caller_inferred_row, span)
}

/// Validate the **installation order** of nested `with H handle E { ... }`
/// expressions. `outer` is the row of effects already handled by
/// outer `with` blocks; `inner_required` is the row of effects this
/// handler depends on (e.g., `H` uses operations from `E2`). If any
/// effect in `inner_required.fixed` is NOT in `outer.fixed`, emit one
/// **F1102** per missing effect, naming the unhandled effect.
#[must_use]
pub fn check_handler_order(
    outer: &EffectRow,
    inner_required: &EffectRow,
    span: Span,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for &eff in &inner_required.fixed {
        if !outer.fixed.contains(&eff) {
            diags.push(
                Diagnostic::error(f_code(F_HANDLER_ORDER))
                    .message(format!(
                        "handler installation order: this handler depends on effect {} \
                         which is not yet handled by an enclosing `with` block",
                        eff.get()
                    ))
                    .with_span(span)
                    .finish(),
            );
        }
    }
    diags
}

fn f_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::F, Severity::Error, n).expect("valid F code")
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;
    use paideia_as_effects::EffectId;

    fn eff(n: u32) -> EffectId {
        EffectId::new(n).unwrap()
    }
    fn row_var(n: u32) -> RowVarId {
        RowVarId::new(n).unwrap()
    }
    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }
    fn row(ids: &[u32], tail: Option<u32>) -> EffectRow {
        EffectRow::from_ids(ids.iter().map(|n| eff(*n)).collect(), tail.map(row_var))
    }

    // ── AC bullets ────────────────────────────────────────────────────

    #[test]
    fn body_within_declared_row_unifies_cleanly() {
        // declared !{Io}, inferred !{Io} — clean.
        let declared = row(&[1], None); // Io = 1
        let inferred = row(&[1], None);
        let out = unify_call_row(&declared, &inferred, span());
        assert!(out.diagnostics.is_empty());
    }

    #[test]
    fn body_with_extra_effect_emits_f1105() {
        // declared !{Io}, inferred !{Io, Ipc} — Ipc isn't declared.
        let declared = row(&[1], None);
        let inferred = row(&[1, 2], None); // Ipc = 2
        let out = unify_call_row(&declared, &inferred, span());
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code().number(), 1105);
    }

    #[test]
    fn row_polymorphic_instantiation_at_call_site() {
        // declared forall e. !{Io | e}, inferred !{Io, Ipc} —
        // unifier binds e ↦ {Ipc}.
        let declared = instantiate_fresh_tail(&row(&[1], Some(99)), row_var(42));
        let inferred = row(&[1, 2], None);
        let out = unify_call_row(&declared, &inferred, span());
        assert!(out.diagnostics.is_empty());
        // The substitution binds the fresh row var to {Ipc}.
        let bound = out.subst.bindings.get(&row_var(42)).unwrap();
        assert_eq!(bound.fixed.len(), 1);
        assert_eq!(bound.fixed[0], eff(2));
    }

    // ── Handler installation order ────────────────────────────────────

    #[test]
    fn handler_order_outer_handles_required_is_clean() {
        // outer handled {Io, Ipc}; inner handler needs {Io} — OK.
        let outer = row(&[1, 2], None);
        let inner_required = row(&[1], None);
        let diags = check_handler_order(&outer, &inner_required, span());
        assert!(diags.is_empty());
    }

    #[test]
    fn handler_order_missing_outer_dependency_emits_f1102() {
        // outer handled {Io}; inner needs {Ipc} — F1102.
        let outer = row(&[1], None);
        let inner_required = row(&[2], None);
        let diags = check_handler_order(&outer, &inner_required, span());
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), 1102);
    }

    #[test]
    fn handler_order_emits_one_diagnostic_per_missing_effect() {
        // outer handled {Io}; inner needs {Ipc, Net}.
        let outer = row(&[1], None);
        let inner_required = row(&[2, 3], None);
        let diags = check_handler_order(&outer, &inner_required, span());
        assert_eq!(diags.len(), 2);
        for d in diags {
            assert_eq!(d.code().number(), 1102);
        }
    }

    #[test]
    fn empty_inner_required_is_clean() {
        let outer = row(&[1], None);
        let inner_required = row(&[], None);
        let diags = check_handler_order(&outer, &inner_required, span());
        assert!(diags.is_empty());
    }

    #[test]
    fn instantiate_fresh_tail_replaces_only_tail_var() {
        let declared = row(&[1, 2], Some(7));
        let instantiated = instantiate_fresh_tail(&declared, row_var(99));
        assert_eq!(instantiated.fixed, declared.fixed);
        assert_eq!(instantiated.tail, Some(row_var(99)));
    }

    #[test]
    fn instantiate_fresh_tail_on_closed_row_is_a_noop() {
        let declared = row(&[1, 2], None);
        let instantiated = instantiate_fresh_tail(&declared, row_var(99));
        assert_eq!(instantiated.tail, None);
    }

    #[test]
    fn monomorphic_call_to_polymorphic_yields_f1105_with_diff() {
        // Build a polymorphic callee: (...) !{Io | r}
        // Call from a context with only {Net}
        // Expect F1105 whose message names Io or Net.
        let declared = row(&[1], Some(99)); // Io = 1, with tail r99
        let inferred = row(&[3], None); // Net = 3, closed
        let out = unify_call_row(&declared, &inferred, span());
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code().number(), 1105);
        let msg = out.diagnostics[0].message();
        // Should show the row diff: inferred has extra [3] (Net), declared is missing [3].
        assert!(msg.contains("1") || msg.contains("3"));
    }

    // ── call_site_instantiate_and_unify tests ────────────────────────────

    #[test]
    fn instantiates_and_unifies_polymorphic_callee_against_monomorphic_caller() {
        // Scenario 1: callee_decl = (Io | r), caller_inferred = (Io)
        // The callee is polymorphic and the caller has no extras. When we instantiate
        // with a fresh tail and unify, the fresh tail binds to the empty row
        // (representing "the caller's row doesn't add anything beyond Io").
        let mut interner = paideia_as_effects::EffectInterner::new();
        let callee_decl = row(&[1], Some(99)); // Io = 1, with tail r99
        let caller_inferred = row(&[1], None); // Io = 1, closed
        let out =
            call_site_instantiate_and_unify(&callee_decl, &caller_inferred, &mut interner, span());
        assert!(out.diagnostics.is_empty());
        // When both fixed sets are the same and caller is closed, the fresh var
        // is not bound (no extras to bind). Just verify clean unification.
        let fresh_var = RowVarId::new(1).unwrap();
        // Since there are no extras on either side, the fresh row var is not bound.
        assert!(!out.subst.bindings.contains_key(&fresh_var));
    }

    #[test]
    fn instantiates_against_caller_with_extra_effects() {
        // Scenario 2: callee_decl = (Io | r), caller_inferred = (Io, Net)
        // The caller has an extra effect (Net). When we instantiate with a fresh
        // tail and unify, the fresh tail binds to {Net}.
        let mut interner = paideia_as_effects::EffectInterner::new();
        let callee_decl = row(&[1], Some(99)); // Io = 1, with tail r99
        let caller_inferred = row(&[1, 2], None); // Io, Net = 1, 2; closed
        let out =
            call_site_instantiate_and_unify(&callee_decl, &caller_inferred, &mut interner, span());
        assert!(out.diagnostics.is_empty());
        // The fresh row var allocated should be bound to {Net}.
        let fresh_var = RowVarId::new(1).unwrap();
        let bound = out.subst.bindings.get(&fresh_var).unwrap();
        assert_eq!(bound.fixed.len(), 1);
        assert_eq!(bound.fixed[0], eff(2)); // Net = 2
        assert_eq!(bound.tail, None);
    }

    #[test]
    fn closed_caller_with_closed_callee_no_substitution() {
        // Scenario 3: callee_decl = {Io}, caller_inferred = {Io}
        // The callee is closed (no tail). instantiate_fresh_tail returns a row
        // with no tail (since the original had no tail to replace).
        // Unification of two identical closed rows succeeds with an empty substitution.
        // The fresh var was allocated but never appears in the instantiated row.
        let mut interner = paideia_as_effects::EffectInterner::new();
        let callee_decl = row(&[1], None); // Io = 1, closed
        let caller_inferred = row(&[1], None); // Io = 1, closed
        let out =
            call_site_instantiate_and_unify(&callee_decl, &caller_inferred, &mut interner, span());
        assert!(out.diagnostics.is_empty());
        // No fresh row var binding because the callee was closed.
        let fresh_var = RowVarId::new(1).unwrap();
        assert!(!out.subst.bindings.contains_key(&fresh_var));
    }

    #[test]
    fn f1105_diagnostic_message_contains_diff_rendering() {
        // Regression test: the F1105 diagnostic message should contain
        // the multi-line expected/got/diff rendering.
        // declared {Io, Net}, inferred {Io, Mmio}
        let declared = row(&[1, 2], None); // Io, Net
        let inferred = row(&[1, 3], None); // Io, Mmio

        let out = unify_call_row(&declared, &inferred, span());
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code().number(), 1105);

        let msg = out.diagnostics[0].message();
        // Check that the message contains the diff rendering components
        assert!(msg.contains("expected:"));
        assert!(msg.contains("got     :"));
        assert!(msg.contains("diff    :"));
        // Should contain the diff markers for the additions/removals
        assert!(msg.contains("+ 3") || msg.contains("Mmio"));
        assert!(msg.contains("- 2") || msg.contains("Net"));
    }
}
