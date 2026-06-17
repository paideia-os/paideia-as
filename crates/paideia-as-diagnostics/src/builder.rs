use crate::diagnostic::{Diagnostic, SecondarySpan, SuggestedFix};
use crate::{DiagnosticCode, Severity, Span};

/// A builder for constructing `Diagnostic` values.
///
/// Must be finished with `.finish()` to produce a `Diagnostic`.
/// All fields are initialized to defaults: `primary_span = None`, `secondary_spans = []`,
/// `message = ""`, `payload = null`, `suggestions = []`.
#[must_use = "DiagnosticBuilder must be finished with .finish() to produce a Diagnostic"]
pub struct DiagnosticBuilder {
    code: DiagnosticCode,
    severity: Severity,
    primary_span: Option<Span>,
    secondary_spans: Vec<SecondarySpan>,
    message: String,
    payload: serde_json::Value,
    suggestions: Vec<SuggestedFix>,
}

impl DiagnosticBuilder {
    /// Creates a new builder with the given code and severity.
    pub(crate) fn new(code: DiagnosticCode, severity: Severity) -> Self {
        Self {
            code,
            severity,
            primary_span: None,
            secondary_spans: vec![],
            message: String::new(),
            payload: serde_json::Value::Null,
            suggestions: vec![],
        }
    }

    /// Sets the diagnostic message.
    pub fn message(mut self, m: impl Into<String>) -> Self {
        self.message = m.into();
        self
    }

    /// Sets the primary source span.
    pub fn with_span(mut self, s: Span) -> Self {
        self.primary_span = Some(s);
        self
    }

    /// Adds a secondary span with a label.
    pub fn with_label(mut self, s: Span, l: impl Into<String>) -> Self {
        self.secondary_spans.push(SecondarySpan {
            span: s,
            label: l.into(),
        });
        self
    }

    /// Sets the JSON payload.
    pub fn with_payload(mut self, p: serde_json::Value) -> Self {
        self.payload = p;
        self
    }

    /// Adds a suggested fix.
    pub fn with_suggestion(mut self, fix: SuggestedFix) -> Self {
        self.suggestions.push(fix);
        self
    }

    /// Finishes building and returns the `Diagnostic`.
    pub fn finish(self) -> Diagnostic {
        Diagnostic {
            code: self.code,
            severity: self.severity,
            primary_span: self.primary_span,
            secondary_spans: self.secondary_spans,
            message: self.message,
            payload: self.payload,
            suggestions: self.suggestions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::Category;

    fn sample_code() -> DiagnosticCode {
        DiagnosticCode::new(Category::E, Severity::Error, 1).unwrap()
    }

    fn sample_span() -> Span {
        Span::new(crate::FileId::new(1).unwrap(), 0, 5)
    }

    #[test]
    fn builder_produces_e0001_full() {
        let code = sample_code();
        let primary_span = sample_span();
        let secondary_span = Span::new(crate::FileId::new(1).unwrap(), 10, 3);

        let diag = Diagnostic::error(code)
            .message("syntax error")
            .with_span(primary_span)
            .with_label(secondary_span, "first defined here")
            .with_payload(serde_json::json!({"a": 1}))
            .with_suggestion(SuggestedFix {
                span: primary_span,
                replacement: "x".into(),
                description: "rename".into(),
            })
            .finish();

        assert_eq!(diag.code(), code);
        assert_eq!(diag.severity(), Severity::Error);
        assert_eq!(diag.primary_span(), Some(primary_span));
        assert_eq!(diag.secondary_spans().len(), 1);
        assert_eq!(diag.secondary_spans()[0].span, secondary_span);
        assert_eq!(diag.secondary_spans()[0].label, "first defined here");
        assert_eq!(diag.message(), "syntax error");
        assert_eq!(diag.payload(), &serde_json::json!({"a": 1}));
        assert_eq!(diag.suggestions().len(), 1);
        assert_eq!(diag.suggestions()[0].replacement, "x");
        assert_eq!(diag.suggestions()[0].description, "rename");
    }

    #[test]
    fn summary_format() {
        let code = sample_code();
        let summary = Diagnostic::error(code).message("x").finish().summary();
        assert_eq!(summary, "E0001: x");
    }

    #[test]
    fn serde_round_trip() {
        let code = sample_code();
        let primary_span = sample_span();
        let payload_json = serde_json::json!({"foo": 1, "bar": [true, "baz"]});

        let diag = Diagnostic::error(code)
            .message("test message")
            .with_span(primary_span)
            .with_payload(payload_json.clone())
            .finish();

        let serialized = serde_json::to_string(&diag).expect("serialize failed");
        let deserialized: Diagnostic =
            serde_json::from_str(&serialized).expect("deserialize failed");

        assert_eq!(deserialized.code(), diag.code());
        assert_eq!(deserialized.severity(), diag.severity());
        assert_eq!(deserialized.primary_span(), diag.primary_span());
        assert_eq!(
            deserialized.secondary_spans().len(),
            diag.secondary_spans().len()
        );
        assert_eq!(deserialized.message(), diag.message());
        assert_eq!(deserialized.payload(), diag.payload());
        assert_eq!(deserialized.suggestions().len(), diag.suggestions().len());
    }

    #[test]
    fn severity_inherited_from_constructor() {
        let error_code = DiagnosticCode::new(Category::E, Severity::Error, 1).unwrap();
        let diag = Diagnostic::warning(error_code).finish();
        assert_eq!(diag.severity(), Severity::Warning);
    }

    #[test]
    fn secondary_spans_accumulate() {
        let code = sample_code();
        let span1 = Span::new(crate::FileId::new(1).unwrap(), 0, 5);
        let span2 = Span::new(crate::FileId::new(1).unwrap(), 10, 3);
        let span3 = Span::new(crate::FileId::new(1).unwrap(), 20, 7);

        let diag = Diagnostic::error(code)
            .with_label(span1, "first")
            .with_label(span2, "second")
            .with_label(span3, "third")
            .finish();

        assert_eq!(diag.secondary_spans().len(), 3);
        assert_eq!(diag.secondary_spans()[0].label, "first");
        assert_eq!(diag.secondary_spans()[1].label, "second");
        assert_eq!(diag.secondary_spans()[2].label, "third");
    }

    #[test]
    fn default_payload_is_null() {
        let code = sample_code();
        let diag = Diagnostic::error(code).finish();
        assert_eq!(diag.payload(), &serde_json::Value::Null);
    }

    #[test]
    fn default_message_empty_and_summary() {
        let code = sample_code();
        let summary = Diagnostic::error(code).finish().summary();
        assert_eq!(summary, "E0001: ");
    }
}
