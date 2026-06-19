//! paideia-as-emitter-pax
//!
//! PAX (PaideiaOS Architectural Executable) emitter. PAX is the
//! canonical PaideiaOS object format carrying capability sigs,
//! effect rows, PQ signatures, BLAKE3 content hashes.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod header;

pub use header::{
    Architecture, HeaderFlag, PAX_FORMAT_VERSION, PAX_HEADER_SIZE, PAX_MAGIC, PaxHeader,
};
