//! Effect-row inference engine for function types.
//!
//! Provides a small analytical module that downstream IR-walking passes call to
//! compose, subtract, and validate effect rows during type and effect inference.
//! This module does NOT yet wire into a full inference walker — that bridge
//! lands when the IR carries enough structure. See `design/toolchain/custom-assembler.md` §4.2.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_effects::{EffectId, EffectInterner, EffectRow};

#[cfg(test)]
use paideia_as_effects::RowVarId;

/// Diagnostic code for an effect that escapes its handler chain.
pub const F_UNHANDLED_EFFECT: u16 = 1100;

/// One row-inference outcome containing a computed effect row and any diagnostics.
///
/// This is the return type for effect-analysis functions, allowing composition
/// of multiple sub-expression analyses with accumulated diagnostics.
#[derive(Clone, Debug)]
pub struct RowOutcome {
    /// The computed effect row.
    pub row: EffectRow,
    /// Diagnostics generated during analysis (e.g., unhandled effects).
    pub diagnostics: Vec<Diagnostic>,
}

/// Effect row for `perform <Effect>.<op>(args)`.
///
/// A `perform` expression contributes the named effect to the surrounding row.
/// Returns a singleton row containing only that effect.
///
/// # Example
/// ```ignore
/// perform_row(Io) → { Io }
/// ```
pub fn perform_row(effect: EffectId) -> EffectRow {
    EffectRow::from_ids(vec![effect], None)
}

/// Effect row for a function call.
///
/// The callee's declared effect row IS the contribution to the caller's row.
/// The caller composes the argument rows separately using `compose_rows`.
///
/// # Example
/// ```ignore
/// call_row({Io}) → {Io}
/// ```
pub fn call_row(callee_row: EffectRow) -> EffectRow {
    callee_row
}

/// Effect row for `with <handler> handle <Effect> { body }`.
///
/// Removes the handled effect from the body's fixed set, leaving the tail
/// variable intact. This preserves row-polymorphism: if the body produces
/// `{Io, Mmio | e}` and we handle `Io`, the result is `{Mmio | e}`, allowing
/// outer handlers or callers to manage the remaining effects and free variable.
///
/// # Invariants
/// - If the handled effect is in the fixed set, it is removed.
/// - The tail variable is always preserved (row-polymorphism).
/// - Unrelated effects are left unchanged.
///
/// # Example
/// ```ignore
/// handle_row({Io, Mmio}, Io) → {Mmio}
/// handle_row({Io}, Io) → {}
/// handle_row({Io | e}, Io) → { | e}  // tail preserved
/// handle_row({Io}, Mmio) → {Io}      // no change if handler doesn't match
/// ```
pub fn handle_row(body_row: &EffectRow, handled: EffectId) -> EffectRow {
    let fixed: Vec<EffectId> = body_row
        .fixed
        .iter()
        .copied()
        .filter(|e| *e != handled)
        .collect();
    EffectRow {
        fixed,
        tail: body_row.tail,
    }
}

/// Compose rows from sub-expressions (sequence, arguments, branches).
///
/// Returns the union of fixed effects from both rows. If either row has a tail
/// variable, the result preserves a tail (preferring the first row's if both exist).
/// This is idempotent: composing `{Io}` with itself yields `{Io}`.
///
/// # Example
/// ```ignore
/// compose_rows({Io}, {Mmio}) → {Io, Mmio}
/// compose_rows({Io}, {Io}) → {Io}
/// ```
pub fn compose_rows(a: &EffectRow, b: &EffectRow) -> EffectRow {
    a.union(b)
}

/// Validate that an inferred row at the top level is empty.
///
/// Every effect must be handled by the time control reaches the program's boundary.
/// Returns one F1100 diagnostic per remaining fixed effect. Ignores tail variables
/// (row-polymorphism at the top level is handled by unification, not by this layer).
///
/// # Diagnostics
/// For each unhandled fixed effect, emits one F1100 error at `program_span`.
///
/// # Example
/// ```ignore
/// check_no_unhandled({}) → []
/// check_no_unhandled({Io, Mmio}) → [F1100, F1100]
/// check_no_unhandled({| e}) → []  // free variable is OK; closed-row unification happens later
/// ```
pub fn check_no_unhandled(final_row: &EffectRow, program_span: Span) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for &eff in &final_row.fixed {
        diags.push(
            Diagnostic::error(f_code(F_UNHANDLED_EFFECT))
                .message(format!(
                    "effect {} escapes the program without a handler",
                    eff.get(),
                ))
                .with_span(program_span)
                .finish(),
        );
    }
    diags
}

/// Generalize an inferred effect row at a function declaration's exit.
///
/// If `row` is closed (`is_closed()`) AND the function declaration didn't
/// explicitly annotate `!{}` (pure), allocate a fresh `RowVarId` and
/// attach it to the row. This makes the function row-polymorphic at
/// every later call site.
///
/// If `row` has a tail already, return as-is (already polymorphic).
/// If `row` was explicitly pure-annotated, return as-is.
///
/// # Example
/// ```ignore
/// generalize_row({Io}, interner, false) → {Io | fresh}
/// generalize_row({Io | r1}, interner, false) → {Io | r1}  // already open
/// generalize_row({}, interner, true) → {}  // explicitly pure
/// ```
pub fn generalize_row(
    row: &EffectRow,
    interner: &mut EffectInterner,
    explicitly_pure: bool,
) -> EffectRow {
    if row.tail.is_some() || explicitly_pure {
        return row.clone();
    }
    let fresh = interner.fresh_row_var();
    EffectRow {
        fixed: row.fixed.clone(),
        tail: Some(fresh),
    }
}

/// Helper to construct an F-category error diagnostic code.
fn f_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::F, Severity::Error, n).expect("valid F code")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: Construct an EffectId from a positive integer.
    fn eff(n: u32) -> EffectId {
        EffectId::new(n).expect("effect id")
    }

    /// Helper: Construct a Span for testing.
    fn span() -> Span {
        Span::new(paideia_as_diagnostics::FileId::new(1).unwrap(), 0, 1)
    }

    /// AC 1: perform_row contributes a singleton row.
    #[test]
    fn perform_contributes_singleton_row() {
        let io = eff(1);
        let row = perform_row(io);

        assert_eq!(row.fixed, vec![io]);
        assert!(row.tail.is_none());
    }

    /// AC 2: call_row passes through the callee's row.
    #[test]
    fn call_passes_through_callee_row() {
        let io = eff(1);
        let callee_row = EffectRow::from_ids(vec![io], None);
        let result = call_row(callee_row.clone());

        assert_eq!(result.fixed, vec![io]);
        assert_eq!(result.tail, callee_row.tail);
    }

    /// AC 3: handle_row removes one effect.
    #[test]
    fn handle_removes_one_effect() {
        let io = eff(1);
        let mmio = eff(2);
        let row = EffectRow::from_ids(vec![io, mmio], None);

        let result = handle_row(&row, io);

        // Should contain only mmio.
        assert_eq!(result.fixed, vec![mmio]);
        assert!(result.tail.is_none());
    }

    /// AC 3b: handle_row on a singleton row removes the effect.
    #[test]
    fn handle_removes_singleton_effect() {
        let io = eff(1);
        let row = EffectRow::from_ids(vec![io], None);

        let result = handle_row(&row, io);

        assert!(result.fixed.is_empty());
        assert!(result.tail.is_none());
    }

    /// handle_row preserves the tail variable.
    #[test]
    fn handle_preserves_tail_variable() {
        let io = eff(1);
        let e = RowVarId::new(1).unwrap();
        let row = EffectRow::from_ids(vec![io], Some(e));

        let result = handle_row(&row, io);

        assert!(result.fixed.is_empty());
        assert_eq!(result.tail, Some(e));
    }

    /// compose_rows unions the effects.
    #[test]
    fn compose_rows_unions() {
        let io = eff(1);
        let mmio = eff(2);
        let row_a = EffectRow::from_ids(vec![io], None);
        let row_b = EffectRow::from_ids(vec![mmio], None);

        let result = compose_rows(&row_a, &row_b);

        assert_eq!(result.fixed.len(), 2);
        assert!(result.fixed.contains(&io));
        assert!(result.fixed.contains(&mmio));
    }

    /// compose_rows is idempotent.
    #[test]
    fn compose_rows_idempotent() {
        let io = eff(1);
        let row = EffectRow::from_ids(vec![io], None);

        let result = compose_rows(&row, &row);

        assert_eq!(result.fixed, vec![io]);
        assert!(result.tail.is_none());
    }

    /// check_no_unhandled on empty row produces no diagnostics.
    #[test]
    fn check_no_unhandled_empty_row_is_clean() {
        let row = EffectRow::empty();
        let s = span();

        let diags = check_no_unhandled(&row, s);

        assert!(diags.is_empty());
    }

    /// AC 4: check_no_unhandled emits F1100 per remaining effect.
    #[test]
    fn check_no_unhandled_emits_f1100_per_effect() {
        let io = eff(1);
        let mmio = eff(2);
        let row = EffectRow::from_ids(vec![io, mmio], None);
        let s = span();

        let diags = check_no_unhandled(&row, s);

        assert_eq!(diags.len(), 2);
        for diag in &diags {
            assert_eq!(diag.code().number(), F_UNHANDLED_EFFECT);
            assert_eq!(diag.code().category(), Category::F);
            assert_eq!(diag.severity(), Severity::Error);
            assert_eq!(diag.primary_span(), Some(s));
            assert!(diag.message().contains("escapes the program"));
        }
    }

    /// check_no_unhandled ignores tail variables.
    #[test]
    fn check_no_unhandled_ignores_tail_variable() {
        let e = RowVarId::new(1).unwrap();
        let row = EffectRow::from_ids(vec![], Some(e));
        let s = span();

        let diags = check_no_unhandled(&row, s);

        // Free row variable is acceptable; closed-row unification handles it later.
        assert!(diags.is_empty());
    }

    /// handle_row on an unrelated effect is a no-op.
    #[test]
    fn handle_unrelated_effect_is_noop() {
        let io = eff(1);
        let mmio = eff(2);
        let row = EffectRow::from_ids(vec![io], None);

        let result = handle_row(&row, mmio);

        // Should be unchanged.
        assert_eq!(result.fixed, vec![io]);
        assert!(result.tail.is_none());
    }

    /// perform + handle round trip removes the effect.
    #[test]
    fn perform_then_handle_round_trip() {
        let io = eff(1);
        let r = perform_row(io);

        let result = handle_row(&r, io);

        assert!(result.fixed.is_empty());
        assert!(result.tail.is_none());
    }

    /// compose with complex tail scenario.
    #[test]
    fn compose_rows_with_tail_preference() {
        let io = eff(1);
        let mmio = eff(2);
        let e1 = RowVarId::new(1).unwrap();
        let e2 = RowVarId::new(2).unwrap();

        let row_a = EffectRow::from_ids(vec![io], Some(e1));
        let row_b = EffectRow::from_ids(vec![mmio], Some(e2));

        let result = compose_rows(&row_a, &row_b);

        // Should contain both effects, and prefer row_a's tail.
        assert_eq!(result.fixed.len(), 2);
        assert!(result.fixed.contains(&io));
        assert!(result.fixed.contains(&mmio));
        assert_eq!(result.tail, Some(e1));
    }

    /// Multiple effects are properly sorted in handle_row result.
    #[test]
    fn handle_row_maintains_sorted_order() {
        let e1 = eff(1);
        let e2 = eff(2);
        let e3 = eff(3);
        let row = EffectRow::from_ids(vec![e3, e1, e2], None);

        let result = handle_row(&row, e2);

        // Should contain e1 and e3, sorted.
        assert_eq!(result.fixed.len(), 2);
        assert_eq!(result.fixed[0], e1);
        assert_eq!(result.fixed[1], e3);
    }

    /// Diagnostic message is informative.
    #[test]
    fn check_no_unhandled_diagnostic_message() {
        let io = eff(42);
        let row = EffectRow::from_ids(vec![io], None);
        let s = span();

        let diags = check_no_unhandled(&row, s);

        assert_eq!(diags.len(), 1);
        let msg = diags[0].message();
        assert!(msg.contains("42"));
        assert!(msg.contains("escapes"));
    }

    /// generalize_closed_row_attaches_fresh_tail: {Io} → {Io | fresh}
    #[test]
    fn generalize_closed_row_attaches_fresh_tail() {
        let io = eff(1);
        let row = EffectRow::from_ids(vec![io], None);
        let mut interner = paideia_as_effects::EffectInterner::new();

        let generalized = generalize_row(&row, &mut interner, false);

        // Should have same fixed effects.
        assert_eq!(generalized.fixed, vec![io]);
        // Should now have a tail.
        assert!(generalized.tail.is_some());
        // Original row is unchanged.
        assert!(row.tail.is_none());
    }

    /// generalize_already_open_row_unchanged: {Io | r1} → {Io | r1}
    #[test]
    fn generalize_already_open_row_unchanged() {
        let io = eff(1);
        let r1 = RowVarId::new(1).unwrap();
        let row = EffectRow::from_ids(vec![io], Some(r1));
        let mut interner = paideia_as_effects::EffectInterner::new();

        let generalized = generalize_row(&row, &mut interner, false);

        assert_eq!(generalized.fixed, vec![io]);
        assert_eq!(generalized.tail, Some(r1));
    }

    /// generalize_explicitly_pure_unchanged: {} → {}
    #[test]
    fn generalize_explicitly_pure_unchanged() {
        let row = EffectRow::empty();
        let mut interner = paideia_as_effects::EffectInterner::new();

        let generalized = generalize_row(&row, &mut interner, true);

        assert!(generalized.is_empty());
        assert!(generalized.tail.is_none());
    }

    /// generalize_uses_unique_id_per_call: two calls produce distinct row vars
    #[test]
    fn generalize_uses_unique_id_per_call() {
        let io = eff(1);
        let row1 = EffectRow::from_ids(vec![io], None);
        let row2 = EffectRow::from_ids(vec![io], None);
        let mut interner = paideia_as_effects::EffectInterner::new();

        let gen1 = generalize_row(&row1, &mut interner, false);
        let gen2 = generalize_row(&row2, &mut interner, false);

        // Both should have tails.
        assert!(gen1.tail.is_some());
        assert!(gen2.tail.is_some());
        // But the tails should be distinct.
        assert_ne!(gen1.tail, gen2.tail);
    }
}
