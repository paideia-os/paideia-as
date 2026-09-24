//! R220.M9 — LSP-embed API for hosted DSLs (thin adapter over the
//! reflection-side router).
//!
//! **R220.M12 relocation.**  The router API (handle type, process-wide
//! slot, thread-local override, hosted-code minters) moved out of this
//! module into `paideia_as_reflection::dsl_diag` so the elaborator's
//! `Elab.elab_error` / `Elab.elab_warn` builtin-call dispatchers can
//! reach it without a `paideia-lsp` dep (the elaborator is a downstream
//! dep of `paideia-lsp`, not the other way around — a cycle).
//!
//! This module now:
//!
//! * **re-exports** the router API for source-compatibility with the
//!   M9 landing surface (existing callers keep writing
//!   `paideia_lsp::install_router_handle(...)` etc. verbatim);
//! * defines the two genuinely LSP-shaped helpers — `to_lsp_diagnostics`
//!   (drains a handle into LSP-shaped diagnostics via the existing
//!   `to_lsp_diagnostic` translator) and `interleave_by_span` (merges
//!   native + hosted diagnostic streams before `publish_diagnostics`).

pub use paideia_as_reflection::{
    DslDiagnosticHandle, HOSTED_DSL_CODE_MAX, HOSTED_DSL_CODE_MIN, current_handle,
    hosted_error_code, hosted_note_code, hosted_warn_code, install_router_handle,
    with_current_handle,
};

use paideia_as_diagnostics::Diagnostic;
use tower_lsp::lsp_types::Diagnostic as LspDiagnostic;

use crate::diagnostics::to_lsp_diagnostic;

/// Translate every buffered diagnostic in `handle` to LSP shape via the
/// existing native-diagnostic translator.  Hosted diagnostics land in
/// the editor identically to native ones (same shape, same severity
/// mapping, same `source: paideia-as`).
///
/// This is a snapshot — the handle is not drained.  The LSP router
/// typically calls [`DslDiagnosticHandle::drain`] instead, merges the
/// vector with the native pass output via [`interleave_by_span`], then
/// maps the merged vector through [`crate::diagnostics::to_lsp_diagnostic`].
#[must_use]
pub fn to_lsp_diagnostics(handle: &DslDiagnosticHandle, source_text: &str) -> Vec<LspDiagnostic> {
    handle
        .snapshot()
        .iter()
        .map(|d| to_lsp_diagnostic(d, source_text))
        .collect()
}

/// Merge a native-diagnostic slice and a hosted-diagnostic slice into
/// one sorted stream, ordered by primary-span byte-start (diagnostics
/// without a primary span sort first, in original order).  Native and
/// hosted diagnostics interleave deterministically — the LSP router
/// uses this to hand a single ordered vector to `publish_diagnostics`.
///
/// Stable sort: equal keys preserve input order — natives before hosted
/// at the same byte offset.
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
    use paideia_as_diagnostics::{Category, DiagnosticCode, FileId, Severity, Span};

    fn span_at(byte_start: u32, byte_len: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, byte_len)
    }

    #[test]
    fn to_lsp_diagnostics_translates_snapshot() {
        let h = DslDiagnosticHandle::new();
        h.emit_note("hi", span_at(0, 2), hosted_note_code(9001))
            .unwrap();
        let lsp = to_lsp_diagnostics(&h, "abc");
        assert_eq!(lsp.len(), 1);
        assert_eq!(lsp[0].message, "hi");
        // Snapshot, not drain.
        assert_eq!(h.len(), 1);
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

    #[test]
    fn interleave_stable_at_equal_offsets() {
        let native = Diagnostic::error(
            DiagnosticCode::new(Category::P, Severity::Error, 100).unwrap(),
        )
        .message("native at 3")
        .with_span(span_at(3, 1))
        .finish();
        let hosted = Diagnostic::error(hosted_error_code(9001))
            .message("hosted at 3")
            .with_span(span_at(3, 1))
            .finish();
        let merged = interleave_by_span(&[native], &[hosted]);
        assert_eq!(merged[0].message(), "native at 3");
        assert_eq!(merged[1].message(), "hosted at 3");
    }
}
