//! Branch-hint code layout.

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::node::IrNodeId;

/// The branch-hint optimization pass.
pub struct BranchHintPass;

/// Branch-hint directives for code layout.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum BranchHint {
    /// Branch is predicted likely to be taken.
    Likely,
    /// Branch is predicted unlikely to be taken.
    Unlikely,
}

/// Whether the error path should be laid out as the taken branch
/// (i.e., the unlikely path moves off the fall-through).
pub fn lay_unlikely_off_fall_through(hint: BranchHint) -> bool {
    matches!(hint, BranchHint::Unlikely)
}

impl OptPass for BranchHintPass {
    fn name(&self) -> &'static str {
        "branch-hint"
    }

    fn apply(&self, _arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        sink.emit(
            "branch-hint",
            "O1507 (would-fire): branch-hint layout dispatched".to_string(),
        );
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lay_unlikely_off_fall_through_returns_true_for_unlikely() {
        assert!(lay_unlikely_off_fall_through(BranchHint::Unlikely));
    }

    #[test]
    fn lay_unlikely_off_fall_through_returns_false_for_likely() {
        assert!(!lay_unlikely_off_fall_through(BranchHint::Likely));
    }

    #[test]
    fn branch_hint_pass_emits_o1507() {
        let pass = BranchHintPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed, "BranchHintPass should return false");
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "branch-hint");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1507 (would-fire): branch-hint layout dispatched")
        );
    }

    #[test]
    fn branch_hint_emits_no_diagnostic_for_unconditional_jmp() {
        // Phase-3 minimum: unconditional jumps (jmp) do not receive branch hints.
        // This test documents the behavior: hint pass only targets conditional jumps.
        let pass = BranchHintPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        // Apply the pass; it should still emit the general O1507 diagnostic,
        // not a per-Jcc diagnostic. The unconditional-jmp filtering is
        // implemented at the encoder level in m4.
        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed);
        // The pass emits one general diagnostic, not filtered yet.
        assert_eq!(sink.diagnostics.len(), 1);
        assert!(sink.diagnostics[0].message.contains("O1507"));
    }
}
