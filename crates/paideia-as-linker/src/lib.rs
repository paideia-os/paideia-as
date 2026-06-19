//! paideia-link — the PAX-format linker.
//!
//! 4-phase pipeline: parse → resolve → relocate → emit.
//! m4-009 ships the parse phase; m4-010..012 ship the rest.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod parse;
pub mod resolve;

pub use parse::{ParseError, ParsedPax, parse_inputs, parse_pax};
pub use resolve::{
    GlobalCapabilityTable, GlobalSymbolTable, ResolvedLink, ResolvedPax, resolve_inputs,
};
