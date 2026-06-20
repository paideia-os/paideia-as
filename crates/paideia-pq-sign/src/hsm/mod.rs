//! Hardware Security Module (HSM) backends.
//!
//! This module provides abstraction layers for different HSM implementations.
//! Currently supports:
//! - PKCS#11 (cryptoki) backend for hardware and software HSMs
//! - YubiHSM2 backend with hybrid fallback (Ed25519 hardware + ML-DSA-65 soft-HSM)

pub mod pkcs11;
pub mod yubihsm;

pub use pkcs11::{Pkcs11Error, Pkcs11Signer};
pub use yubihsm::{YubiHsmError, YubiHsmSigner};
