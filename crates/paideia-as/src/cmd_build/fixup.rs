//! Label-fixup patching for the encoded `.text` section.
//! Split out of `cmd_build.rs` (2026-07-08).

use paideia_as_encoder::LabelFixup;
use paideia_as_ir::IrNodeId;

use super::BuildError;

/// Patch label fixups after .text encoding is complete.
///
/// Phase-6-m4-004: Called after all instructions have been encoded
/// and byte offsets are known. For each LabelFixup, computes the
/// displacement as: label_offset - (fixup_byte_offset + 4), then
/// writes the i32 LE value into the buffer at the fixup location.
///
/// # Arguments
///
/// * `buffer` - Mutable reference to the .text section bytes
/// * `label_fixups` - List of fixup sites collected during encoding
/// * `labels` - Map of label names to their byte offsets in .text
/// * `strict_mode` - Whether to abort on unresolved labels
///
/// # Returns
///
/// `Ok(())` if all fixups applied successfully, or
/// `Err(BuildError::Encoder)` if a label is unresolved in strict mode.
pub(super) fn patch_label_fixups(
    buffer: &mut [u8],
    label_fixups: &[LabelFixup],
    labels: &std::collections::HashMap<String, u32>,
    strict_mode: bool,
    file: paideia_as_diagnostics::FileId,
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
                // Unresolved label: emit U1610
                eprintln!("error: unresolved label '{}' (U1610)", fixup.label_name);
                if strict_mode {
                    let span = paideia_as_diagnostics::Span::new(file, 0, 1);
                    return Err(BuildError::Encoder {
                        node: IrNodeId::new(1).unwrap(),
                        source_span: span,
                        encoder_message: format!("unresolved label '{}'", fixup.label_name),
                    });
                }
            }
        }
    }
    Ok(())
}
