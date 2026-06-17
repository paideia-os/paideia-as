#![warn(missing_docs)]
#![forbid(unsafe_code)]
//! Diagnostic substrate for paideia-as. See `design/toolchain/diagnostics.md`.

mod code;

pub use code::{Category, CodeParseError, DiagnosticCode, Severity};
