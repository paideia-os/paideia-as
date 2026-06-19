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
use paideia_as_effects::{EffectId, EffectRow, RowVarId, Substitution, UnifyError, unify};

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
    let extra: Vec<EffectId> = inferred
        .fixed
        .iter()
        .copied()
        .filter(|e| !declared.fixed.contains(e))
        .collect();
    let missing: Vec<EffectId> = declared
        .fixed
        .iter()
        .copied()
        .filter(|e| !inferred.fixed.contains(e))
        .collect();
    let extra_str: Vec<u32> = extra.iter().map(|e| e.get()).collect();
    let missing_str: Vec<u32> = missing.iter().map(|e| e.get()).collect();
    Diagnostic::error(f_code(F_ROW_MISMATCH))
        .message(format!(
            "effect-row mismatch at call site: declared row missing {extra_str:?}, \
             inferred row missing {missing_str:?}"
        ))
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
}
