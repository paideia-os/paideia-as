//! Tail-call elimination.

use super::{OptPass, OptDiagSink};
use crate::IrArena;
use crate::node::IrNodeId;

/// The tail-call elimination optimization pass.
pub struct TailCallPass;

/// Conditions that suppress TCO.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TcoBlocker {
    /// Calling across a capability frontier.
    CapabilityBoundary,
    /// Call installs a handler the caller would lose track of.
    EffectHandlerInstalling,
    /// ABI mismatch (e.g., SysV → MS-x64).
    DifferentCallConvention,
    /// Caller still needs to run epilogue (saved regs).
    FrameRequiresEpilogue,
}

/// Whether the call site is eligible for TCO. Returns the blocker
/// (Some) or None (eligible).
pub fn tco_blocker(
    crosses_cap_boundary: bool,
    installs_handler: bool,
    abi_mismatch: bool,
    frame_has_callee_saves: bool,
) -> Option<TcoBlocker> {
    if crosses_cap_boundary { return Some(TcoBlocker::CapabilityBoundary); }
    if installs_handler { return Some(TcoBlocker::EffectHandlerInstalling); }
    if abi_mismatch { return Some(TcoBlocker::DifferentCallConvention); }
    if frame_has_callee_saves { return Some(TcoBlocker::FrameRequiresEpilogue); }
    None
}

impl OptPass for TailCallPass {
    fn name(&self) -> &'static str { "tailcall" }

    fn apply(&self, _arena: &mut IrArena, _root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        sink.emit("tailcall", "O1510 (would-fire): tail-call elimination dispatched".to_string());
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tco_blocker_returns_none_when_eligible() {
        let result = tco_blocker(false, false, false, false);
        assert_eq!(result, None);
    }

    #[test]
    fn tco_blocker_returns_capability_boundary() {
        let result = tco_blocker(true, false, false, false);
        assert_eq!(result, Some(TcoBlocker::CapabilityBoundary));
    }

    #[test]
    fn tco_blocker_returns_effect_handler_installing() {
        let result = tco_blocker(false, true, false, false);
        assert_eq!(result, Some(TcoBlocker::EffectHandlerInstalling));
    }

    #[test]
    fn tco_blocker_returns_different_call_convention() {
        let result = tco_blocker(false, false, true, false);
        assert_eq!(result, Some(TcoBlocker::DifferentCallConvention));
    }

    #[test]
    fn tco_blocker_returns_frame_requires_epilogue() {
        let result = tco_blocker(false, false, false, true);
        assert_eq!(result, Some(TcoBlocker::FrameRequiresEpilogue));
    }

    #[test]
    fn tailcall_pass_emits_o1510() {
        let pass = TailCallPass;
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();

        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed, "TailCallPass should return false");
        assert_eq!(sink.diagnostics.len(), 1, "Expected one diagnostic emitted");
        assert_eq!(sink.diagnostics[0].pass, "tailcall");
        assert!(
            sink.diagnostics[0]
                .message
                .contains("O1510 (would-fire): tail-call elimination dispatched")
        );
    }
}
