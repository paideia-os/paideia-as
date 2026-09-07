//! EmitWalker — constructor, state accessors, diagnostic sinks, mode/loop
//! stacks, and the `emitted_lambdas` / `record_lambda_entry` bookkeeping.
//!
//! Split from `emit_walker.rs` (paideia-as#1411). Pure accessor / small-op
//! surface; the heavy per-construct lowering lives in sibling submodules.

use paideia_as_diagnostics::{Diagnostic, DiagnosticCode};
use paideia_as_ir::instruction::InstrMode;
use paideia_as_ir::IrNodeId;

use super::{EmitPassState, EmitWalker, LoopContext};

impl EmitWalker {
    /// Create a new, empty EmitWalker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: EmitPassState::default(),
            diagnostics: Vec::new(),
            structured_diagnostics: Vec::new(),
            loop_contexts: Vec::new(),
        }
    }

    /// Access the emission state (read-only).
    #[must_use]
    pub fn state(&self) -> &EmitPassState {
        &self.state
    }

    /// Access the emission state (mutable).
    #[must_use]
    pub fn state_mut(&mut self) -> &mut EmitPassState {
        &mut self.state
    }

    /// Access the accumulated legacy free-form diagnostics.
    ///
    /// New code should use `push_typed_diag` / `take_typed_diagnostics`
    /// instead — see the v0.17 refactor plan (2026-07-07) for the
    /// migration path.
    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Push a canonical typed diagnostic (Step 3, v0.17 refactor).
    ///
    /// The `code` names a T####/S####/etc. entry in the diagnostic
    /// catalog. `message` is the human-readable body; no `T####:` prefix
    /// is required because the code already carries the identity.
    ///
    /// The diagnostic accumulates in `structured_diagnostics` and is
    /// drained by `cmd_build::run` into the shared `DiagnosticSink` right
    /// after `emit_walker.walk(...)`. Silent-fire-then-discard cannot
    /// happen here — the drain wiring is a static compile-time contract.
    pub fn push_typed_diag(&mut self, code: DiagnosticCode, message: impl Into<String>) {
        let diag = Diagnostic::error(code).message(message).finish();
        self.structured_diagnostics.push(diag);
    }

    /// Drain and return the typed diagnostics accumulated during the walk.
    ///
    /// Called once by `cmd_build::run` after the emit walk completes.
    #[must_use]
    pub fn take_typed_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.structured_diagnostics)
    }

    /// Drain and return the legacy free-form diagnostics accumulated during the walk.
    ///
    /// Called once by `cmd_build::run` after typed diagnostics are drained
    /// (issue #1082). Each message is wrapped in a U1616 Diagnostic before emission.
    /// Post-#1086 migration, this channel holds only non-T-coded internal errors
    /// (invariant violations, missing side-tables, unpopulated layouts).
    /// Any fire indicates a silent-broken-.o class bug.
    #[must_use]
    pub fn take_legacy_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Phase 15 m2-002a: Set the root module's instruction mode.
    /// This initializes the mode_stack for instruction emission.
    /// Must be called before walk() or walk_with_typer().
    pub fn set_root_mode(&mut self, mode: InstrMode) {
        self.state.mode_stack.clear();
        self.state.mode_stack.push(mode);
    }

    /// Phase 7 m1-008: Check if we are currently in a loop body.
    /// Returns Some((loop_kind, exit_label)) if in loop, None if outside.
    #[must_use]
    pub fn current_loop_context(&self) -> Option<(LoopContext, &str)> {
        self.loop_contexts
            .last()
            .map(|(ctx, label)| (*ctx, label.as_str()))
    }

    /// Phase 7 m1-008: Pop loop context on loop/while exit.
    pub fn pop_loop_context(&mut self) {
        let _ = self.loop_contexts.pop();
    }

    /// Phase 15 m2-002: Enter a new instruction mode scope.
    /// Will be used in m2-002b for scope-aware mode propagation.
    #[allow(dead_code)]
    pub(super) fn enter_mode_scope(&mut self, mode: InstrMode) {
        self.state.mode_stack.push(mode);
    }

    /// Phase 15 m2-002: Exit the current instruction mode scope.
    /// Will be used in m2-002b for scope-aware mode propagation.
    #[allow(dead_code)]
    pub(super) fn exit_mode_scope(&mut self) {
        self.state.mode_stack.pop();
    }

    /// Get the set of Lambda IR node IDs that emitted bytecode.
    #[must_use]
    pub fn emitted_lambdas(&self) -> &std::collections::HashSet<u32> {
        &self.state.emitted_lambdas
    }

    /// Record a lambda's entry point instruction and mark it as emitted.
    ///
    /// Called at the START of each emit_*_lambda function to record:
    /// 1. The first instruction's IrNodeId (for post-encoding offset projection via offset_map)
    /// 2. Marks the lambda as emitted for symbol filtering
    pub fn record_lambda_entry(&mut self, lambda_id: IrNodeId, first_instr_id: IrNodeId) {
        // Record the first instruction's IR node ID for offset_map projection
        self.state
            .lambda_first_instr
            .entry(lambda_id.get())
            .or_insert(first_instr_id);

        self.state.mark_lambda_emitted(lambda_id.get());
    }
}
