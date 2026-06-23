//! paideia-as-encoder
//!
//! Shared x86_64 instruction encoder used by both ELF and PE backends.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod dispatch;
pub mod encode;
pub mod encode_and_or_xor;
pub mod encode_imul;
pub mod encode_instruction;
pub use dispatch::{DispatchKind, classify};
pub use encode::*;
pub use encode_instruction::{
    EncodeError, EncodeOutput, EncodeStats, LabelFixup, RelocKind, RelocSite, encode_instruction,
};
