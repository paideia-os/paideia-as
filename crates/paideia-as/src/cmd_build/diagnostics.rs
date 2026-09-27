//! Common diagnostic finisher for encoder + emitter build errors.
//! Split out of `cmd_build.rs` (2026-07-08).
//!
//! Phase 8 m1-004: Helper builders for typed diagnostics routed through DiagnosticSink.
//! - encoder_error / encoder_warn: B1705/B1706 for encoding failures
//! - symbol_layout_invalid: B1703 for emitter validation
//! - function_symbol_no_offset: B1704 for missing function offsets
//! - span_of: Lookup IR node span from arena
//!
//! Wave 29 (paideia-as#1553): the `unresolved_label` (U1610) builder and
//! its `node_for_fixup` reverse-lookup helper were retired here — the
//! fixup pass can no longer reach an unresolved label from user syntax
//! (the elaborator's `process_stmt` guard catches every user-authored
//! case), and a stray fixup at that stage is now an elaborator ICE panic
//! rather than a user diagnostic. See `cmd_build/fixup.rs`.

use std::path::Path;
use std::process::ExitCode;

use std::str::FromStr;

use paideia_as_diagnostics::{Catalog, Diagnostic, DiagnosticCode, DiagnosticSink, HumanRenderer, HumanSink, SourceMap, Span, VecSink};
use paideia_as_ir::{IrArena, IrNodeId, InstructionSideTable};

use crate::cmd_common;
use super::BuildError;

pub(super) fn finish_build_error(
    source_map: &SourceMap,
    catalog: &Catalog,
    sink: VecSink,
    _error: BuildError,
    _input: &Path,
    sarif: Option<&Path>,
) -> ExitCode {
    let diagnostics = sink.into_diagnostics();

    // Render human diagnostics to stderr (drive-by for symmetry).
    let stderr = std::io::stderr();
    let renderer = HumanRenderer::with_catalog(source_map, crate::color::should_use_color(), catalog);
    let mut human = HumanSink::new(stderr.lock(), renderer);
    for d in &diagnostics {
        let _ = human.emit(d.clone());
    }

    // Write SARIF if requested, even on encoder/emitter failure.
    if let Some(path) = sarif {
        let _ = cmd_common::write_sarif(source_map, catalog, &diagnostics, path);
    }

    // Phase 8 m1-004: BuildError::Failed is a marker only.
    // Diagnostics have already been emitted through sink at the error site.
    ExitCode::from(2)
}

/// Extract the source span for an IR node from the arena.
/// Used to populate physicalLocation in SARIF output.
pub(super) fn span_of(arena: &IrArena, node: IrNodeId) -> Option<Span> {
    // Phase 8 m1-004: Lookup IR node span via arena.get().
    // Returns Some(span) if the node exists and has recorded source info,
    // None otherwise (internal nodes, rewritten nodes, etc.).
    arena.get(node).map(|node_data| node_data.span)
}

/// Find the first (earliest) instruction in the table that likely caused an encoder failure.
/// Since instructions are encoded sequentially, the failure is usually on the first one.
/// Returns Some(node_id) if found, None if the table is empty.
///
/// Phase-6-m1-004: Used to attribute encoder errors to a specific IR node for diagnostics.
pub(super) fn find_failing_instruction(instruction_table: &InstructionSideTable) -> Option<IrNodeId> {
    let mut entries: Vec<_> = instruction_table.entries().iter().collect();
    entries.sort_by_key(|&(&node_id, _)| node_id);
    entries.first().map(|&(&node_id, _)| node_id)
}

/// Build a typed B1705 encoder-error diagnostic.
pub(super) fn encoder_error(
    _node: IrNodeId,
    message: &str,
    span: Option<Span>,
) -> Diagnostic {
    // Phase 8 m1-004: B1705 fires when the encoder encounters a failure.
    let code = DiagnosticCode::from_str("B1705").expect("B1705 is a valid code");
    let mut diag = Diagnostic::error(code).message(message);
    if let Some(s) = span {
        diag = diag.with_span(s);
    }
    diag.finish()
}

/// Build a typed B1706 encoder-warn diagnostic.
pub(super) fn encoder_warn(
    _node: IrNodeId,
    message: &str,
    span: Option<Span>,
) -> Diagnostic {
    // Phase 8 m1-004: B1706 fires when the encoder encounters a warning
    // that does not abort the build (e.g., with --encoder-warn).
    let code = DiagnosticCode::from_str("B1706").expect("B1706 is a valid code");
    let mut diag = Diagnostic::warning(code).message(message);
    if let Some(s) = span {
        diag = diag.with_span(s);
    }
    diag.finish()
}

// Wave 29 (paideia-as#1553): removed `unresolved_label` (U1610 builder).
// The only call site was `patch_label_fixups` in `cmd_build/fixup.rs`,
// which no longer emits user diagnostics on the unresolved branch — see
// that module's header for the reachability argument. The elaborator's
// own U1610 emission in `paideia_as_elaborator::unsafe_walker::process_stmt`
// remains the canonical (and only) source of this code.

/// Build a typed B1703 symbol-layout-invalid diagnostic.
pub(super) fn symbol_layout_invalid(message: &str) -> Diagnostic {
    // Phase 7 m1-002: B1703 fires when symbol layout validation fails.
    let code = DiagnosticCode::from_str("B1703").expect("B1703 is a valid code");
    Diagnostic::error(code).message(message).finish()
}

/// Build a typed B1704 function-symbol-no-offset diagnostic.
///
/// #1261: dropped the `ir_node N` suffix (meaningless outside the
/// compiler's memory) and reframed as an ICE with a bug-report URL —
/// this only fires when the encoder failed to record an offset it was
/// supposed to, which is always an internal invariant violation.
pub(super) fn function_symbol_no_offset(name: &str, _ir_node: u32) -> Diagnostic {
    let code = DiagnosticCode::from_str("B1704").expect("B1704 is a valid code");
    let msg = format!(
        "internal compiler error: no byte-offset recorded for function symbol \
         `{}` — the object will link with st_value=0 and st_size=0. \
         please file a bug at https://github.com/paideia-os/paideia-as/issues \
         with the offending .pdx source.",
        name
    );
    Diagnostic::warning(code).message(&msg).finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{IrKind, IrArena};

    #[test]
    fn span_of_returns_arena_span_for_encoder_node() {
        // Phase 8 m1-004: Verify span_of returns the correct span for a node.
        let mut arena = IrArena::new();
        let file_id = paideia_as_diagnostics::FileId::new(1).expect("FileId 1 is valid");
        let test_span = Span::new(file_id, 10, 20);
        let node_id = arena.alloc(IrKind::Module, test_span);

        // Assert span_of returns the expected span
        let result = span_of(&arena, node_id);
        assert_eq!(result, Some(test_span));

        // Assert span_of returns None for a nonexistent node
        let nonexistent = IrNodeId::new(9999).expect("9999 is valid");
        let result = span_of(&arena, nonexistent);
        assert_eq!(result, None);
    }

    // Wave 29 (paideia-as#1553): removed `node_for_fixup_finds_referring_instruction`.
    // The `node_for_fixup` helper it exercised was retired along with the
    // `unresolved_label` (U1610) builder above — the fixup pass no longer
    // needs to correlate a stray fixup with an IR span because it now panics
    // with an ICE message on that branch (unreachable from user syntax).
}
