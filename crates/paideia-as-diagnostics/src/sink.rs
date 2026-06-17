//! Diagnostic sinks and bail policy.
//!
//! A [`DiagnosticSink`] consumes [`Diagnostic`]s emitted by paideia-as
//! passes. Concrete sinks: [`VecSink`] (collect), [`HumanSink`] (render
//! human format and write to a `Write`), [`SarifSink`] (buffer + emit
//! SARIF on `finish`), [`MultiSink`] (fan out).
//!
//! Bail discipline: per `syntax-reference.md` §2.4 the lexer (and other
//! passes) bail after 100 *errors* (not total diagnostics). Configured
//! via [`BailPolicy::cap(100)`]; warnings/notes/hints/lints do not
//! consume budget.

use crate::Severity;
use crate::diagnostic::Diagnostic;
use crate::render_human::HumanRenderer;
use crate::sarif::SarifEmitter;
use std::io;

/// Error indicating that a diagnostic sink has exceeded its error budget.
///
/// Per `syntax-reference.md` §2.4, only errors consume budget; warnings,
/// notes, hints, and lints do not.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("diagnostic budget exceeded: error count exceeded cap of {limit}")]
pub struct DiagnosticOverflow {
    /// The error cap that was exceeded.
    pub limit: usize,
}

/// Bail policy for diagnostic sinks.
///
/// Configures the maximum number of *errors* (not total diagnostics)
/// that a sink will accept before returning `Err(DiagnosticOverflow)`.
/// Warnings, notes, hints, and lints do not consume budget.
#[derive(Debug, Clone, Copy)]
pub struct BailPolicy {
    cap: usize,
}

impl BailPolicy {
    /// Creates a new bail policy with the given error cap.
    ///
    /// Per `syntax-reference.md` §2.4, only errors consume budget.
    #[must_use]
    pub fn cap(cap: usize) -> Self {
        Self { cap }
    }

    /// Creates a bail policy with no error limit.
    #[must_use]
    pub fn unlimited() -> Self {
        Self { cap: usize::MAX }
    }

    /// Returns true if the cap is exceeded (i.e., the error budget is depleted).
    ///
    /// Per `syntax-reference.md` §2.4, only errors consume budget;
    /// warnings/notes/hints/lints are exempt. This check should be called
    /// *after* incrementing the error count. With cap(100), the first 100
    /// errors succeed; the 101st error (current_error_count=101) triggers
    /// overflow.
    pub fn check(&self, current_error_count: usize) -> bool {
        current_error_count > self.cap
    }

    /// Returns the error cap for this policy.
    #[must_use]
    pub fn limit(&self) -> usize {
        self.cap
    }
}

/// Trait for consuming diagnostics emitted by paideia-as passes.
///
/// Concrete implementations handle rendering, buffering, or fanning out
/// to multiple sinks. Implementations must track error count and enforce
/// the bail policy.
pub trait DiagnosticSink {
    /// Emits a diagnostic to this sink.
    ///
    /// Returns `Err(DiagnosticOverflow)` if the emit would exceed the
    /// error cap. The diagnostic is still recorded in the sink
    /// (i.e., the emit has a side effect even on overflow).
    fn emit(&mut self, diagnostic: Diagnostic) -> Result<(), DiagnosticOverflow>;

    /// Returns the total number of diagnostics emitted to this sink.
    fn count(&self) -> usize;

    /// Returns the total number of error diagnostics emitted to this sink.
    fn error_count(&self) -> usize;
}

/// A sink that collects diagnostics into a vector.
///
/// Useful for buffering diagnostics in memory for later inspection or
/// batch processing.
#[derive(Debug)]
pub struct VecSink {
    diagnostics: Vec<Diagnostic>,
    errors: usize,
    policy: BailPolicy,
}

impl VecSink {
    /// Creates a new vector sink with no error limit.
    #[must_use]
    pub fn new() -> Self {
        Self::with_policy(BailPolicy::unlimited())
    }

    /// Creates a new vector sink with the given bail policy.
    #[must_use]
    pub fn with_policy(policy: BailPolicy) -> Self {
        Self {
            diagnostics: vec![],
            errors: 0,
            policy,
        }
    }

    /// Returns a reference to the buffered diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Consumes this sink and returns the buffered diagnostics.
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

impl Default for VecSink {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnosticSink for VecSink {
    fn emit(&mut self, d: Diagnostic) -> Result<(), DiagnosticOverflow> {
        let is_error = d.severity() == Severity::Error;
        self.diagnostics.push(d);
        if is_error {
            self.errors += 1;
            if self.policy.check(self.errors) {
                return Err(DiagnosticOverflow {
                    limit: self.policy.cap,
                });
            }
        }
        Ok(())
    }

    fn count(&self) -> usize {
        self.diagnostics.len()
    }

    fn error_count(&self) -> usize {
        self.errors
    }
}

/// A sink that renders diagnostics to human-readable text and writes to a writer.
///
/// Each diagnostic is rendered using a `HumanRenderer` and written immediately.
/// I/O panics by design in phase-1; production hardening will add fallible I/O later.
pub struct HumanSink<'a, W: io::Write> {
    writer: W,
    renderer: HumanRenderer<'a>,
    count: usize,
    errors: usize,
    policy: BailPolicy,
}

impl<'a, W: io::Write> HumanSink<'a, W> {
    /// Creates a new human sink with the given writer and renderer, using an unlimited error cap.
    #[must_use]
    pub fn new(writer: W, renderer: HumanRenderer<'a>) -> Self {
        Self::with_policy(writer, renderer, BailPolicy::unlimited())
    }

    /// Creates a new human sink with the given writer, renderer, and bail policy.
    #[must_use]
    pub fn with_policy(writer: W, renderer: HumanRenderer<'a>, policy: BailPolicy) -> Self {
        Self {
            writer,
            renderer,
            count: 0,
            errors: 0,
            policy,
        }
    }

    /// Returns a reference to the underlying writer.
    #[must_use]
    pub fn writer(&self) -> &W {
        &self.writer
    }

    /// Consumes this sink and returns the underlying writer.
    pub fn into_writer(self) -> W {
        self.writer
    }
}

impl<W: io::Write> DiagnosticSink for HumanSink<'_, W> {
    fn emit(&mut self, d: Diagnostic) -> Result<(), DiagnosticOverflow> {
        let is_error = d.severity() == Severity::Error;
        let rendered = self.renderer.render(&d);
        writeln!(self.writer, "{}", rendered).expect("io write to HumanSink");
        self.count += 1;
        if is_error {
            self.errors += 1;
            if self.policy.check(self.errors) {
                return Err(DiagnosticOverflow {
                    limit: self.policy.cap,
                });
            }
        }
        Ok(())
    }

    fn count(&self) -> usize {
        self.count
    }

    fn error_count(&self) -> usize {
        self.errors
    }
}

/// A sink that buffers diagnostics and emits them to SARIF JSON on explicit `finish()`.
///
/// Diagnostics are buffered in memory. SARIF JSON output is produced only when
/// `finish()` is called explicitly. Note: `Drop` does NOT flush the writer
/// — callers must call `finish()` to write the output.
pub struct SarifSink<'a, W: io::Write> {
    writer: W,
    emitter: SarifEmitter<'a>,
    diagnostics: Vec<Diagnostic>,
    errors: usize,
    policy: BailPolicy,
}

impl<'a, W: io::Write> SarifSink<'a, W> {
    /// Creates a new SARIF sink with the given writer and emitter, using an unlimited error cap.
    #[must_use]
    pub fn new(writer: W, emitter: SarifEmitter<'a>) -> Self {
        Self::with_policy(writer, emitter, BailPolicy::unlimited())
    }

    /// Creates a new SARIF sink with the given writer, emitter, and bail policy.
    #[must_use]
    pub fn with_policy(writer: W, emitter: SarifEmitter<'a>, policy: BailPolicy) -> Self {
        Self {
            writer,
            emitter,
            diagnostics: vec![],
            errors: 0,
            policy,
        }
    }

    /// Serializes the buffered diagnostics to SARIF JSON and flushes to the writer.
    ///
    /// This must be called explicitly. Note that `Drop` does NOT flush
    /// — if this method is not called, no output will be written.
    pub fn finish(mut self) -> io::Result<()> {
        let json = self.emitter.emit_string(&self.diagnostics);
        self.writer.write_all(json.as_bytes())
    }
}

impl<W: io::Write> DiagnosticSink for SarifSink<'_, W> {
    fn emit(&mut self, d: Diagnostic) -> Result<(), DiagnosticOverflow> {
        let is_error = d.severity() == Severity::Error;
        self.diagnostics.push(d);
        if is_error {
            self.errors += 1;
            if self.policy.check(self.errors) {
                return Err(DiagnosticOverflow {
                    limit: self.policy.cap,
                });
            }
        }
        Ok(())
    }

    fn count(&self) -> usize {
        self.diagnostics.len()
    }

    fn error_count(&self) -> usize {
        self.errors
    }
}

/// A sink that fans out emissions to multiple inner sinks.
///
/// Each diagnostic is cloned and emitted to every inner sink. If any inner
/// sink returns `Err(DiagnosticOverflow)`, `MultiSink` returns
/// `Err(DiagnosticOverflow { limit: 0 })` to indicate an aggregate overflow
/// (even if all inner sinks recorded the diagnostic).
pub struct MultiSink<'a> {
    sinks: Vec<&'a mut dyn DiagnosticSink>,
}

impl<'a> MultiSink<'a> {
    /// Creates a new empty multi-sink.
    #[must_use]
    pub fn new() -> Self {
        Self { sinks: vec![] }
    }

    /// Adds an inner sink to fan out to.
    ///
    /// Each diagnostic will be cloned and emitted to this sink.
    pub fn push(&mut self, sink: &'a mut dyn DiagnosticSink) {
        self.sinks.push(sink);
    }
}

impl Default for MultiSink<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnosticSink for MultiSink<'_> {
    fn emit(&mut self, d: Diagnostic) -> Result<(), DiagnosticOverflow> {
        let mut overflowed = false;
        for sink in self.sinks.iter_mut() {
            // Each inner sink gets its own clone.
            if sink.emit(d.clone()).is_err() {
                overflowed = true;
            }
        }
        if overflowed {
            Err(DiagnosticOverflow { limit: 0 }) // aggregate sentinel
        } else {
            Ok(())
        }
    }

    fn count(&self) -> usize {
        self.sinks.iter().map(|s| s.count()).max().unwrap_or(0)
    }

    fn error_count(&self) -> usize {
        self.sinks
            .iter()
            .map(|s| s.error_count())
            .max()
            .unwrap_or(0)
    }
}
