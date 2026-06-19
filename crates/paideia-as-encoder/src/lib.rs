//! paideia-as-encoder
//!
//! Shared x86_64 instruction encoder used by both ELF and PE backends.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod encode;
pub use encode::*;
