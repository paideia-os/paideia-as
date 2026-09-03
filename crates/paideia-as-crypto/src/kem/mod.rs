//! Key-encapsulation mechanisms (KEMs).
//!
//! Each KEM impl is a marker struct with three associated functions
//! (`keygen`, `encaps`, `decaps`) exposed on top of fixed-size byte
//! buffers whose lengths are compile-time constants of the underlying
//! parameter set. That shape mirrors how [`crate::Argon2id`] and
//! [`crate::ChaCha20Poly1305`] evolved before their `Kdf` / `Aead`
//! traits landed — a full `Kem` trait is not defined here because
//! only one KEM impl currently exists and every parameter set has a
//! different concrete size table. The trait can be added later
//! (behind a generic `type Sizes`-style associated type family)
//! without breaking existing callers.
//!
//! Current impls:
//! - [`MlKem768`] — Module-Lattice-based Key-Encapsulation Mechanism,
//!   [FIPS 203] category-3 parameter set (formerly Kyber768). Backed
//!   by the `ml-kem` crate from RustCrypto, running with the
//!   `deterministic` feature so seed-driven KeyGen / Encaps reproduce
//!   byte-identically against NIST ACVP known-answer vectors.
//!
//! Intended consumers:
//! - R108 (paideia-os multi-user identity substrate) for device-to-
//!   device key agreement.
//! - R91+ networking milestones for TLS/Noise-shaped post-quantum
//!   handshakes.
//!
//! [FIPS 203]: https://csrc.nist.gov/pubs/fips/203/final
//!
//! Design invariants shared by every impl:
//!
//! 1. **Deterministic under fixed seeds.** Every KEM's KeyGen and
//!    Encaps accepts an explicit random-tape seed and, given the same
//!    seed, MUST produce byte-identical output. Seeds are fixed-size
//!    `&[u8; N]`; no impl fabricates entropy internally. Callers
//!    sample fresh seeds via `crate::rng`.
//! 2. **Fixed-size buffers on the surface.** Public keys / cipher-
//!    texts / shared secrets are all fixed-length byte arrays per the
//!    normative spec, exposed as `pub const` sizes (`EK_LEN`, `DK_LEN`,
//!    `CT_LEN`, `SS_LEN`). The FFI thunks in `crate::ffi` cast raw
//!    pointers to fixed-size array references without runtime length
//!    checks — the sizes are ABI, not inputs.
//! 3. **Decapsulation is infallible on the ML-KEM surface.** FIPS 203
//!    specifies implicit rejection: a tampered ciphertext decapsulates
//!    to a pseudo-random shared secret derived from `dk`'s implicit-
//!    rejection secret `z`, NOT an error. Callers are responsible for
//!    detecting mismatch through an authenticated channel that wraps
//!    the shared secret. This mirrors the RustCrypto `ml-kem` crate's
//!    `Decapsulate::decapsulate -> Result<_, ()>` shape (its `Err` is
//!    reserved for future decoding failures at the encoded-buffer
//!    boundary, which our fixed-length inputs cannot trigger).

mod ml_kem_768;

pub use ml_kem_768::{
    ACVP_DE_C, ACVP_DE_DK, ACVP_DE_K,
    // ACVP encap vector (function=encapsulation, tcId 26).
    ACVP_EN_C, ACVP_EN_EK, ACVP_EN_K, ACVP_EN_M,
    // ACVP keyGen vector (function=keyGen, tcId 26).
    ACVP_KG_D, ACVP_KG_DK, ACVP_KG_EK, ACVP_KG_Z,
    CT_LEN, DK_LEN, EK_LEN, MlKem768, SEED_LEN, SS_LEN,
};

use alloc::string::String;

use thiserror::Error;

/// Errors a KEM implementation may return on the trait surface.
///
/// The variants are intentionally narrow: seed / key / ciphertext
/// lengths on the trait API are `&[u8; N]` (checked at compile time),
/// and ML-KEM's implicit-rejection contract means decapsulation
/// itself never fails. `Primitive` is kept as an escape hatch for
/// underlying-crate errors we do not want to bake into our public
/// error surface — a future impl may surface conditions the current
/// one cannot.
#[derive(Debug, Error)]
pub enum KemError {
    /// A parameter fell outside the range permitted by the specification.
    #[error("invalid KEM parameter: {0}")]
    InvalidParams(&'static str),

    /// The primitive failed internally. Kept generic because the
    /// underlying crate's error type is not stable across major
    /// versions.
    #[error("KEM primitive failure: {0}")]
    Primitive(String),
}
