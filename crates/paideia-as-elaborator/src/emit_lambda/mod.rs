//! Special-shape lambda emitters and the indirect-call sequence.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Hosts the
//! per-shape lambda lowerings (`identity`, `bitnot`, `cast`, `double`) plus
//! the indirect-call marshalling used by `PA-r17-004`.
//!
//! All functions are `impl EmitWalker` methods and share the walker's
//! internal state (`emit_inst`, `record_lambda_entry`, `current_mode`,
//! `state.local_bindings`, `emit_mov_literal_to_reg`) via `pub(crate)`
//! visibility set on `EmitWalker`.
//!
//! # File layout
//!
//! Split out of `emit_lambda.rs` (paideia-as #1404). The sub-files:
//! - `special_shape` — one-shot special-shape lambdas (`identity`, `bitnot`,
//!   `cast`, `cast_with_shape`, `double`).
//! - `indirect_call` — the three indirect-call sequences: via a register,
//!   via a RIP-relative symbol, and via `[base + disp]`.
//! - `closure_call` — closure invocation (`emit_closure_call`), the fat-pair
//!   `mov r14, [r11+0]; call [r11+8]` sequence with scratch save/restore.
//! - `closure_cons` — `emit_closure_cons`, the fat-pointer materializer
//!   for `ClosureCons` IR nodes (env writes + code_ptr LEA).

// --- Internal submodules ---
mod closure_call;
mod closure_cons;
mod indirect_call;
mod special_shape;

// No `pub use` re-exports needed: every function in this module is an
// `impl EmitWalker` method with `pub(crate)` visibility, so the methods
// remain callable as `walker.emit_identity_lambda(...)` etc. regardless
// of which sub-file houses the `impl` block. The module path
// `crate::emit_lambda` itself is preserved by the `pub mod emit_lambda;`
// declaration in `lib.rs`.
