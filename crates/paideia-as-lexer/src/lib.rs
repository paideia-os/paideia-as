//! paideia-as-lexer
//!
//! UTF-8 + BOM-aware source-text validation and (in later PRs) the
//! token-stream lexer for paideia-as source files. See
//! `design/toolchain/syntax-reference.md` for the spec.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod source;

pub use source::SourceText;
