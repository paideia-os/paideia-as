//! Label-fixup patching for the encoded `.text` section.
//! Split out of `cmd_build.rs` (2026-07-08).
//!
//! Phase 8 m1-004: Routes unresolved-label errors through DiagnosticSink as U1610.

use paideia_as_diagnostics::DiagnosticSink;
use paideia_as_encoder::LabelFixup;
use paideia_as_ir::{IrArena, InstructionSideTable};

use super::BuildError;

/// Patch label fixups after .text encoding is complete.
///
/// Phase-6-m4-004: Called after all instructions have been encoded
/// and byte offsets are known. For each LabelFixup, computes the
/// displacement as: label_offset - (fixup_byte_offset + 4), then
/// writes the i32 LE value into the buffer at the fixup location.
///
/// Phase 8 m1-004: Routes unresolved-label errors through DiagnosticSink as U1610.
///
/// # Arguments
///
/// * `buffer` - Mutable reference to the .text section bytes
/// * `label_fixups` - List of fixup sites collected during encoding
/// * `labels` - Map of label names to their byte offsets in .text
/// * `strict_mode` - Whether to abort on unresolved labels
/// * `sink` - Diagnostic sink for emitting U1610 errors
/// * `arena` - IR arena for span lookups
/// * `instructions` - Instruction side-table for reverse lookup
/// * `file` - Source file ID for span generation
///
/// # Returns
///
/// `Ok(())` if all fixups applied successfully, or
/// `Err(BuildError::Failed)` if a label is unresolved in strict mode.
pub(super) fn patch_label_fixups(
    buffer: &mut [u8],
    label_fixups: &[LabelFixup],
    labels: &std::collections::HashMap<String, u32>,
    strict_mode: bool,
    sink: &mut dyn DiagnosticSink,
    arena: &IrArena,
    instructions: &InstructionSideTable,
    file: paideia_as_diagnostics::FileId,
) -> Result<(), BuildError> {
    use super::diagnostics::{unresolved_label, node_for_fixup, span_of};
    use paideia_as_diagnostics::Span;

    for fixup in label_fixups {
        match labels.get(&fixup.label_name) {
            Some(&label_offset) => {
                // Compute displacement: label_offset - (fixup_byte_offset + 4)
                // The "+4" accounts for the fact that relative offsets are computed
                // from the byte AFTER the displacement field (i.e., the next instruction).
                let disp = (label_offset as i64) - ((fixup.byte_offset as i64) + 4);
                let disp_i32 = disp as i32;

                // Write the displacement as i32 LE at the fixup offset
                let offset = fixup.byte_offset as usize;
                if offset + 4 <= buffer.len() {
                    let disp_bytes = disp_i32.to_le_bytes();
                    buffer[offset..offset + 4].copy_from_slice(&disp_bytes);
                }
            }
            None => {
                // Unresolved label: emit U1610 with real span if possible
                // Phase 8 m1-004: Attempt to resolve the instruction node via node_for_fixup,
                // then extract span from arena. Fall back to placeholder span if lookup fails.
                let span = node_for_fixup(instructions, fixup)
                    .and_then(|node_id| span_of(arena, node_id))
                    .or_else(|| Some(Span::new(file, 0, 1)));

                let diag = unresolved_label(&fixup.label_name, span);
                let _ = sink.emit(diag);
                if strict_mode {
                    return Err(BuildError::Failed);
                }
            }
        }
    }
    Ok(())
}
