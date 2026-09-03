//! paideia-as-parser
//!
//! Hand-written Pratt + recursive-descent parser for paideia-as source
//! files. See `design/toolchain/syntax-reference.md` §7 (operator
//! precedence) and §8 (grammar EBNF). Parser diagnostics live in the
//! `P0100-P0299` range per `diagnostics.md` §2.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod cursor;
mod endian_attr;
mod functor;
mod gpu_context;
mod modules;
mod packed_struct;
mod parse_action;
mod parse_control;
mod parse_expr;
mod parse_handler;
mod parse_item;
mod parse_lambda;
mod parse_macro;
mod parse_match;
mod parse_memref;
mod parse_pattern;
mod parse_postfix;
mod parse_prefix;
mod parse_primary;
mod parse_stmt;
mod parse_type;
mod parse_unsafe;
mod parser;
mod precedence;
mod quote;
mod timeline;

pub use cursor::TokenCursor;
pub use endian_attr::{Endianness, parse_endian_attr};
pub use functor::{FunctorDecl, SessionBinding, parse_functor};
pub use gpu_context::{GpuContextBlock, parse_gpu_context};
pub use packed_struct::parse_packed_struct;
pub use parser::{ParseError, Parser};
pub use timeline::{TimelineOp, TimelineOpKind, parse_timeline_signal, parse_timeline_wait};
