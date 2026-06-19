//! paideia-pq-sign — post-quantum signing for paideia-as / PaideiaOS.
//!
//! Wires Ed25519 (classical) and ML-DSA-65 (PQ) behind a common
//! Signer trait. m7-002 ships the hybrid composition.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod ed25519;
pub mod hybrid;
pub mod mldsa;

pub use ed25519::Ed25519;
pub use hybrid::{Hybrid, HybridPublicKey, HybridSecretKey, HybridSignature};
pub use mldsa::MlDsa65Marker;

/// Ed25519 public key length (bytes).
pub const ED25519_PK_LEN: usize = 32;
/// Ed25519 secret key length (bytes).
pub const ED25519_SK_LEN: usize = 32;
/// Ed25519 signature length (bytes).
pub const ED25519_SIG_LEN: usize = 64;

/// ML-DSA-65 public key length (bytes).
pub const MLDSA65_PK_LEN: usize = 1952;
/// ML-DSA-65 secret key length (bytes, seed only).
pub const MLDSA65_SK_LEN: usize = 32;
/// ML-DSA-65 signature length (bytes).
pub const MLDSA65_SIG_LEN: usize = 3309;

/// Hybrid public key length (bytes).
pub const HYBRID_PK_LEN: usize = ED25519_PK_LEN + MLDSA65_PK_LEN;
/// Hybrid secret key length (bytes, seed form).
pub const HYBRID_SK_LEN: usize = ED25519_SK_LEN + MLDSA65_SK_LEN;
/// Hybrid signature length (bytes).
pub const HYBRID_SIG_LEN: usize = ED25519_SIG_LEN + MLDSA65_SIG_LEN;

/// Common trait for post-quantum and classical signers.
pub trait Signer {
    /// The secret key type for this signer.
    type SecretKey;
    /// The public key type for this signer.
    type PublicKey;
    /// The signature type for this signer.
    type Signature: AsRef<[u8]>;

    /// Generate a new keypair using the provided RNG.
    fn keygen<R: rand_core::RngCore + rand_core::CryptoRng>(
        rng: &mut R,
    ) -> (Self::PublicKey, Self::SecretKey);

    /// Sign a message with the secret key.
    fn sign(sk: &Self::SecretKey, message: &[u8]) -> Self::Signature;

    /// Verify a signature over a message with the public key.
    fn verify(pk: &Self::PublicKey, message: &[u8], sig: &Self::Signature) -> bool;
}
