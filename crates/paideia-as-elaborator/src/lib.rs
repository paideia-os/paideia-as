//! paideia-as-elaborator
//!
//! AST → IR lowering and (in later PRs) type/effect/capability checking
//! passes. See `design/toolchain/custom-assembler.md` §6.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod check_expr;
pub mod check_linearity;
pub mod env;
pub mod linearity_ctx;
mod lower;
mod placeholder_emit;

pub use check_expr::{InferOutcome, check_annotation, infer_node};
pub use check_linearity::{S_NEVER_USED, S_OVERUSED, validate_scope};
pub use env::{Symbol, TypeEnv};
pub use linearity_ctx::{Binding, LinearityCtx};
pub use lower::{LoweringResult, lower_ast_to_ir};
pub use placeholder_emit::placeholder_for;
