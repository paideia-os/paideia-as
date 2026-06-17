#![warn(missing_docs)]
#![forbid(unsafe_code)]
//! Diagnostic substrate for paideia-as. See `design/toolchain/diagnostics.md`.

mod code;
mod source_map;
mod span;

pub use code::{Category, CodeParseError, DiagnosticCode, Severity};
pub use source_map::{LineCol, SourceMap};
pub use span::{FileId, Span};
