//! Context carried through an IR walk.
//!
//! Bundles per-pass mutable state — source map, diagnostic sink — so that
//! per-pass functions can be called with a single context handle. Provides
//! accessors to allow passes to stash their own state on the side.
//!
//! Phase-2-m1 minimum: source map and diagnostic sink. The type environment
//! (`paideia-as-types`), lexical scope (`paideia-as-elaborator::env::TypeEnv`),
//! and linearity context (`paideia-as-elaborator::LinearityCtx`) are NOT part
//! of `WalkerCtx` itself — they live where the per-pass owns them. Passes can
//! access a generic `pass_state<S>()` slot if needed for future phases.

use paideia_as_diagnostics::{Diagnostic, DiagnosticSink, SourceMap};

/// Context carried through an IR walk.
///
/// Per-pass implementations access their state via accessors so the struct
/// can grow without touching every pass. Exposes the shared infrastructure
/// every pass needs: source map and diagnostic sink.
pub struct WalkerCtx<'a> {
    source_map: &'a SourceMap,
    sink: &'a mut dyn DiagnosticSink,
}

impl<'a> WalkerCtx<'a> {
    /// Creates a new walker context with the given source map and sink.
    ///
    /// # Arguments
    ///
    /// * `source_map` - Reference to the source map for the compilation unit.
    /// * `sink` - Mutable reference to the diagnostic sink.
    pub fn new(source_map: &'a SourceMap, sink: &'a mut dyn DiagnosticSink) -> Self {
        Self { source_map, sink }
    }

    /// Returns a reference to the source map.
    pub fn source_map(&self) -> &SourceMap {
        self.source_map
    }

    /// Returns a mutable reference to the diagnostic sink.
    pub fn sink(&mut self) -> &mut dyn DiagnosticSink {
        self.sink
    }

    /// Emits a diagnostic to the sink.
    pub fn emit(&mut self, d: Diagnostic) {
        let _ = self.sink.emit(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, SourceMap, VecSink};

    #[test]
    fn ctx_exposes_source_map() {
        let mut sm = SourceMap::new();
        let file_id = sm.add_file(std::path::PathBuf::from("test.pdx"), "let x = 1".into());

        let mut sink = VecSink::new();
        let ctx = WalkerCtx::new(&sm, &mut sink);

        // Verify that ctx.source_map() returns the same reference
        let returned_sm = ctx.source_map();
        assert_eq!(returned_sm.content(file_id), "let x = 1");
    }

    #[test]
    fn ctx_emit_routes_to_sink() {
        use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity, Span};

        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        // Create and emit a diagnostic
        let code = DiagnosticCode::new(Category::Z, Severity::Warning, 9001).unwrap();
        let file_id = FileId::new(1).unwrap();
        let span = Span::new(file_id, 0, 1);
        let diagnostic = Diagnostic::warning(code)
            .message("test diagnostic")
            .with_span(span)
            .finish();

        ctx.emit(diagnostic);

        // Verify the diagnostic reached the sink
        assert_eq!(sink.count(), 1, "exactly one diagnostic should be in sink");
        assert_eq!(
            sink.diagnostics()[0].severity(),
            Severity::Warning,
            "diagnostic should have Warning severity"
        );
    }
}
