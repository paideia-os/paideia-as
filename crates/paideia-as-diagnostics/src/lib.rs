#![warn(missing_docs)]
#![forbid(unsafe_code)]
//! Diagnostic substrate for paideia-as.
//!
//! Defines stable diagnostic identifiers, source positions, and the
//! `Diagnostic` value type emitted by every paideia-as pass. See
//! `design/toolchain/diagnostics.md` for the catalog rules.

mod builder;
mod catalog;
mod code;
mod diagnostic;
mod source_map;
mod span;

pub use builder::DiagnosticBuilder;
pub use catalog::{Catalog, CatalogEntry, CatalogError};
pub use code::{Category, CodeParseError, DiagnosticCode, Severity};
pub use diagnostic::{Diagnostic, SecondarySpan, SuggestedFix};
pub use source_map::{LineCol, SourceMap};
pub use span::{FileId, Span};
