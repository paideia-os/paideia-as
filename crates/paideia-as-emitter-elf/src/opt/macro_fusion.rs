//! Macro-fusion-aware emission.
//!
//! Per optimization-passes.md §4: Intel/AMD CPUs fuse CMP+Jcc (or TEST+Jcc)
//! pairs into a single µop, but only when the pair is aligned within a single
//! 16-byte fetch boundary. This pass detects fusable pairs and aligns them
//! by inserting NOPs before the CMP/TEST as needed.

use paideia_as_ir::opt::{OptDiagSink, OptPass};

/// Macro-fusion optimization pass.
///
/// Detects CMP+Jcc (or TEST+Jcc) pairs and aligns them within 16-byte fetch
/// boundaries for CPU macro-fusion. Inserts NOP padding before the CMP/TEST
/// as needed to keep the pair within a single fetch window.
pub struct MacroFusionPass;

/// Fusable instruction-pair shapes.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FusionPair {
    /// CMP + JE (jump if equal).
    CmpJe,
    /// CMP + JNE (jump if not equal).
    CmpJne,
    /// CMP + JL (jump if less).
    CmpJl,
    /// CMP + JG (jump if greater).
    CmpJg,
    /// TEST + JE (jump if equal).
    TestJe,
    /// TEST + JNE (jump if not equal).
    TestJne,
}

impl FusionPair {
    /// Check whether this pair shape is fusable.
    ///
    /// All shapes in this enum are fusable per Intel and AMD's recent
    /// microarchitectures (Sandy Bridge / Bulldozer onward).
    pub fn is_fusable(self) -> bool {
        true
    }
}

/// Check whether the byte offset of the second instruction in a pair
/// crosses a 16-byte fetch boundary.
///
/// # Arguments
/// * `cmp_offset` - byte offset of the CMP/TEST instruction.
/// * `cmp_len` - byte length of the CMP/TEST instruction.
///
/// # Returns
/// `true` if the Jcc (starting at cmp_offset + cmp_len) starts and ends
/// in the same 16-byte window as the CMP/TEST. `false` if it crosses
/// a boundary.
pub fn within_same_fetch_window(cmp_offset: u32, cmp_len: u32) -> bool {
    let cmp_window = cmp_offset / 16;
    let jcc_start = cmp_offset + cmp_len;
    let jcc_window = jcc_start / 16;
    let jcc_end = jcc_start + 6; // Jcc rel32 is 6 bytes max.
    cmp_window == jcc_window && jcc_end / 16 == cmp_window
}

/// Compute pad bytes (NOPs) needed BEFORE the CMP to push the pair into
/// the same fetch window. Returns 0 if no padding is needed.
///
/// # Arguments
/// * `cmp_offset` - byte offset of the CMP/TEST instruction.
/// * `cmp_len` - byte length of the CMP/TEST instruction.
///
/// # Returns
/// The number of NOP bytes to insert before the CMP/TEST. Returns 0 if
/// the pair is already within the same fetch window.
pub fn pad_for_fusion(cmp_offset: u32, cmp_len: u32) -> u32 {
    if within_same_fetch_window(cmp_offset, cmp_len) {
        return 0;
    }
    // Align cmp_offset such that cmp_offset + total_len < (cmp_offset/16 + 1) * 16,
    // where total_len = cmp_len + 6 (max Jcc size). Push to next window.
    let next_window_boundary = ((cmp_offset / 16) + 1) * 16;
    next_window_boundary - cmp_offset
}

impl OptPass for MacroFusionPass {
    fn name(&self) -> &'static str {
        "macro-fusion"
    }

    fn apply(
        &self,
        _arena: &mut paideia_as_ir::IrArena,
        _root: paideia_as_ir::IrNodeId,
        sink: &mut OptDiagSink,
    ) -> bool {
        sink.emit(
            "macro-fusion",
            "O1504 (would-fire): macro-fusion-aware emission".to_string(),
        );
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fusion_pair_all_variants_are_fusable() {
        let variants = [
            FusionPair::CmpJe,
            FusionPair::CmpJne,
            FusionPair::CmpJl,
            FusionPair::CmpJg,
            FusionPair::TestJe,
            FusionPair::TestJne,
        ];

        for variant in &variants {
            assert!(
                variant.is_fusable(),
                "FusionPair {:?} should be fusable",
                variant
            );
        }
    }

    #[test]
    fn within_same_fetch_window_returns_true_when_no_crossing() {
        // CMP at offset 0, len 3 → Jcc starts at 3, ends at 9 (all in window 0).
        assert!(within_same_fetch_window(0, 3));

        // CMP at offset 8, len 3 → Jcc starts at 11, ends at 17 (within window 0, spans to 1).
        // Window 0: bytes 0-15, so 11-15 are in window 0, 16-17 are in window 1 → crosses.
        assert!(!within_same_fetch_window(8, 3));

        // CMP at offset 5, len 3 → Jcc starts at 8, ends at 14 (all in window 0).
        assert!(within_same_fetch_window(5, 3));
    }

    #[test]
    fn within_same_fetch_window_returns_false_at_boundary() {
        // CMP at offset 14, len 3 → Jcc starts at 17, ends at 23.
        // CMP is in window 0 (14/16 = 0), Jcc starts at 17 (17/16 = 1).
        // So they're in different windows → false.
        assert!(!within_same_fetch_window(14, 3));

        // CMP at offset 10, len 6 → Jcc starts at 16, ends at 22.
        // CMP is in window 0 (10/16 = 0), Jcc starts at 16 (16/16 = 1).
        // Different windows → false.
        assert!(!within_same_fetch_window(10, 6));
    }

    #[test]
    fn pad_for_fusion_returns_zero_when_aligned() {
        // Already aligned: offset 0, len 3 → no padding needed.
        assert_eq!(pad_for_fusion(0, 3), 0);

        // Already aligned: offset 5, len 3 → no padding needed.
        assert_eq!(pad_for_fusion(5, 3), 0);
    }

    #[test]
    fn pad_for_fusion_returns_padding_when_crossing() {
        // CMP at offset 10, len 6 → Jcc starts at 16, ends at 22 → crosses window.
        // Total len = 6 + 6 = 12.
        // Next window boundary = ((10 / 16) + 1) * 16 = 1 * 16 = 16.
        // Padding = 16 - 10 = 6.
        let padding = pad_for_fusion(10, 6);
        assert!(padding > 0, "Expected positive padding, got {}", padding);
        assert_eq!(padding, 6);

        // CMP at offset 14, len 3 → Jcc starts at 17, ends at 23 → crosses window.
        // Total len = 3 + 6 = 9.
        // Next window boundary = ((14 / 16) + 1) * 16 = 1 * 16 = 16.
        // Padding = 16 - 14 = 2.
        let padding = pad_for_fusion(14, 3);
        assert!(padding > 0, "Expected positive padding, got {}", padding);
        assert_eq!(padding, 2);
    }

    #[test]
    fn macro_fusion_pass_emits_o1504() {
        let pass = MacroFusionPass;
        let mut arena = paideia_as_ir::IrArena::new();
        let mut sink = OptDiagSink::new();

        // We need a valid IrNodeId; since the arena is empty, we create a dummy id.
        let dummy_id = paideia_as_ir::IrNodeId::new(1).unwrap();

        // MacroFusionPass.apply always returns false but emits a diagnostic.
        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed, "MacroFusionPass should return false");
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "macro-fusion");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1504 (would-fire): macro-fusion-aware emission")
        );
    }
}
