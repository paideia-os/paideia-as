//! Block-body emit paths (multi-statement function bodies + match arm bodies).
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Hosts the
//! twin lowerings that walk the statement children of an `Action` block:
//!
//! - `emit_block_body`     — Lambda → Action shape at function level
//! - `emit_block_body_arm` — same shape inside a match arm body
//!
//! Both walk `Let` / `StmtExpr` / `RawInstruction` children, allocating
//! scratch registers via `state.local_bindings` and emitting the tail
//! expression to RAX.
//!
//! Split into cohesive submodules during the god-file refactor
//! (issue #1410, umbrella #1405):
//!
//! - [`diagnostics`]     — typed `DiagnosticCode` factories shared across
//!   the sibling modules (T0527, U1621, U1642, U1659, T0559).
//! - [`store_dispatch`]  — three-way Store dispatch helper
//!   (`dispatch_store`) that chooses field-assign / var-assign / general
//!   store on a `Store` node's first child.
//! - [`block_body`]      — `emit_block_body`: statement-walk driver for
//!   the enclosing Lambda's Action body (function-level).
//! - [`block_body_arm`]  — `emit_block_body_arm`: mirror walker for the
//!   Action body of a match arm (no trailing ret; owns scope push/pop).
//! - [`tail_expr`]       — `emit_tail_expr`: tail-position placement of
//!   Literal / Var / EnumCons / Match / Branch per `TailContext`.
//! - [`action_stmt`]     — `emit_action_stmt`: statement-position action
//!   dispatch (App / FieldAccess / Var / Literal / Store).
//!
//! All extracted methods remain on `impl EmitWalker` and preserve their
//! previous `pub(crate)` / private visibility. No behavior change; no
//! public path change. `TailContext` continues to be reachable at
//! `crate::emit_block_body::TailContext` (used by sibling modules
//! `emit_enum_match`, `emit_int_match`, `emit_visit_lambda`, and
//! `emit_walker`).

mod diagnostics;
mod store_dispatch;
mod block_body;
mod block_body_arm;
mod tail_expr;
mod action_stmt;

/// PA-r17-013 (#991): Tracks the tail-expression context for proper return-value placement.
/// When an expression appears in trailing position, its result must land in the correct
/// location per the function's return convention, not RAX (which is for discarded values).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TailContext {
    /// Not the trailing expression — value is discarded.
    Discard,
    /// Trailing expression, result must land in RAX only.
    ReturnRax,
    /// Trailing expression, result must land in RAX (discriminant) + RDX (payload).
    ReturnRaxRdx,
    /// Trailing expression, result must be written to [RDI + disp] (indirect return).
    ReturnIndirect {
        /// Discriminant size in bytes for discriminant-only enums.
        disc_size: i32,
    },
}
