//! Optimization pass infrastructure for paideia-as IR.
//!
//! Per OS-requirements §6 / optimization-passes.md (m9 milestone):
//! optimization is opt-in. The user annotates functions with
//! #[peephole], #[unroll(n)], #[dse], etc. The dispatcher walks the
//! pass catalog in canonical order and invokes only the annotated
//! passes.

pub mod branch_hint;
pub mod composition;
pub mod dispatch;
pub mod dse;
pub mod peephole;
pub mod pool_constants;
pub mod schedule;
pub mod tailcall;
pub mod unroll;

// Re-export canonical pass types.
pub use branch_hint::BranchHintPass;
pub use composition::{canonical_pass_order, dispatch_collecting_order};
pub use dse::DsePass;
pub use peephole::PeepholePass;
pub use pool_constants::PoolConstantsPass;
pub use schedule::{
    InstructionClass, InstructionSchedulingPass, schedule_block, schedule_block_impl,
};
pub use tailcall::TailCallPass;
pub use unroll::UnrollPass;

use crate::IrArena;
use crate::node::IrNodeId;

/// A single optimization pass.
pub trait OptPass {
    /// The canonical pass name (matches the annotation token).
    fn name(&self) -> &'static str;

    /// Apply the pass to the given function. Returns true if anything
    /// changed (useful for fixed-point iteration in dispatch).
    fn apply(&self, arena: &mut IrArena, function_root: IrNodeId, sink: &mut OptDiagSink) -> bool;
}

/// Diagnostic sink for opt passes.
///
/// Phase-2-m9-001 minimum: a Vec<OptDiagnostic> collected per
/// dispatch run. m9-011 wires it to paideia-as-diagnostics' real sink.
#[derive(Default, Debug)]
pub struct OptDiagSink {
    /// Collected diagnostics from optimization passes.
    pub diagnostics: Vec<OptDiagnostic>,
}

/// A single diagnostic emitted by an optimization pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptDiagnostic {
    /// The pass that emitted this diagnostic.
    pub pass: String,
    /// The diagnostic message.
    pub message: String,
}

impl OptDiagSink {
    /// Construct an empty diagnostic sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Emit a diagnostic from a pass.
    pub fn emit(&mut self, pass: &str, message: String) {
        self.diagnostics.push(OptDiagnostic {
            pass: pass.to_string(),
            message,
        });
    }
}

/// The no-op pass: smoke-test infrastructure (AC bullet 3).
pub struct NoOpPass;

impl OptPass for NoOpPass {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn apply(
        &self,
        _arena: &mut IrArena,
        _function_root: IrNodeId,
        _sink: &mut OptDiagSink,
    ) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_pass_returns_false_no_changes() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let pass = NoOpPass;

        // We need a valid IrNodeId; since the arena is empty, we can't use it directly.
        // For this smoke test, we use a hypothetical id and rely on the pass not accessing it.
        let dummy_id = IrNodeId::new(1).unwrap();

        let changed = pass.apply(&mut arena, dummy_id, &mut sink);

        assert!(!changed);
    }

    #[test]
    fn opt_diag_sink_collects_emits() {
        let mut sink = OptDiagSink::new();

        assert_eq!(sink.diagnostics.len(), 0);

        sink.emit("pass1", "warning 1".to_string());
        sink.emit("pass2", "error 1".to_string());

        assert_eq!(sink.diagnostics.len(), 2);
        assert_eq!(sink.diagnostics[0].pass, "pass1");
        assert_eq!(sink.diagnostics[0].message, "warning 1");
        assert_eq!(sink.diagnostics[1].pass, "pass2");
        assert_eq!(sink.diagnostics[1].message, "error 1");
    }
}
