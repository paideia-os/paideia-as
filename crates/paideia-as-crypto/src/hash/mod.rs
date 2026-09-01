//! Cryptographic hash functions.
//!
//! - `sha256` — FIPS 180-4 §6.2 SHA-256 (portable u32 implementation).
//! - `sha512` — FIPS 180-4 §6.4 SHA-512 (portable u64 implementation) —
//!   added in #1341 as an Ed25519 dependency (RFC 8032 §5.1).
//!
//! Written from scratch with no external dependency; the SHA-NI hardware
//! acceleration path is intentionally deferred (issue #1338 discussion) —
//! it can slot in behind the same one-shot / streaming API once we're on
//! real hardware.

pub mod sha256;
pub mod sha512;

pub use sha256::{Sha256Ctx, sha256};
pub use sha512::{Sha512Ctx, sha512};
