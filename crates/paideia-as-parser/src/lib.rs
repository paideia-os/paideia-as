//! paideia-as-parser
//!
//! Hand-written Pratt + recursive-descent parser for paideia-as source
//! files. See `design/toolchain/syntax-reference.md` §7 (operator
//! precedence) and §8 (grammar EBNF). Parser diagnostics live in the
//! `P0100-P0299` range per `diagnostics.md` §2.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod cursor;
mod parser;

pub use cursor::TokenCursor;
pub use parser::{ParseError, Parser};
