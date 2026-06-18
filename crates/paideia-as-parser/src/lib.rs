//! paideia-as-parser
//!
//! Hand-written Pratt + recursive-descent parser for paideia-as source
//! files. See `design/toolchain/syntax-reference.md` §7 (operator
//! precedence) and §8 (grammar EBNF). Parser diagnostics live in the
//! `P0100-P0299` range per `diagnostics.md` §2.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod cursor;
mod parse_action;
mod parse_control;
mod parse_expr;
mod parse_handler;
mod parse_lambda;
mod parse_match;
mod parse_postfix;
mod parse_prefix;
mod parse_primary;
mod parse_stmt;
mod parse_type;
mod parse_unsafe;
mod parser;
mod precedence;

pub use cursor::TokenCursor;
pub use parser::{ParseError, Parser};
