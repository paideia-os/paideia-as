//! paideia-as-elaborator
//!
//! AST → IR lowering and (in later PRs) type/effect/capability checking
//! passes. See `design/toolchain/custom-assembler.md` §6.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod lower;
mod placeholder_emit;

pub use lower::{LoweringResult, lower_ast_to_ir};
pub use placeholder_emit::placeholder_for;
