//! Cryptographic hash functions.
//!
//! - `sha256` — FIPS 180-4 §6.2 SHA-256 (portable u32 implementation).
//!
//! Written from scratch with no external dependency; the SHA-NI hardware
//! acceleration path is intentionally deferred (issue #1338 discussion) —
//! it can slot in behind the same one-shot / streaming API once we're on
//! real hardware. SHA-512 will land alongside Ed25519 (#1341) which needs it.

pub mod sha256;

pub use sha256::{Sha256Ctx, sha256};
