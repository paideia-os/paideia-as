//! EmitWalker — Phase 5 m1-001 entry to the build-emit pipeline.
//!
//! Walks the IR; per-construct lowering (m1-002 Let-literal, m1-003 Lambda,
//! m1-004 Unsafe) lands as siblings inside this module. The walker
//! populates an InstructionSideTable + tracks per-function offsets.
//!
//! Refactored (paideia-as#1411) from a 1697-line monolith into topic-focused
//! submodules. The `pub struct EmitWalker` definition and all its `impl`
//! blocks (split across the submodules below) remain reachable at
//! `crate::emit_walker::EmitWalker`; downstream sibling files that
//! `use crate::emit_walker::EmitWalker` continue to resolve unchanged.

use paideia_as_diagnostics::Diagnostic;
// The re-imports below are load-bearing for the test files under
// `src/emit_walker_tests/`, which pull them in via `use super::super::*;`.
// Before the paideia-as#1411 split every use lived at the top of the
// monolithic `emit_walker.rs`, so the test glob-import saw them all;
// now each sub-file carries its own uses privately and mod.rs has to
// re-stage the set the tests still expect. Everything below is
// `#[cfg(test)]`+`#[allow(unused_imports)]` — the split submodules import
// what they actually use, so at the module root these are consumed only
// by the test glob.
#[cfg(test)]
#[allow(unused_imports)]
use paideia_as_ir::{DataSideTable, IrArena, IrKind, IrNodeId, SmallVec, Symbol, SymbolKind, abi};
#[cfg(test)]
#[allow(unused_imports)]
use paideia_as_ir::instruction::{InstrMode, Instruction, Mnemonic, Operand};
#[cfg(test)]
#[allow(unused_imports)]
use paideia_as_ir::instruction::{Cond, IntWidth};
#[cfg(test)]
#[allow(unused_imports)]
use paideia_as_ir::record_layout::{FieldLayout, RecordLayout, RecordTypeId};
#[cfg(test)]
#[allow(unused_imports)]
use paideia_as_ir::{EnumLayout, EnumTypeId};
#[cfg(test)]
#[allow(unused_imports)]
use paideia_as_diagnostics::DiagnosticCode;

pub use crate::cast_shape::{CastPlan, CastShape, cast_plan};
pub use crate::emit_pass_state::{EmitPassState, LoopContext};

mod closure_prepass;
mod emit_core;
mod emit_interrupt;
mod pending_unsafe;
mod state;
mod walk;

/// EmitWalker — drives IR traversal and instruction emission.
///
/// Skeleton implementation for Phase 5 m1-001. Per-construct lowering
/// hooks (visit_let, visit_lambda, visit_unsafe) land in m1-002..004
/// as siblings of this walker.
///
/// Phase 7 m1-008 (PA7-008): Tracks loop context stack for break validation.
pub struct EmitWalker {
    pub(crate) state: EmitPassState,
    /// Legacy free-form diagnostic buffer. Each entry is a `format!` string
    /// with a `T####:` prefix. Retirement into `structured_diagnostics` is
    /// tracked as a follow-up in `.plans/refactor-2026-07-07.md`.
    pub(crate) diagnostics: Vec<String>,
    /// Canonical typed diagnostic buffer introduced in the v0.17 refactor
    /// (Step 3, 2026-07-07). All NEW EmitWalker diagnostics must be pushed
    /// via `push_typed_diag`, which routes here. Drained by cmd_build.rs
    /// via `take_typed_diagnostics()` into the shared `DiagnosticSink`,
    /// making silent-fire-then-discard impossible for new push sites.
    pub(crate) structured_diagnostics: Vec<Diagnostic>,
    /// Stack of (loop_kind, exit_label) for nested loops/while.
    /// Push on loop/while entry, pop on exit. Used to validate break statements.
    pub(crate) loop_contexts: Vec<(LoopContext, String)>,
}

impl Default for EmitWalker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../emit_walker_tests.rs"]
mod tests;
