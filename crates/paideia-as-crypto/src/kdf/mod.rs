//! Key-derivation functions.
//!
//! The `Kdf` trait abstracts over password-based KDFs so that PaideiaOS
//! call sites (passphrase unlock in `design/user/model.md` §2.1, sealed
//! `user_sk` in §9.2) never depend on a specific implementation crate.
//!
//! Current impls:
//! - [`Argon2id`] — RFC 9106, backed by the `argon2` crate.
//!
//! Companion primitives (not password-based, but part of the KDF surface
//! TLS 1.3 depends on — see `libpdx-net` M3):
//! - [`hkdf::hmac_sha256`] — RFC 2104 / FIPS 198-1.
//! - [`hkdf::hkdf_extract`] / [`hkdf::hkdf_expand`] — RFC 5869.

mod argon2id;
pub mod hkdf;

pub use argon2id::{Argon2id, Argon2idParams, RFC_9106_ARGON2ID_TAG};
pub use hkdf::{hkdf_expand, hkdf_extract, hmac_sha256, HkdfExpandError};

use thiserror::Error;

/// Errors that a KDF implementation may return.
///
/// Individual impls narrow these into concrete diagnostics via the
/// `#[from]` variants. Keeping the enum in the shared module means
/// downstream code can bind on `KdfError` regardless of which KDF
/// produced the failure — helpful when the same call site is exercised
/// against a soft/HSM/paideia-native backend.
#[derive(Debug, Error)]
pub enum KdfError {
    /// A parameter fell outside the range permitted by the specification.
    ///
    /// The message is expected to name the offending field (e.g.
    /// `"m_cost < 8 * p_cost"`) so tests and diagnostics can pin on it.
    #[error("invalid KDF parameter: {0}")]
    InvalidParams(&'static str),

    /// The output buffer was rejected by the underlying primitive
    /// (too short, too long, or wrong length for a tag-anchored mode).
    #[error("invalid output length: {0} bytes")]
    InvalidOutputLen(usize),

    /// The primitive failed internally. Kept generic because the
    /// underlying crate's error type is not stable across major
    /// versions.
    #[error("KDF primitive failure: {0}")]
    Primitive(String),
}

/// A password-based key-derivation function.
///
/// The `Params` associated type carries all inputs to the derivation
/// (password, salt, tuning parameters, optional associated data /
/// secret pepper). This lets callers construct parameters once and
/// re-derive against multiple output buffers without re-plumbing
/// method arguments each time a new primitive is added.
///
/// Implementations MUST be deterministic: the same `params` and
/// `output.len()` MUST always yield the same bytes.
pub trait Kdf {
    /// Concrete parameter bundle for this KDF.
    type Params<'a>
    where
        Self: 'a;

    /// Derive `output.len()` bytes of key material into `output`.
    ///
    /// Returns `Err(KdfError::InvalidOutputLen)` if the primitive
    /// rejects `output.len()` (Argon2 requires 4 ≤ len ≤ 2^32 − 1).
    fn derive(params: &Self::Params<'_>, output: &mut [u8]) -> Result<(), KdfError>;
}
