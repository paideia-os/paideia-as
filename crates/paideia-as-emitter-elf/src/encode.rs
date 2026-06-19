//! Facade module re-exporting the shared encoder.
//!
//! See paideia-as-encoder for the canonical API. Maintained as a
//! facade so existing callsites that use `crate::encode::Reg64`
//! continue to compile without modification.

pub use paideia_as_encoder::*;
