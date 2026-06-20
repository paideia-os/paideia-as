//! Hardware Security Module (HSM) backends.
//!
//! This module provides abstraction layers for different HSM implementations.
//! Currently supports:
//! - PKCS#11 (cryptoki) backend for hardware and software HSMs

pub mod pkcs11;

pub use pkcs11::{Pkcs11Error, Pkcs11Signer};
