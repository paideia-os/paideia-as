//! R220.M9 substrate — hosted-DSL diagnostic router (relocated from
//! `paideia_lsp::dsl_embed` in R220.M12 close-out).
//!
//! **Why this crate hosts the router.**  R220.M9 originally landed the
//! [`DslDiagnosticHandle`] type + process-wide slot in `paideia-lsp` and
//! deferred the elaborator-side dispatcher wiring, because the elaborator
//! cannot depend on `paideia-lsp` (paideia-lsp itself depends on the
//! elaborator — a cycle).  R220.M12 breaks that stalemate by moving the
//! router API here, into `paideia-as-reflection`, which both the
//! elaborator and paideia-lsp already depend on.  `paideia-lsp`
//! re-exports the API for source-compatibility with the M9 surface, and
//! keeps only the LSP-specific translator
//! (`to_lsp_diagnostics`) plus the router-side interleave helper.
//!
//! **What lives here.**
//!
//! * [`DslDiagnosticHandle`] — cloneable `Arc<Mutex<VecSink>>` sink a
//!   hosted DSL emits into via [`emit_elab_error`](DslDiagnosticHandle::emit_elab_error)
//!   / [`emit_elab_warn`](DslDiagnosticHandle::emit_elab_warn) / [`emit_note`](DslDiagnosticHandle::emit_note);
//! * [`install_router_handle`] + [`current_handle`] +
//!   [`with_current_handle`] — the process-wide slot + scoped override
//!   the elaborator's `Elab.elab_error` / `Elab.elab_warn` builtin-call
//!   dispatchers read (see `paideia-as-elaborator` `term_eval::call`);
//! * [`hosted_error_code`] / [`hosted_warn_code`] / [`hosted_note_code`]
//!   — mint hosted-DSL codes in the reserved `Z9000..=Z9099` band.
//!
//! **What does *not* live here** (kept in `paideia-lsp`):
//! `to_lsp_diagnostics` (needs the LSP-side `Diagnostic` translator) and
//! `interleave_by_span` (a router-side merge helper — no `paideia-lsp`
//! type dep, but conceptually router-shaped).

use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock};

use paideia_as_diagnostics::{
    Category, Diagnostic, DiagnosticCode, DiagnosticOverflow, DiagnosticSink, Severity, Span,
    VecSink,
};

use crate::elab_effect::{ElabError, ElabWarn};

// -------- Hosted-DSL code minters --------

/// Lower bound of the hosted-DSL sub-range within the `Z` category.
///
/// Matches the R220.M9 plan wording (`E9000+` / `W9000+` / `N9000+`); on
/// the wire the codes render as `Z9000`..`Z9099` — severity is not part
/// of the wire form per `design/toolchain/diagnostics.md` §1 (DI-D1).
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

    /// Emit a hosted-DSL note from a plain message + span.
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
}

impl Default for DslDiagnosticHandle {
    fn default() -> Self {
        Self::new()
    }
}

// -------- Router-side registration (elaborator-facing) --------

/// Process-wide slot the LSP router / `paideia-as-check` populates once
/// at startup so a hosted DSL (or the elaborator forwarding on its
/// behalf) can look up "the handle" without an explicit thread-through.
static ROUTER_HANDLE: OnceLock<DslDiagnosticHandle> = OnceLock::new();

/// Install a router-wide handle for hosted-DSL diagnostics.  Idempotent
/// — subsequent calls are a no-op and return `false`; the first-set
/// handle is what every future [`current_handle`] returns unless an
/// override [`with_current_handle`] scope is active.
///
/// Called once at LSP server / `paideia-as-check` startup.
pub fn install_router_handle(handle: DslDiagnosticHandle) -> bool {
    ROUTER_HANDLE.set(handle).is_ok()
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
/// thread.  Restores the previous slot on drop (panic-safe).  Nesting is
/// supported; the outer handle is restored when the inner scope ends.
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
        let r = std::panic::catch_unwind(|| hosted_error_code(8999));
        assert!(r.is_err());
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
    fn drain_leaves_handle_empty() {
        let h = DslDiagnosticHandle::new();
        h.emit_note("hello", span_at(0, 1), hosted_note_code(9050))
            .unwrap();
        assert_eq!(h.len(), 1);
        let drained = h.drain();
        assert_eq!(drained.len(), 1);
        assert!(h.is_empty());
    }
}
