//! paideia-as-elaborator
//!
//! AST → IR lowering and (in later PRs) type/effect/capability checking
//! passes. See `design/toolchain/custom-assembler.md` §6.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod branch_merge;
pub mod capture;
pub mod check_expr;
pub mod check_lambda;
pub mod check_linearity;
pub mod effect_infer;
pub mod env;
pub mod linearity_ctx;
mod lower;
mod placeholder_emit;

pub use branch_merge::{S_BRANCH_MISMATCH, merge_branches};
pub use capture::{CaptureKind, CapturedBinding, analyze_captures};
pub use check_expr::{InferOutcome, check_annotation, infer_node};
pub use check_lambda::{S_ILLEGAL_CAPTURE, check_lambda};
pub use check_linearity::{S_NEVER_USED, S_OVERUSED, validate_scope};
pub use effect_infer::{
    F_UNHANDLED_EFFECT, RowOutcome, call_row, check_no_unhandled, compose_rows, handle_row,
    perform_row,
};
pub use env::{Symbol, TypeEnv};
pub use linearity_ctx::{Binding, LinearityCtx};
pub use lower::{LoweringResult, lower_ast_to_ir};
pub use placeholder_emit::placeholder_for;
