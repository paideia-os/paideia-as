//! Pure-context effect guard.
//!
//! A function declared `!{}` (the empty effect row) is a pure context:
//! its body must not perform any operation that contributes to its
//! effect row. Per `custom-assembler.md` §4.3, the check is the
//! contrapositive of effect-row matching: any non-empty inferred row
//! inside a pure-declared scope is a `F1106` violation.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};
use paideia_as_effects::EffectRow;

/// Diagnostic code for effect in a pure context.
pub const F_PURE_VIOLATION: u16 = 1106;

/// Validate that `body_row` is empty (no fixed effects).
///
/// Emits one F1106 per effect remaining in the row. Tail variables are
/// ignored — closing the row to `!{}` is the unifier's job.
#[must_use]
pub fn check_pure(body_row: &EffectRow, declaration_span: Span) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for &eff in &body_row.fixed {
        diags.push(
            Diagnostic::error(f_code(F_PURE_VIOLATION))
                .message(format!(
                    "function declared `!{{}}` (pure) but body performs effect {}",
                    eff.get()
                ))
                .with_span(declaration_span)
                .finish(),
        );
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

    fn span(byte_start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, 1)
    }

    fn eff(n: u32) -> EffectId {
        EffectId::new(n).unwrap()
    }

    fn row(ids: &[u32]) -> EffectRow {
        EffectRow::from_ids(ids.iter().map(|n| eff(*n)).collect(), None)
    }

    #[test]
    fn empty_row_in_pure_context_passes() {
        assert!(check_pure(&row(&[]), span(0)).is_empty());
    }

    #[test]
    fn non_empty_row_emits_f1106_per_effect() {
        let diags = check_pure(&row(&[1, 2]), span(0));
        assert_eq!(diags.len(), 2);
        for d in &diags {
            assert_eq!(d.code().number(), 1106);
        }
    }

    #[test]
    fn tail_variable_alone_is_silent() {
        let row = EffectRow::from_ids(vec![], Some(paideia_as_effects::RowVarId::new(1).unwrap()));
        assert!(check_pure(&row, span(0)).is_empty());
    }
}
