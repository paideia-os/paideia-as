//! Label-fixup patching for the encoded `.text` section.
//! Split out of `cmd_build.rs` (2026-07-08).
//!
//! Wave 29 (paideia-as#1553): Retired the user-facing U1610 emission on
//! the unresolved-label branch. The elaborator's own U1610 guard at
//! `paideia_as_elaborator::unsafe_walker::process_stmt` catches every
//! user-authored unresolved label BEFORE encoding: `parse_operand_from_ast`
//! only emits `Operand::LabelRef` when the identifier is present in the
//! per-block labels map, and otherwise falls through to `Operand::SymbolRef`
//! (link-time resolution). No `.pdx` source can therefore reach this pass
//! with an unresolved `LabelRef`.
//!
//! A LabelFixup arriving here without a matching entry is exclusively a
//! compiler invariant violation — an `emit_*` site synthesised a
//! `LabelRef { name }` without a paired `insert_label(name, ...)`. That is
//! an ICE, not a user diagnostic, so this pass now panics with a bug-report
//! message instead of emitting U1610 through the diagnostic sink.

use paideia_as_encoder::LabelFixup;

use super::BuildError;

/// Patch label fixups after .text encoding is complete.
///
/// Phase-6-m4-004: Called after all instructions have been encoded
/// and byte offsets are known. For each LabelFixup, computes the
/// displacement as: label_offset - (fixup_byte_offset + 4), then
/// writes the i32 LE value into the buffer at the fixup location.
///
/// Wave 29 (paideia-as#1553): An unresolved label at this stage is a
/// compiler invariant violation (see module doc); the previous
/// user-diagnostic U1610 path has been retired.
///
/// # Arguments
///
/// * `buffer` - Mutable reference to the .text section bytes
/// * `label_fixups` - List of fixup sites collected during encoding
/// * `labels` - Map of label names to their byte offsets in .text
///
/// # Returns
///
/// `Ok(())` if all fixups applied successfully.
///
/// # Panics
///
/// Panics if a fixup references a label that is not present in `labels`.
/// This indicates an elaborator bug — a compiler-synthesised `LabelRef`
/// without a matching `insert_label`. Users cannot trigger this branch;
/// see the module documentation.
pub(super) fn patch_label_fixups(
    buffer: &mut [u8],
    label_fixups: &[LabelFixup],
    labels: &std::collections::HashMap<String, u32>,
) -> Result<(), BuildError> {
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
                // Wave 29 (paideia-as#1553): user-side unresolved-label handling
                // now lives entirely in the elaborator (U1610 in
                // `unsafe_walker::process_stmt`). Reaching this branch means
                // an elaborator emit site produced `Operand::LabelRef { name }`
                // without a paired `insert_label(name, ...)` — a compiler bug.
                panic!(
                    "internal compiler error: label fixup references unresolved label \
                     `{}` at .text offset 0x{:x} (instruction_size={}). This indicates \
                     an elaborator emit site synthesised a LabelRef without registering \
                     the matching label. Please file a bug at \
                     https://github.com/paideia-os/paideia-as/issues with the offending \
                     .pdx source.",
                    fixup.label_name, fixup.byte_offset, fixup.instruction_size,
                );
            }
        }
    }
    Ok(())
}
