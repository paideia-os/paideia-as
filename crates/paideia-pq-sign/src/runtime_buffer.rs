//! Runtime buffer signing — PQ signature discipline for JIT / dynamic-emit workflows.
//!
//! This module provides thin wrappers around the hybrid signing scheme
//! (Ed25519 + ML-DSA-65) tailored to raw bytecode buffers produced by
//! `paideia-as-emit` and similar runtime-code generators.
//!
//! # Composition
//!
//! The typical flow is:
//!
//! ```text
//! build IR → resolve_symbols → emit_instruction → buf.bytes: Vec<u8>
//!                                                      ↓
//!                                       sign_runtime_buffer(&sk, &buf)
//!                                                      ↓
//!                                       HybridSignature (3373 B)
//! ```
//!
//! The verifier mirrors this:
//!
//! ```text
//! received_bytes + received_sig ↓
//!                verify_runtime_buffer(&pk, &bytes, &sig) → bool
//! ```
//!
//! # Determinism
//!
//! Signature is deterministic for a given `(sk, buf)` pair (backed by the
//! deterministic-rnd discipline in `mldsa::sign`). Reproducible signing
//! is what makes byte-identical-emit tractable across hosts.
//!
//! # Stability
//!
//! Part of the paideia-as v0.20 stable public API on the pq-sign side.
//! Signature changes require a major-version bump.

use crate::hybrid::{Hybrid, HybridPublicKey, HybridSecretKey, HybridSignature};
use crate::Signer;

/// Sign a runtime-emitted byte buffer with a hybrid keypair.
///
/// This is the signing seam for JIT / dynamic-emit workflows where the
/// buffer is produced by `paideia-as-emit::emit_instruction` and similar
/// runtime code generators.
///
/// # Deterministic
///
/// Signature is deterministic for a given `(sk, buf)` pair (backed by the
/// deterministic-rnd discipline in `mldsa::sign` — see `pq-trust-root.md §0`).
/// Reproducible signing is what makes byte-identical-emit tractable across
/// hosts.
///
/// # Stability
///
/// Part of the paideia-as v0.20 stable public API on the pq-sign side.
/// Signature changes require a major-version bump.
///
/// # See also
///
/// - [`verify_runtime_buffer`] — mirror on the verify side.
/// - [`runtime_buffer_digest`] — the intermediate BLAKE3 hash, exposed so
///   callers can persist / index by digest without re-signing.
/// - [`crate::sign_pax_hash`] — for **PAX artifacts** where the canonical
///   content hash was already computed by the emitter pipeline. Use that,
///   not `sign_runtime_buffer`, whenever a PAX header exists.
pub fn sign_runtime_buffer(
    sk: &HybridSecretKey,
    buf: &[u8],
) -> HybridSignature {
    let digest = runtime_buffer_digest(buf);
    Hybrid::sign(sk, &digest)
}

/// Verify a hybrid signature over a runtime-emitted byte buffer.
///
/// AND-verify semantics (both Ed25519 and ML-DSA-65 halves must verify).
/// Returns `false` on any tamper: buffer bytes changed, signature bytes
/// changed, wrong public key, or half-verify failure.
///
/// # See also
///
/// - [`sign_runtime_buffer`] — the signing dual.
pub fn verify_runtime_buffer(
    pk: &HybridPublicKey,
    buf: &[u8],
    sig: &HybridSignature,
) -> bool {
    let digest = runtime_buffer_digest(buf);
    Hybrid::verify(pk, &digest, sig)
}

/// Compute the BLAKE3 digest of a runtime buffer without signing it.
///
/// Exposed so callers who need to persist the digest separately (loader
/// manifest, JIT cache key, PAX header slot) can compute it once instead
/// of BLAKE3-hashing the buffer twice.
///
/// # Returns
///
/// A 32-byte BLAKE3 digest of the input buffer.
pub fn runtime_buffer_digest(buf: &[u8]) -> [u8; 32] {
    blake3::hash(buf).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    /// T-1: Sign and verify a small buffer (16 NOPs).
    #[test]
    fn sign_verify_roundtrip_small_buffer() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let buf = [0x90u8; 16]; // 16 NOPs

        let sig = sign_runtime_buffer(&sk, &buf);
        assert!(
            verify_runtime_buffer(&pk, &buf[..], &sig),
            "Signature over small buffer should verify"
        );
    }

    /// T-2: Sign and verify an empty buffer.
    #[test]
    fn sign_verify_roundtrip_empty_buffer() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let buf: [u8; 0] = [];

        let sig = sign_runtime_buffer(&sk, &buf);
        assert!(
            verify_runtime_buffer(&pk, &buf[..], &sig),
            "Signature over empty buffer should verify"
        );
    }

    /// T-3: Sign and verify a large buffer (64 KiB).
    ///
    /// Guards the streaming-BLAKE3 path against pathological large-input
    /// regressions.
    #[test]
    fn sign_verify_roundtrip_large_buffer() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let buf = vec![0xCCu8; 65_536]; // 64 KiB of INT3

        let sig = sign_runtime_buffer(&sk, &buf);
        assert!(
            verify_runtime_buffer(&pk, &buf, &sig),
            "Signature over large buffer should verify"
        );
    }

    /// T-4: Verify rejects a tampered buffer (1-bit flip).
    #[test]
    fn tampered_buffer_rejected() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let mut buf = [0x90u8; 16]; // 16 NOPs

        let sig = sign_runtime_buffer(&sk, &buf);

        // Tamper with the buffer
        buf[7] ^= 0x01;

        assert!(
            !verify_runtime_buffer(&pk, &buf[..], &sig),
            "Signature should not verify over tampered buffer"
        );
    }

    /// T-5: Verify rejects a tampered signature.
    #[test]
    fn tampered_signature_rejected() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let buf = [0x90u8; 16]; // 16 NOPs

        let mut sig = sign_runtime_buffer(&sk, &buf);

        // Tamper with the signature (flip first byte of ML-DSA half)
        sig.mldsa.0[0] ^= 0xFF;

        assert!(
            !verify_runtime_buffer(&pk, &buf[..], &sig),
            "Signature should not verify when tampered"
        );
    }

    /// T-6: Confirm deterministic signing reproduces the same signature.
    ///
    /// The deterministic-rnd discipline in `mldsa::sign` ensures that
    /// signing the same `(sk, buf)` twice produces identical signatures.
    #[test]
    fn deterministic_signing_reproducible() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let buf = [0x90u8; 16]; // 16 NOPs

        let sig1 = sign_runtime_buffer(&sk, &buf);
        let sig2 = sign_runtime_buffer(&sk, &buf);

        assert_eq!(
            sig1.to_bytes(),
            sig2.to_bytes(),
            "Signatures over the same buffer should be identical (deterministic rnd)"
        );

        // Verify both
        assert!(
            verify_runtime_buffer(&pk, &buf[..], &sig1),
            "First signature should verify"
        );
        assert!(
            verify_runtime_buffer(&pk, &buf[..], &sig2),
            "Second signature should verify"
        );
    }
}
