//! Authenticated encryption with associated data (AEAD).
//!
//! The [`Aead`] trait abstracts over AEAD ciphers so PaideiaOS call
//! sites (sealed `user_sk` in `design/user/model.md` §9.2, at-rest key
//! wrapping in the soft-HSM) never depend on a specific implementation
//! crate. Every AEAD impl is anchored to its normative RFC / NIST
//! reference and ships an in-crate vector suite drawn from it.
//!
//! Current impls:
//! - [`ChaCha20Poly1305`] — RFC 8439, backed by the `chacha20poly1305`
//!   crate.
//!
//! Design invariants shared by every impl:
//!
//! 1. The tag is appended to the ciphertext on the wire (the RFC 8439
//!    §2.8.1 convention adopted by the `aead` crate ecosystem). The
//!    trait therefore returns / accepts a single byte buffer:
//!    `ciphertext || tag`. Callers that need the tag as a separate
//!    field slice off the last N bytes where N is the impl's tag
//!    length (e.g. [`TAG_LEN`] for ChaCha20-Poly1305).
//! 2. Nonces are the caller's responsibility. Reusing a nonce with the
//!    same key destroys confidentiality (chacha20 is a stream cipher)
//!    and authentication (poly1305 is a Wegman-Carter MAC). The trait
//!    takes nonces by value inside `Params` so a fresh one is a visible
//!    input at every callsite; no impl fabricates one internally.
//! 3. Authentication is not optional. `open` MUST return
//!    [`AeadError::AuthenticationFailed`] on any tag mismatch, and MUST
//!    NOT return partial plaintext.

mod chacha20_poly1305;

pub use chacha20_poly1305::{
    ChaCha20Poly1305, ChaCha20Poly1305Params, KEY_LEN, NONCE_LEN, TAG_LEN,
    // RFC 8439 §2.8.2 encryption vector (re-exported for downstream
    // integration suites in issue v0.33-005).
    RFC_8439_SEC_2_8_2_AAD, RFC_8439_SEC_2_8_2_CIPHERTEXT, RFC_8439_SEC_2_8_2_KEY,
    RFC_8439_SEC_2_8_2_NONCE, RFC_8439_SEC_2_8_2_PLAINTEXT, RFC_8439_SEC_2_8_2_TAG,
};

use thiserror::Error;

/// Errors an AEAD implementation may return.
///
/// Kept generic across impls so mixed backends (soft, HSM, paideia-
/// native) share one caller-visible failure enum.
#[derive(Debug, Error)]
pub enum AeadError {
    /// The key slice length did not match the cipher's key size.
    #[error("invalid AEAD key length: {got} bytes (expected {expected})")]
    InvalidKeyLen {
        /// Length the caller supplied.
        got: usize,
        /// Length the primitive requires.
        expected: usize,
    },

    /// The nonce slice length did not match the cipher's nonce size.
    #[error("invalid AEAD nonce length: {got} bytes (expected {expected})")]
    InvalidNonceLen {
        /// Length the caller supplied.
        got: usize,
        /// Length the primitive requires.
        expected: usize,
    },

    /// A tagged buffer was too short to contain the authentication tag.
    #[error("AEAD ciphertext shorter than tag: {got} bytes (need at least {expected})")]
    CiphertextTooShort {
        /// Length the caller supplied.
        got: usize,
        /// Minimum length required (the tag length).
        expected: usize,
    },

    /// The tag did not authenticate the ciphertext + associated data.
    ///
    /// The impl MUST NOT return partial plaintext when this variant
    /// fires — the plaintext buffer, if the impl allocated one, is
    /// zeroed / dropped before the error is returned.
    #[error("AEAD authentication failed")]
    AuthenticationFailed,

    /// The primitive failed internally. Kept generic because the
    /// underlying crate's error type is not stable across major
    /// versions.
    #[error("AEAD primitive failure: {0}")]
    Primitive(String),
}

/// An authenticated encryption cipher with associated-data support.
///
/// Implementations MUST be deterministic under a fixed `(key, nonce,
/// aad, plaintext)`: seal always produces the same ciphertext + tag,
/// and open of that ciphertext always returns the original plaintext.
///
/// The associated `Params` type carries all inputs to a single seal /
/// open call — key, nonce, and associated data. This mirrors the shape
/// used by [`crate::kdf::Kdf`] so callers can build parameter bundles
/// once and reuse them across multiple invocations.
pub trait Aead {
    /// Concrete parameter bundle for this AEAD.
    type Params<'a>
    where
        Self: 'a;

    /// Encrypt `plaintext` under `params` and return
    /// `ciphertext || tag`.
    ///
    /// The output length is `plaintext.len()` plus the impl's tag
    /// length (16 bytes for ChaCha20-Poly1305).
    /// The nonce inside `params` MUST be unique per key across the
    /// entire lifetime of that key — the AEAD does not enforce this.
    fn seal(params: &Self::Params<'_>, plaintext: &[u8]) -> Result<Vec<u8>, AeadError>;

    /// Verify the authentication tag over `params.aad || ciphertext`
    /// and, on success, return the recovered plaintext.
    ///
    /// The `sealed` buffer is `ciphertext || tag` — the same layout
    /// produced by [`Aead::seal`].
    ///
    /// Returns [`AeadError::AuthenticationFailed`] on any tag
    /// mismatch. Callers MUST treat this as an unrecoverable auth
    /// failure and MUST NOT retry with a modified buffer — a
    /// modified-buffer retry is how padding-oracle attacks are built.
    fn open(params: &Self::Params<'_>, sealed: &[u8]) -> Result<Vec<u8>, AeadError>;
}
