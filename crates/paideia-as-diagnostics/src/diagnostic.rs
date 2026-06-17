use crate::builder::DiagnosticBuilder;
use crate::{DiagnosticCode, Severity, Span};
use serde::{Deserialize, Serialize};

/// A secondary source location annotated with a label.
///
/// Provides additional context locations within the diagnostic message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecondarySpan {
    /// The source position.
    pub span: Span,
    /// Human-readable label for this location.
    pub label: String,
}

/// A suggested fix for a diagnostic.
///
/// Combines a source span, replacement text, and a description of the fix.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuggestedFix {
    /// The span to replace.
    pub span: Span,
    /// The replacement text.
    pub replacement: String,
    /// Human-readable description of the suggested fix.
    pub description: String,
}

/// A diagnostic message emitted by any paideia-as pass.
///
/// Diagnostic includes a stable code, severity, primary and secondary source spans,
/// a freeform message, a JSON payload for structured data, and optional suggestions.
/// Construct via `Diagnostic::error()`, `::warning()`, etc. and `DiagnosticBuilder`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    pub(crate) code: DiagnosticCode,
    pub(crate) severity: Severity,
    pub(crate) primary_span: Option<Span>,
    pub(crate) secondary_spans: Vec<SecondarySpan>,
    pub(crate) message: String,
    pub(crate) payload: serde_json::Value,
    pub(crate) suggestions: Vec<SuggestedFix>,
}

impl Diagnostic {
    /// Starts building an error-severity diagnostic.
    pub fn error(code: DiagnosticCode) -> DiagnosticBuilder {
        DiagnosticBuilder::new(code, Severity::Error)
    }

    /// Starts building a warning-severity diagnostic.
    pub fn warning(code: DiagnosticCode) -> DiagnosticBuilder {
        DiagnosticBuilder::new(code, Severity::Warning)
    }

    /// Starts building a note-severity diagnostic.
    pub fn note(code: DiagnosticCode) -> DiagnosticBuilder {
        DiagnosticBuilder::new(code, Severity::Note)
    }

    /// Starts building a hint-severity diagnostic.
    pub fn hint(code: DiagnosticCode) -> DiagnosticBuilder {
        DiagnosticBuilder::new(code, Severity::Hint)
    }

    /// Starts building a lint-severity diagnostic.
    pub fn lint(code: DiagnosticCode) -> DiagnosticBuilder {
        DiagnosticBuilder::new(code, Severity::Lint)
    }

    /// Returns the diagnostic code.
    #[must_use]
    pub fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// Returns the severity level.
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// Returns the primary source span, if set.
    #[must_use]
    pub fn primary_span(&self) -> Option<Span> {
        self.primary_span
    }

    /// Returns all secondary spans with labels.
    #[must_use]
    pub fn secondary_spans(&self) -> &[SecondarySpan] {
        &self.secondary_spans
    }

    /// Returns the diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the JSON payload.
    #[must_use]
    pub fn payload(&self) -> &serde_json::Value {
        &self.payload
    }

    /// Returns all suggested fixes.
    #[must_use]
    pub fn suggestions(&self) -> &[SuggestedFix] {
        &self.suggestions
    }

    /// Returns a one-line summary in the form "Cxxxx: message".
    #[must_use]
    pub fn summary(&self) -> String {
        format!("{}: {}", self.code, self.message)
    }
}
