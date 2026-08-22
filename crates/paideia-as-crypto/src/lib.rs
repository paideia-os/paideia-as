//! paideia-as-crypto — cryptographic primitives for paideia-as tooling
//! and (via `stdlib_lowering`) the PaideiaOS runtime.
//!
//! The crate is deliberately narrow: it exposes traits and reference
//! implementations for the primitives PaideiaOS depends on for user
//! authentication and at-rest key sealing. Each primitive is anchored
//! to a normative reference (RFC / NIST publication) so the trait shape
//! and the vector suite move together.
//!
//! Layout:
//!
//! - `kdf`  — password-based key-derivation functions.
//!   - `kdf::Argon2id` (RFC 9106).
//! - `aead` — authenticated encryption with associated data.
//!   - `aead::ChaCha20Poly1305` (RFC 8439).
//!
//! Future issues in the v0.33 bundle add:
//!
//! - `rng`  — secure random source (RDSEED / RDRAND fallback).
//!
//! Design rules for this crate:
//!
//! 1. Traits are the primary API. Concrete impls (e.g. `Argon2id`,
//!    `ChaCha20Poly1305`) live behind the trait so `stdlib_lowering`
//!    recipes can be swapped for a paideia-native implementation
//!    without touching callers.
//! 2. Every impl ships a reference-vector test drawn from the cited
//!    standard. Vectors are treated as `pub` byte constants so
//!    downstream integration suites (issue v0.33-005) can re-use them.
//! 3. No `unsafe`. No allocation on the hot path beyond what the
//!    underlying primitive requires.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod aead;
pub mod kdf;

pub use aead::{Aead, AeadError, ChaCha20Poly1305, ChaCha20Poly1305Params};
pub use kdf::{Argon2id, Argon2idParams, Kdf, KdfError};
