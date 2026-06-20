//! Function-entry alignment.

use paideia_as_ir::opt::{OptDiagSink, OptPass};

/// The alignment optimization pass.
pub struct AlignPass;

/// Compute the number of pad bytes (NOPs) needed to align the next
/// instruction to `alignment`. alignment must be a power of two.
pub fn pad_for_alignment(current_offset: u32, alignment: u32) -> u32 {
    debug_assert!(alignment.is_power_of_two());
    let mask = alignment - 1;
    (alignment - (current_offset & mask)) & mask
}

impl OptPass for AlignPass {
    fn name(&self) -> &'static str {
        "align"
    }

    fn apply(
        &self,
        _arena: &mut paideia_as_ir::IrArena,
        _root: paideia_as_ir::IrNodeId,
        sink: &mut OptDiagSink,
    ) -> bool {
        sink.emit(
            "align",
            "O1508 (would-fire): alignment padding dispatched".to_string(),
        );
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_for_alignment_returns_zero_when_aligned() {
        // Aligned to 64-byte boundary: offset 0 → 0 padding.
        assert_eq!(pad_for_alignment(0, 64), 0);

        // Aligned to 64-byte boundary: offset 64 → 0 padding.
        assert_eq!(pad_for_alignment(64, 64), 0);

        // Aligned to 16-byte boundary: offset 32 → 0 padding.
        assert_eq!(pad_for_alignment(32, 16), 0);
    }

    #[test]
    fn pad_for_alignment_pads_to_64() {
        // Current offset 16, alignment 64 → need 48 bytes to reach 64.
        assert_eq!(pad_for_alignment(16, 64), 48);

        // Current offset 8, alignment 64 → need 56 bytes to reach 64.
        assert_eq!(pad_for_alignment(8, 64), 56);

        // Current offset 1, alignment 64 → need 63 bytes to reach 64.
        assert_eq!(pad_for_alignment(1, 64), 63);
    }

    #[test]
    fn pad_for_alignment_pads_to_smaller_boundaries() {
        // Current offset 5, alignment 16 → need 11 bytes to reach 16.
        assert_eq!(pad_for_alignment(5, 16), 11);

        // Current offset 3, alignment 8 → need 5 bytes to reach 8.
        assert_eq!(pad_for_alignment(3, 8), 5);

        // Current offset 1, alignment 4 → need 3 bytes to reach 4.
        assert_eq!(pad_for_alignment(1, 4), 3);
    }

    #[test]
    fn align_pass_emits_o1508() {
        let pass = AlignPass;
        let mut arena = paideia_as_ir::IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = paideia_as_ir::IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed, "AlignPass should return false");
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "align");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1508 (would-fire): alignment padding dispatched")
        );
    }
}
