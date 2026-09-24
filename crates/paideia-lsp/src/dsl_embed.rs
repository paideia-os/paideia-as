//! R220.M9 — LSP-embed API for hosted DSLs.
//!
//! A hosted DSL (declared via `@dsl_parser` in R220.M3) needs to surface
//! its own elaboration diagnostics through the same LSP pipe the native
//! elaborator uses.  This module lands the in-process handle
//! ([`DslDiagnosticHandle`]) that:
//!
//! * accepts [`ElabError`] / [`ElabWarn`] payloads from
//!   `paideia-as-reflection` (R220.M1), plus a plain-message note path;
//! * translates each payload into a fully-formed
//!   [`paideia_as_diagnostics::Diagnostic`] carrying the hosted DSL's
//!   diagnostic code, severity, primary span, and message — the same
//!   value type native passes emit;
//! * buffers those diagnostics behind an [`Arc<Mutex<VecSink>>`], so the
//!   LSP router can drain them alongside the native diagnostics it
//!   already renders through [`SarifEmitter`];
//! * exposes an LSP-facing adapter (`to_lsp_diagnostics`) that reuses
//!   the existing `to_lsp_diagnostic` translator verbatim — hosted
//!   diagnostics reach the editor byte-identically to native ones.
//!
//! ## Diagnostic code range
//!
//! Hosted DSLs claim numbers in the paideia-as `Z` (experimental /
//! catch-all) category, range `9000..=9099`.  The three helpers
//! [`hosted_error_code`], [`hosted_warn_code`], [`hosted_note_code`]
//! mint canonical [`DiagnosticCode`]s from that range with the intended
//! severity.  The task briefing spells these as `E9001` / `W9001` /
//! `N9001`; internally the wire form is `Z9001` (severity is metadata,
//! not part of the wire form — see `diagnostics.md` DI-D1).  A hosted
//! DSL that wants a stable printable prefix can wrap the code with its
//! own display; the LSP surface reports `Z9001` today.
//!
//! ## Thread-through vs. thread-local
//!
//! The task asks for `Option<&DslDiagnosticHandle>` plumbing until
//! R220.M3 lands `@dsl_parser`.  We land both:
//!
//! * `DslDiagnosticHandle` is `Clone` + `Send` + `Sync` — the router
//!   can hand it explicitly to any pipeline stage that opts in;
//! * [`current_handle`] / [`with_current_handle`] provide a scoped
//!   thread-local slot the R220.M3 `@dsl_parser` runtime can read
//!   without threading the handle through every builtin-call frame.
//!
//! Both paths hit the same backing sink; both are optional for callers
//! that don't need the DSL surface.
//!
//! ## Scope cap
//!
//! Wiring the handle into `cmd_check` / the real LSP server startup is
//! deferred: R220.M3 (`@dsl_parser`) is the caller that first *needs*
//! the wiring, and it lands in parallel.  This module ships the
//! constructor + register + emit + drain API today; the eventual
//! server-side call to [`install_router_handle`] is a one-line
//! follow-on documented at that function.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock};

use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticOverflow, DiagnosticSink, Severity, Span,
    VecSink,
};
use paideia_as_reflection::{ElabError, ElabWarn};
use tower_lsp::lsp_types::Diagnostic as LspDiagnostic;

use crate::diagnostics::to_lsp_diagnostic;

// -------- Hosted-DSL code minters --------

/// Lower bound of the hosted-DSL sub-range within the `Z` category.
///
/// Matches the plan wording (`E9000+` / `W9000+` / `N9000+`); on the
/// wire the codes render as `Z9000`..`Z9099` — severity is not part of
/// the wire form per `design/toolchain/diagnostics.md` §1 (DI-D1).
pub const HOSTED_DSL_CODE_MIN: u16 = 9000;

/// Upper bound (inclusive) of the hosted-DSL sub-range within `Z`.
pub const HOSTED_DSL_CODE_MAX: u16 = 9099;

/// Mint an error-severity hosted-DSL diagnostic code.
///
/// Panics if `n` is outside `9000..=9099` (the R220.M9 hosted-DSL
/// reservation within category `Z`).
#[must_use]
pub fn hosted_error_code(n: u16) -> DiagnosticCode {
    assert!(
        (HOSTED_DSL_CODE_MIN..=HOSTED_DSL_CODE_MAX).contains(&n),
        "hosted_error_code: {n} outside R220.M9 reservation \
         ({HOSTED_DSL_CODE_MIN}..={HOSTED_DSL_CODE_MAX})"
    );
    DiagnosticCode::new(Category::Z, Severity::Error, n).expect("Z9000..=Z9099 valid")
}

/// Mint a warning-severity hosted-DSL diagnostic code.
#[must_use]
pub fn hosted_warn_code(n: u16) -> DiagnosticCode {
    assert!(
        (HOSTED_DSL_CODE_MIN..=HOSTED_DSL_CODE_MAX).contains(&n),
        "hosted_warn_code: {n} outside R220.M9 reservation \
         ({HOSTED_DSL_CODE_MIN}..={HOSTED_DSL_CODE_MAX})"
    );
    DiagnosticCode::new(Category::Z, Severity::Warning, n).expect("Z9000..=Z9099 valid")
}

/// Mint a note-severity hosted-DSL diagnostic code.
#[must_use]
pub fn hosted_note_code(n: u16) -> DiagnosticCode {
    assert!(
        (HOSTED_DSL_CODE_MIN..=HOSTED_DSL_CODE_MAX).contains(&n),
        "hosted_note_code: {n} outside R220.M9 reservation \
         ({HOSTED_DSL_CODE_MIN}..={HOSTED_DSL_CODE_MAX})"
    );
    DiagnosticCode::new(Category::Z, Severity::Note, n).expect("Z9000..=Z9099 valid")
}

// -------- The handle --------

/// Shared, cloneable handle to a diagnostic sink dedicated to hosted-DSL
/// output.  Every clone points at the same underlying [`VecSink`], so
/// diagnostics emitted from any thread appear in a single ordered
/// buffer the LSP router can drain.
///
/// Construction: [`DslDiagnosticHandle::new`] creates a fresh buffer;
/// [`DslDiagnosticHandle::from_sink`] wraps a caller-supplied sink so
/// the router can share one buffer with a `MultiSink` pipeline if it
/// prefers.
#[derive(Clone)]
pub struct DslDiagnosticHandle {
    inner: Arc<Mutex<VecSink>>,
}

impl DslDiagnosticHandle {
    /// Creates a handle backed by a fresh, unlimited-cap [`VecSink`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecSink::new())),
        }
    }

    /// Wraps an externally-owned sink.  All hosted-DSL emissions land in
    /// `sink`; callers keep their existing handle to drain from.
    #[must_use]
    pub fn from_sink(sink: Arc<Mutex<VecSink>>) -> Self {
        Self { inner: sink }
    }

    /// Emit a hosted-DSL error whose payload was constructed on the
    /// reflection side.
    ///
    /// The [`ElabError`] payload provides `message` + `span`; the caller
    /// supplies the diagnostic code (typically minted via
    /// [`hosted_error_code`]).  Returns the sink's overflow signal as
    /// forwarded from the underlying [`VecSink`] — hosted DSLs get the
    /// same bail-policy contract native passes do.
    pub fn emit_elab_error(
        &self,
        err: &ElabError,
        code: DiagnosticCode,
    ) -> Result<(), DiagnosticOverflow> {
        let diag = Diagnostic::error(code)
            .message(err.message.clone())
            .with_span(err.span)
            .finish();
        self.push(diag)
    }

    /// Emit a hosted-DSL warning whose payload was constructed on the
    /// reflection side.
    pub fn emit_elab_warn(
        &self,
        warn: &ElabWarn,
        code: DiagnosticCode,
    ) -> Result<(), DiagnosticOverflow> {
        let diag = Diagnostic::warning(code)
            .message(warn.message.clone())
            .with_span(warn.span)
            .finish();
        self.push(diag)
    }

    /// Emit a hosted-DSL note from a plain message + span (no reflection
    /// payload counterpart today — R220.M1 only defines `ElabError` /
    /// `ElabWarn`).
    pub fn emit_note(
        &self,
        message: impl Into<String>,
        span: Span,
        code: DiagnosticCode,
    ) -> Result<(), DiagnosticOverflow> {
        let diag = Diagnostic::note(code)
            .message(message)
            .with_span(span)
            .finish();
        self.push(diag)
    }

    /// Push a fully-formed [`Diagnostic`] — the escape hatch for
    /// hosted-DSL implementations that already built a diagnostic and
    /// want it in the same buffer.
    pub fn push(&self, diag: Diagnostic) -> Result<(), DiagnosticOverflow> {
        let mut guard = self.inner.lock().expect("DslDiagnosticHandle poisoned");
        guard.emit(diag)
    }

    /// Drain the buffered diagnostics, leaving the handle empty.
    pub fn drain(&self) -> Vec<Diagnostic> {
        let mut guard = self.inner.lock().expect("DslDiagnosticHandle poisoned");
        let taken = std::mem::replace(&mut *guard, VecSink::new());
        taken.into_diagnostics()
    }

    /// Snapshot the currently buffered diagnostics without draining.
    #[must_use]
    pub fn snapshot(&self) -> Vec<Diagnostic> {
        let guard = self.inner.lock().expect("DslDiagnosticHandle poisoned");
        guard.diagnostics().to_vec()
    }

    /// Count of currently buffered diagnostics.
    #[must_use]
    pub fn len(&self) -> usize {
        let guard = self.inner.lock().expect("DslDiagnosticHandle poisoned");
        guard.count()
    }

    /// True when no diagnostics are buffered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Translate every buffered diagnostic to LSP shape via the
    /// existing native-diagnostic translator.  Hosted diagnostics land
    /// in the editor identically to native ones (same shape, same
    /// severity mapping, same `source: paideia-as`).
    #[must_use]
    pub fn to_lsp_diagnostics(&self, source_text: &str) -> Vec<LspDiagnostic> {
        self.snapshot()
            .iter()
            .map(|d| to_lsp_diagnostic(d, source_text))
            .collect()
    }
}

impl Default for DslDiagnosticHandle {
    fn default() -> Self {
        Self::new()
    }
}

// -------- Router-side registration (M3-facing) --------

/// Process-wide slot the LSP router populates once at startup so a
/// hosted DSL can look up "the handle" without an explicit thread-through.
///
/// R220.M3 (`@dsl_parser`) is the first consumer; the elaborator's
/// builtin-call dispatcher will call [`current_handle`] on entry to
/// `Elab.elab_error` / `Elab.elab_warn` and, if `Some`, forward the
/// payload here.
static ROUTER_HANDLE: OnceLock<DslDiagnosticHandle> = OnceLock::new();

/// Install a router-wide handle for hosted-DSL diagnostics.  Idempotent
/// — subsequent calls are a no-op and return `false`; the first-set
/// handle is what every future [`current_handle`] returns unless an
/// override [`with_current_handle`] scope is active.
///
/// Called once at LSP server / `paideia-as-check` startup.  Wiring into
/// the server binary is a follow-on when R220.M3 lands.
pub fn install_router_handle(handle: DslDiagnosticHandle) -> bool {
    ROUTER_HANDLE.set(handle).is_ok()
}

/// Test-only reset for the router-wide slot.  Not exposed outside
/// `cfg(test)` — production code sets the slot exactly once.
#[cfg(test)]
fn _reset_router_handle_for_test() {
    // OnceLock has no `take` on stable; tests use a fresh scoped
    // override (`with_current_handle`) instead of touching the slot.
    // This function exists as a marker documenting the intentional
    // choice.
}

thread_local! {
    static SCOPED_HANDLE: RefCell<Option<DslDiagnosticHandle>> = const { RefCell::new(None) };
}

/// Look up the ambient hosted-DSL handle.  Prefers a scoped override
/// (set via [`with_current_handle`]) and falls back to the process-wide
/// router handle installed by [`install_router_handle`].
#[must_use]
pub fn current_handle() -> Option<DslDiagnosticHandle> {
    let scoped = SCOPED_HANDLE.with(|slot| slot.borrow().clone());
    if scoped.is_some() {
        return scoped;
    }
    ROUTER_HANDLE.get().cloned()
}

/// Run `f` with `handle` installed as the ambient handle on the current
/// thread.  Restores the previous slot on unwind.  Nesting is supported;
/// the outer handle is restored when the inner scope ends.
pub fn with_current_handle<T, F>(handle: DslDiagnosticHandle, f: F) -> T
where
    F: FnOnce() -> T,
{
    struct Restore(Option<DslDiagnosticHandle>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let prev = self.0.take();
            SCOPED_HANDLE.with(|slot| *slot.borrow_mut() = prev);
        }
    }

    let prev = SCOPED_HANDLE.with(|slot| slot.replace(Some(handle)));
    let _restore = Restore(prev);
    f()
}

// -------- Interleaving helper --------

/// Merge a native-diagnostic slice and a hosted-diagnostic slice into
/// one sorted stream, ordered by primary-span byte-start (diagnostics
/// without a primary span sort first, in original order).  Native and
/// hosted diagnostics interleave deterministically — the LSP router
/// uses this to hand a single ordered vector to `publish_diagnostics`.
///
/// Stable sort: equal keys preserve input order — natives before
/// hosted at the same byte offset.
#[must_use]
pub fn interleave_by_span(native: &[Diagnostic], hosted: &[Diagnostic]) -> Vec<Diagnostic> {
    let mut merged: Vec<Diagnostic> = native.iter().cloned().collect();
    merged.extend(hosted.iter().cloned());
    merged.sort_by_key(|d| d.primary_span().map(|s| s.byte_start()).unwrap_or(0));
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span_at(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
    }

    #[test]
    fn handle_starts_empty() {
        let h = DslDiagnosticHandle::new();
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
    }

    #[test]
    fn emit_elab_error_records_diagnostic() {
        let h = DslDiagnosticHandle::new();
        let err = ElabError {
            message: "numeric literal out of range".into(),
            span: span_at(0, 4),
        };
        h.emit_elab_error(&err, hosted_error_code(9001)).unwrap();

        let snap = h.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].code().category(), Category::Z);
        assert_eq!(snap[0].code().number(), 9001);
        assert_eq!(snap[0].severity(), Severity::Error);
        assert_eq!(snap[0].message(), "numeric literal out of range");
    }

    #[test]
    fn hosted_code_helpers_reject_out_of_range() {
        // Below range.
        let r = std::panic::catch_unwind(|| hosted_error_code(8999));
        assert!(r.is_err());
        // Above range.
        let r = std::panic::catch_unwind(|| hosted_warn_code(9100));
        assert!(r.is_err());
    }

    #[test]
    fn scoped_handle_isolated_from_router() {
        let scoped = DslDiagnosticHandle::new();
        with_current_handle(scoped.clone(), || {
            let via_lookup = current_handle().expect("scoped handle visible");
            let err = ElabError {
                message: "inner".into(),
                span: span_at(0, 1),
            };
            via_lookup
                .emit_elab_error(&err, hosted_error_code(9042))
                .unwrap();
        });
        assert_eq!(scoped.len(), 1);
    }

    #[test]
    fn interleave_sorts_by_span_start() {
        let native = Diagnostic::error(
            DiagnosticCode::new(Category::P, Severity::Error, 100).unwrap(),
        )
        .message("native at 5")
        .with_span(span_at(5, 1))
        .finish();
        let hosted = Diagnostic::error(hosted_error_code(9001))
            .message("hosted at 2")
            .with_span(span_at(2, 1))
            .finish();
        let merged = interleave_by_span(&[native], &[hosted]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].primary_span().unwrap().byte_start(), 2);
        assert_eq!(merged[1].primary_span().unwrap().byte_start(), 5);
    }
}
