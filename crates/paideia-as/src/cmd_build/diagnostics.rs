//! Common diagnostic finisher for encoder + emitter build errors.
//! Split out of `cmd_build.rs` (2026-07-08).
//!
//! Phase 8 m1-004: Helper builders for typed diagnostics routed through DiagnosticSink.
//! - encoder_error / encoder_warn: B1705/B1706 for encoding failures
//! - unresolved_label: U1610 for label fixups
//! - symbol_layout_invalid: B1703 for emitter validation
//! - function_symbol_no_offset: B1704 for missing function offsets
//! - span_of: Lookup IR node span from arena
//! - node_for_fixup: Reverse lookup instruction node ID from label fixup

use std::path::Path;
use std::process::ExitCode;

use std::str::FromStr;

use paideia_as_diagnostics::{Catalog, Diagnostic, DiagnosticCode, DiagnosticSink, HumanRenderer, HumanSink, SourceMap, Span, VecSink};
use paideia_as_ir::{IrArena, IrNodeId, InstructionSideTable};
use paideia_as_encoder::LabelFixup;

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
    let renderer = HumanRenderer::with_catalog(source_map, true, catalog);
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

/// Reverse lookup: find the IR node ID of the instruction that failed to encode.
/// Used to correlate label fixups with IR source locations.
/// Phase 8 m1-004: Implementation via byte_offset reverse lookup.
pub(super) fn node_for_fixup(instructions: &InstructionSideTable, fixup: &LabelFixup) -> Option<IrNodeId> {
    // Phase 8 m1-004: For label fixups, reverse-lookup the instruction node via byte_offset.
    // The fixup's byte_offset is where the 4-byte rel32 sits. The instruction starts
    // at byte_offset - (instruction_size - 4). We iterate through the side-table
    // and match on the calculated start offset.
    let expected_instr_start = fixup.byte_offset.saturating_sub(fixup.instruction_size - 4);

    for (&node_id, instr) in instructions.entries().iter() {
        if let Some(byte_offset) = instr.byte_offset_in_text {
            if byte_offset == expected_instr_start {
                return Some(node_id);
            }
        }
    }

    None
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

/// Build a typed U1610 unresolved-label diagnostic.
pub(super) fn unresolved_label(
    label: &str,
    span: Option<Span>,
) -> Diagnostic {
    // Phase 6 m4-002: U1610 fires when a label reference is unresolved.
    let code = DiagnosticCode::from_str("U1610").expect("U1610 is a valid code");
    let msg = format!("unresolved label '{}'", label);
    let mut diag = Diagnostic::error(code).message(&msg);
    if let Some(s) = span {
        diag = diag.with_span(s);
    }
    diag.finish()
}

/// Build a typed B1703 symbol-layout-invalid diagnostic.
pub(super) fn symbol_layout_invalid(message: &str) -> Diagnostic {
    // Phase 7 m1-002: B1703 fires when symbol layout validation fails.
    let code = DiagnosticCode::from_str("B1703").expect("B1703 is a valid code");
    Diagnostic::error(code).message(message).finish()
}

/// Build a typed B1704 function-symbol-no-offset diagnostic.
pub(super) fn function_symbol_no_offset(name: &str, ir_node: u32) -> Diagnostic {
    // Phase 8 m1-003: B1704 fires when a function symbol has no recorded offset.
    let code = DiagnosticCode::from_str("B1704").expect("B1704 is a valid code");
    let msg = format!("function symbol '{}' (ir_node {}) has no recorded offset", name, ir_node);
    Diagnostic::warning(code).message(&msg).finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{IrKind, IrArena};
    use paideia_as_encoder::LabelFixup;

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

    #[test]
    fn node_for_fixup_finds_referring_instruction() {
        // Phase 8 m1-004: Verify node_for_fixup reverse-lookups the instruction by byte_offset.
        use paideia_as_ir::{Instruction, Mnemonic, InstrMode};

        let mut instructions = InstructionSideTable::new();

        // Construct a known instruction at a specific byte offset
        let node_id = IrNodeId::new(42).expect("42 is valid");
        let instr = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: Default::default(),
            encoding_hint: None,
            byte_offset_in_text: Some(100),
            mode: InstrMode::Mode64,
            emission_order: 0,
};
        instructions.insert(node_id, instr);

        // Construct a LabelFixup pointing to offset 101 (start at 100, instruction_size=5 for jmp)
        // Reverse calculation: fixup.byte_offset - (fixup.instruction_size - 4) = 101 - 1 = 100 ✓
        let fixup = LabelFixup {
            byte_offset: 101,  // Where the 4-byte rel32 sits (1 byte into the instruction)
            label_name: "test_label".to_string(),
            addend: 0,
            instruction_size: 5,  // jmp is 5 bytes
        };

        let result = node_for_fixup(&instructions, &fixup);
        assert_eq!(result, Some(node_id));

        // Test with a nonexistent fixup (different byte offset)
        let nonexistent_fixup = LabelFixup {
            byte_offset: 999,
            label_name: "nonexistent".to_string(),
            addend: 0,
            instruction_size: 5,
        };
        let result = node_for_fixup(&instructions, &nonexistent_fixup);
        assert_eq!(result, None);
    }
}
