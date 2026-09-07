//! Diagnostic constructors shared across the term evaluator's dispatch arms.
//!
//! Split out from the single-file `term_eval.rs` in the phase-2 God-file
//! refactor (issue #1412 under umbrella #1405). Each helper builds a
//! `Diagnostic` under the T (elaborator) or M (macro) category with the
//! exact wording and codes the original evaluator emitted.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

use super::value::Value;

/// Create a diagnostic for an undefined identifier.
pub(super) fn undef_ident_diag(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::new(Category::T, Severity::Error, 500).expect("valid T code"))
        .message(format!("undefined identifier: {}", name))
        .with_span(span)
        .finish()
}

/// Create a diagnostic for a type mismatch.
pub(super) fn type_mismatch_diag(expected: &str, got: &Value, span: Span) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::new(Category::T, Severity::Error, 501).expect("valid T code"))
        .message(format!(
            "type mismatch: expected {} but got {}",
            expected,
            got.display()
        ))
        .with_span(span)
        .finish()
}

/// Create a diagnostic for a non-exhaustive match.
pub(super) fn nonexhaustive_match_diag(span: Span) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::new(Category::T, Severity::Error, 502).expect("valid T code"))
        .message("non-exhaustive pattern match: no arm matched")
        .with_span(span)
        .finish()
}

/// Create a diagnostic for fuel exhaustion in the evaluator.
pub(super) fn fuel_exhausted_diag(span: Span, steps_taken: u64) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::new(Category::M, Severity::Error, 311).expect("valid M code"))
        .message(format!(
            "macro evaluation: fuel exhausted after {} steps (infinite loop?)",
            steps_taken
        ))
        .with_span(span)
        .finish()
}

/// Create a diagnostic for stack depth limit exceeded in the evaluator.
pub(super) fn depth_exceeded_diag(span: Span, depth: u32) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::new(Category::M, Severity::Error, 311).expect("valid M code"))
        .message(format!(
            "macro evaluation: stack depth {} exceeded (unbounded recursion?)",
            depth
        ))
        .with_span(span)
        .finish()
}
