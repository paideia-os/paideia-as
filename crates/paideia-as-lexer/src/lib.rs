//! paideia-as-lexer
//!
//! UTF-8 + BOM-aware source-text validation and (in later PRs) the
//! token-stream lexer for paideia-as source files. See
//! `design/toolchain/syntax-reference.md` for the spec.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod reserved;
mod scan_ident;
mod source;
mod token;
mod trivia;

pub use scan_ident::{IdentScan, scan_identifier};
pub use source::SourceText;
pub use token::{RESERVED_WORDS, Token, TokenKind, keyword_kind};
pub use trivia::{Trivia, TriviaKind};
