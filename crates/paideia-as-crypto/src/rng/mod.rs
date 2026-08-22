//! Cryptographically-secure random number sources.
//!
//! The [`SecureRandom`] trait abstracts over hardware and (future)
//! paideia-native entropy sources so PaideiaOS call sites — sealed
//! `user_sk` salt sampling (`design/user/model.md` §9.2), nonce
//! generation for AEAD, ephemeral post-quantum keypair setup — never
//! depend on a specific implementation.
//!
//! Current impl:
//! - [`HardwareRng`] — CPU RDSEED (preferred) with RDRAND fallback,
//!   selected at construction via CPUID feature detection.
//!
//! # Reference
//!
//! Intel, *"Intel Digital Random Number Generator (DRNG) Software
//! Implementation Guide"*, revision 2.1, October 2018. Sections 4.2
//! and 4.3 govern the retry contract; section 5.2 the CPUID feature
//! detection.
//!
//! NIST SP 800-90B classifies RDSEED as a Non-deterministic Random
//! Bit Generator (NRBG); RDRAND is a Deterministic RBG (DRBG) seeded
//! by the same underlying entropy source. Both are appropriate for
//! symmetric-key and nonce generation. Long-term asymmetric keypair
//! generation SHOULD prefer RDSEED (or another NRBG) — hence RDSEED
//! is chosen first when available.
//!
//! # Design invariants
//!
//! 1. **Consume only what you need.** The trait fills the caller's
//!    exact-sized buffer and returns. No reservoir is cached between
//!    calls: leftover entropy is a compromise vector (a snapshot of
//!    the reservoir can predict future values), and PaideiaOS callers
//!    ask for small buffers (12-byte nonces, 32-byte salts) where a
//!    reservoir buys nothing.
//! 2. **Distinguish `Unavailable` from `Exhausted`.** `Unavailable`
//!    is a permanent CPU-capability failure (returned once by
//!    [`HardwareRng::new`]). `Exhausted` is a transient hardware
//!    entropy exhaustion (returned after the retry budget); the
//!    caller MAY retry after a backoff.
//! 3. **No paideia-native impl silently swaps in.** New impls land
//!    as explicit types so callers can reason about the entropy
//!    source at their call site.

mod hardware;

pub use hardware::{EntropySource, HardwareRng, RDRAND_RETRIES, RDSEED_RETRIES};

use thiserror::Error;

/// Errors a secure-random source may return.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RngError {
    /// Neither RDSEED nor RDRAND is present on this CPU.
    ///
    /// Emitted once by [`HardwareRng::new`]; a successfully-
    /// constructed [`HardwareRng`] cannot return this from `fill`.
    #[error("no hardware entropy source available (neither RDSEED nor RDRAND)")]
    Unavailable,

    /// The hardware source failed to produce entropy within the
    /// retry budget. Transient — the caller MAY retry after a
    /// backoff. Per Intel's DRNG Software Implementation Guide,
    /// RDRAND is retried [`RDRAND_RETRIES`] times and RDSEED
    /// [`RDSEED_RETRIES`] times before this variant is emitted.
    #[error("hardware entropy exhausted after {retries} retries")]
    Exhausted {
        /// Number of retries attempted before giving up.
        retries: u32,
    },
}

/// A cryptographically-secure random-number source.
///
/// Implementations MUST fill `output` completely with unbiased
/// random bytes, or return an error. Partial fills are forbidden — a
/// short-write is a use-after-error footgun for cryptographic
/// callers (a "silently truncated" nonce is the classic case).
pub trait SecureRandom {
    /// Fill `output` with cryptographically-secure random bytes.
    ///
    /// On success, every byte in `output` has been overwritten with
    /// fresh entropy. On error, the state of `output` is undefined —
    /// callers MUST NOT use it.
    fn fill(&self, output: &mut [u8]) -> Result<(), RngError>;
}
