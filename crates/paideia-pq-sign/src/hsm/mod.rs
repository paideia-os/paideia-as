//! Hardware Security Module (HSM) backends.
//!
//! This module provides abstraction layers for different HSM implementations.
//! Currently supports:
//! - PKCS#11 (cryptoki) backend for hardware and software HSMs
//! - YubiHSM2 backend with hybrid fallback (Ed25519 hardware + ML-DSA-65 soft-HSM)

pub mod hybrid;
pub mod pkcs11;
pub mod yubihsm;

pub use hybrid::HybridSigner;
pub use pkcs11::{Pkcs11Error, Pkcs11Signer};
pub use yubihsm::{YubiHsmError, YubiHsmSigner};

use thiserror::Error;

/// Errors from HSM signer operations.
#[derive(Debug, Error)]
pub enum HsmSignerError {
    /// PKCS#11 error.
    #[error("PKCS#11 error: {0}")]
    Pkcs11(#[from] Pkcs11Error),

    /// YubiHSM error.
    #[error("YubiHSM error: {0}")]
    YubiHsm(#[from] YubiHsmError),

    /// Generic signing error.
    #[error("signing error: {0}")]
    Sign(String),
}

/// Common trait for HSM-backed signers.
///
/// Provides a unified interface for signing with Ed25519 and ML-DSA-65 keys
/// across different HSM backends. Implementations include:
/// - `Pkcs11Signer` (hardware PKCS#11 HSM)
/// - `YubiHsmSigner` (YubiHSM2 hardware for Ed25519 + soft fallback for ML-DSA-65)
/// - `SoftHsm` (development-only software HSM with Argon2id + ChaCha20-Poly1305)
pub trait HsmSigner: Send + Sync {
    /// Sign a message with the Ed25519 key.
    fn sign_ed25519(&self, msg: &[u8]) -> Result<Vec<u8>, HsmSignerError>;

    /// Sign a message with the ML-DSA-65 key.
    fn sign_mldsa65(&self, msg: &[u8]) -> Result<Vec<u8>, HsmSignerError>;

    /// Returns true if the Ed25519 key is protected by hardware (HSM, TPM, or
    /// YubiHSM2 firmware). Phase-3 m6-003: ML-DSA-65 is always soft today; this
    /// returns the Ed25519-leg's hardware status only.
    ///
    /// This predicate enables callers to introspect their signer backend
    /// and make policy decisions about key backup, delegation, etc.
    fn is_hardware(&self) -> bool;
}
