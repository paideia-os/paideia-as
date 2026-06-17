//! paideia-as-lexer
//!
//! UTF-8 + BOM-aware source-text validation and (in later PRs) the
//! token-stream lexer for paideia-as source files. See
//! `design/toolchain/syntax-reference.md` for the spec.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod glyph_table;
pub mod reserved;
mod scan_char;
mod scan_comment;
mod scan_ident;
mod scan_number;
mod scan_op;
mod scan_string;
mod scan_ws;
mod source;
mod token;
mod trivia;

pub use scan_char::{CharScan, scan_char};
pub use scan_comment::{CommentScan, scan_comment};
pub use scan_ident::{IdentScan, scan_identifier};
pub use scan_number::{NumberScan, scan_number};
pub use scan_op::{AsciiMode, OpScan, scan_op};
pub use scan_string::{StringScan, scan_string};
pub use scan_ws::{WhitespaceScan, scan_whitespace};
pub use source::SourceText;
pub use token::{RESERVED_WORDS, Token, TokenKind, keyword_kind};
pub use trivia::{Trivia, TriviaKind};
