//! paideia-as-encoder
//!
//! Shared x86_64 instruction encoder used by both ELF and PE backends.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod encode;
pub mod encode_instruction;
pub use encode::*;
pub use encode_instruction::{EncodeError, encode_instruction};
