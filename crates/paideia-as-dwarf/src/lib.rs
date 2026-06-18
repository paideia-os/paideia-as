//! paideia-as-dwarf
//!
//! DWARF 5 emitter for paideia-as object files per
//! `design/toolchain/debug-info.md`.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod info;
pub mod line;
pub mod vendor;

pub use info::{CompilationUnit, FunctionDie, build_cu};
pub use line::LineEntry;
pub use vendor::{VENDOR_SECTIONS, empty_vendor_payloads};
