//! Enum + match lowering — variant constructors, discriminant extraction,
//! pattern binding decomposition, and match dispatch.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. The four
//! methods hosted here are cohesive: `visit_enum_cons` produces a value
//! that `visit_enum_discriminant` and `visit_match` consume, and
//! `lower_pattern` is the recursive helper that both stack-form enum
//! bindings and match arms delegate to.
//!
//! Split into cohesive submodules during the god-file refactor
//! (issue #1409, umbrella #1405):
//!
//! - [`diagnostics`] — typed `DiagnosticCode` factories shared across the
//!   sibling modules.
//! - [`scrutinee`] — scrutinee load + stack-form discriminant extraction
//!   (`emit_scrutinee_load`, `visit_enum_discriminant`).
//! - [`enum_cons`] — variant constructor lowering (`visit_enum_cons`,
//!   `emit_enum_cons_inner`).
//! - [`pattern_lower`] — nested pattern binding decomposition
//!   (`lower_pattern`, `lower_pattern_from_reg`, and their inner recursion
//!   helpers).
//! - [`match_jump_table`] — dense-match jump-table dispatch
//!   (`visit_match_jump_table`).
//! - [`match_dispatch`] — cmp/jne cascade dispatch + guard evaluation
//!   (`visit_match`, `emit_guard_expression`).
//! - [`arm_body`] — App-node arm body emission (`emit_arm_body_app`).
//!
//! All extracted methods remain on `impl EmitWalker` and preserve their
//! previous `pub(crate)` / private visibility. No behavior change; no
//! public path change.

mod diagnostics;
mod scrutinee;
mod enum_cons;
mod pattern_lower;
mod match_jump_table;
mod match_dispatch;
mod arm_body;

#[cfg(test)]
mod tests;
