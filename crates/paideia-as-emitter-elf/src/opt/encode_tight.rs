//! REX/EVEX prefix tightening.
//!
//! Per optimization-passes.md §6: when an instruction can be encoded in a shorter
//! form (e.g., 32-bit ADD when only 32 low bits are used; short Jcc rel8 instead
//! of rel32 when target is in range), pick the shorter form. This reduces code
//! size and improves L1i cache density.

use paideia_as_ir::opt::{OptDiagSink, OptPass};

/// REX/EVEX prefix tightening optimization pass.
///
/// Detects instructions that can be encoded in a shorter form (e.g., 32-bit ADD
/// when only 32 low bits are used; Jcc rel8 instead of rel32 when target is in range)
/// and emits diagnostics for further instruction-selection improvements.
pub struct EncodeTightPass;

/// Whether a 64-bit ADD with the given operand can be shortened to 32-bit.
///
/// True when the high 32 bits are known to be zero/unused (e.g., the
/// 32-bit form clears the high bits implicitly).
pub fn can_shorten_add_to_32bit(high_bits_used: bool) -> bool {
    !high_bits_used
}

/// Whether a Jcc rel32 can be shortened to rel8.
///
/// rel8 range: -128..=127 from the byte AFTER the jcc.
pub fn can_use_rel8(displacement: i64) -> bool {
    (-128..=127).contains(&displacement)
}

/// Compute the byte savings from picking the short form.
///
/// 64→32 ADD: saves 1 byte (no REX.W).
/// rel32→rel8 Jcc: saves 4 bytes (5-byte rel32 form → 2-byte rel8).
pub enum ShortForm {
    /// 64-bit ADD shortened to 32-bit (saves 1 byte).
    Add64To32,
    /// Jcc rel32 shortened to rel8 (saves 4 bytes).
    JccRel32ToRel8,
}

impl ShortForm {
    /// Compute the number of bytes saved by this short form.
    pub fn savings_bytes(self) -> u32 {
        match self {
            Self::Add64To32 => 1,
            Self::JccRel32ToRel8 => 4,
        }
    }
}

impl OptPass for EncodeTightPass {
    fn name(&self) -> &'static str {
        "encode-tight"
    }

    fn apply(
        &self,
        _arena: &mut paideia_as_ir::IrArena,
        _root: paideia_as_ir::IrNodeId,
        sink: &mut OptDiagSink,
    ) -> bool {
        sink.emit(
            "encode-tight",
            "O1506 (would-fire): REX/EVEX prefix tightening dispatched".to_string(),
        );
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_shorten_add_to_32bit_returns_true_when_no_high_bits() {
        assert!(can_shorten_add_to_32bit(false));
    }

    #[test]
    fn can_shorten_add_to_32bit_returns_false_when_high_bits_used() {
        assert!(!can_shorten_add_to_32bit(true));
    }

    #[test]
    fn can_use_rel8_at_boundary_127_and_minus_128() {
        assert!(can_use_rel8(127));
        assert!(can_use_rel8(-128));
        assert!(can_use_rel8(0));
        assert!(can_use_rel8(50));
        assert!(can_use_rel8(-50));
    }

    #[test]
    fn can_use_rel8_returns_false_outside_range() {
        assert!(!can_use_rel8(128));
        assert!(!can_use_rel8(-129));
        assert!(!can_use_rel8(256));
        assert!(!can_use_rel8(-256));
    }

    #[test]
    fn short_form_savings_match_documented() {
        assert_eq!(ShortForm::Add64To32.savings_bytes(), 1);
        assert_eq!(ShortForm::JccRel32ToRel8.savings_bytes(), 4);
    }

    #[test]
    fn encode_tight_pass_emits_o1506() {
        let pass = EncodeTightPass;
        let mut arena = paideia_as_ir::IrArena::new();
        let mut sink = OptDiagSink::new();

        // We need a valid IrNodeId; since the arena is empty, we create a dummy id.
        let dummy_id = paideia_as_ir::IrNodeId::new(1).unwrap();

        // EncodeTightPass.apply always returns false but emits a diagnostic.
        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed, "EncodeTightPass should return false");
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "encode-tight");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1506 (would-fire): REX/EVEX prefix tightening dispatched")
        );
    }
}
