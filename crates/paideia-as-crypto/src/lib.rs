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
//! - `hash` — cryptographic hash functions.
//!   - `hash::sha256` (FIPS 180-4 §6.2) — one-shot [`hash::sha256`] and
//!     streaming [`hash::Sha256Ctx`]; foundational for HMAC-SHA256 / HKDF
//!     (RFC 5869) and TLS 1.3's transcript hash.
//!   - `hash::sha512` (FIPS 180-4 §6.4) — one-shot [`hash::sha512`] and
//!     streaming [`hash::Sha512Ctx`]; foundational for Ed25519 (RFC 8032).
//! - `kdf`  — password-based key-derivation functions.
//!   - `kdf::Argon2id` (RFC 9106).
//! - `aead` — authenticated encryption with associated data.
//!   - `aead::ChaCha20Poly1305` (RFC 8439).
//! - `kem`  — key-encapsulation mechanisms.
//!   - `kem::MlKem768` (FIPS 203) — Module-Lattice-based KEM,
//!     category-3 parameter set (paideia-as#1352). Consumed by
//!     R108 (multi-user identity substrate) for device-to-device key
//!     agreement and by R91+ networking for post-quantum handshakes.
//! - `rng`  — secure random source.
//!   - `rng::HardwareRng` (RDSEED preferred, RDRAND fallback).
//! - `ffi`  — C-ABI thunks over the trait APIs, called by `.pdx` code
//!   through the `stdlib_lowering::cryptoops` recipes
//!   (paideia-as#1305). This is the only module in the crate that
//!   touches raw pointers.
//!
//! Design rules for this crate:
//!
//! 1. Traits are the primary API. Concrete impls (e.g. `Argon2id`,
//!    `ChaCha20Poly1305`, `HardwareRng`) live behind the trait so
//!    `stdlib_lowering` recipes can be swapped for a paideia-native
//!    implementation without touching callers.
//! 2. Every impl ships a reference-vector test drawn from the cited
//!    standard (or, for hardware-TRNG primitives, a mock-driven
//!    retry-loop test plus a live hardware smoke test). Vectors are
//!    treated as `pub` byte constants so downstream integration
//!    suites (issue v0.33-005) can re-use them.
//! 3. No `unsafe` outside the tightly-scoped x86_64 intrinsic
//!    wrappers in `rng::hardware` (RDSEED, RDRAND, PAUSE) — those
//!    are unavoidable for hardware entropy access and each block
//!    carries a SAFETY note tying the invocation to the CPUID
//!    feature check performed at `HardwareRng` construction.
//!    No allocation on the hot path beyond what the underlying
//!    primitive requires.
//!
//! # `no_std + alloc` posture
//!
//! The crate builds as `no_std + alloc` for library / staticlib
//! outputs — see `#![cfg_attr(not(test), no_std)]` below — so the
//! resulting archive can be linked into a satellite host tool (via
//! `paideia-satellite-runtime`) without dragging `std` / `libc` /
//! `_Unwind_*` scaffolding onto that tool's link line. The satellite
//! consumer must supply its own `#[global_allocator]`; this crate
//! declares none (that decision belongs to the final binary).
//!
//! Under `cfg(test)` the crate is compiled as std so `cargo test`
//! keeps working with the standard test harness; test code may use
//! `std::` freely.

#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]
#![deny(unsafe_code)]

extern crate alloc;

pub mod aead;
pub mod curve;
pub mod ffi;
pub mod hash;
pub mod kdf;
pub mod kem;
pub mod rng;

pub use aead::{Aead, AeadError, ChaCha20Poly1305, ChaCha20Poly1305Params};
pub use curve::{
    ed25519_public_from_secret, ed25519_sign, ed25519_verify, x25519_public_from_secret,
    x25519_scalarmult,
};
pub use hash::{Sha256Ctx, Sha512Ctx, sha256, sha512};
pub use kdf::{
    Argon2id, Argon2idParams, HkdfExpandError, Kdf, KdfError, hkdf_expand, hkdf_extract,
    hmac_sha256,
};
// paideia-as#1354 — one `pub use` per symbol so future append-only
// primitive additions never collide inside `{ … }` alongside another
// wave's authoring. Same shape carried into
// `paideia-satellite-runtime/src/lib.rs`'s `pub use` block.
pub use kem::CT_LEN as ML_KEM_768_CT_LEN;
pub use kem::DK_LEN as ML_KEM_768_DK_LEN;
pub use kem::EK_LEN as ML_KEM_768_EK_LEN;
pub use kem::KemError;
pub use kem::MlKem768;
pub use kem::SEED_LEN as ML_KEM_768_SEED_LEN;
pub use kem::SS_LEN as ML_KEM_768_SS_LEN;
pub use rng::{EntropySource, HardwareRng, RngError, SecureRandom};
