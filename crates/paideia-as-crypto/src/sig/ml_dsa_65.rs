//! ML-DSA-65 signature — FIPS 204 (Module-Lattice-based Digital
//! Signature Standard), NIST security-category 3 parameter set.
//!
//! Wave υ (paideia-as υ-01 / υ-02) — `no_std + alloc` sibling of the
//! std-linked signer surface already provided by
//! `paideia-pq-sign::mldsa`. The offline packaging tools consume the
//! `pq-sign` surface (std, `thiserror`, rich error types); the kernel
//! / satellite `.pdx` runtime consumes THIS surface through the FFI
//! thunks in [`crate::ffi::ml_dsa_65`].
//!
//! # Reference
//!
//! [FIPS 204] *Module-Lattice-Based Digital Signature Standard*, NIST,
//! August 2024. Parameter set ML-DSA-65: `(k, l) = (6, 5)`, `d = 13`,
//! `η = 4`, `τ = 49`, `β = 196`, `γ₁ = 2^19`, `γ₂ = (q-1)/32`, `ω = 55`.
//! Byte sizes:
//!
//! | Byte string                     | Length (bytes) |
//! |---------------------------------|----------------|
//! | KeyGen seed `ξ`                 | 32             |
//! | Verifying key `pk`              | 1952           |
//! | Signing key seed (compact form) | 32             |
//! | Signature `σ`                   | 3309           |
//!
//! `MLDSA65_SK_LEN` is the compact-form seed (32 bytes), NOT the
//! 4032-byte expanded signing key. That matches the shape used by
//! `paideia-pq-sign::mldsa::SecretKey(Vec<u8>)`, whose `SecretKey.0`
//! is the 32-byte seed. Consumers that hold an expanded key on disk
//! must derive it back from the seed via `SigningKey::from_seed` at
//! call time; the compact form is what `.pdxtrust` / `.pdxpkg` blob
//! headers embed.
//!
//! [FIPS 204]: https://csrc.nist.gov/pubs/fips/204/final
//!
//! # Backend
//!
//! Thin wrappers over the RustCrypto [`ml-dsa`] crate. Same pattern
//! as [`crate::kem::MlKem768`] over `ml-kem`. The `alloc` feature is
//! required for `expanded_key()` and `Signature::to_bytes()` — both
//! allocate a fixed-size heap buffer whose length is a compile-time
//! constant of the parameter set. `default-features = false` on the
//! `ml-dsa` dependency keeps std out of the graph.
//!
//! [`ml-dsa`]: https://docs.rs/ml-dsa
//!
//! # Determinism
//!
//! FIPS 204 §7.2 supports two sign variants: hedged (`rnd` is fresh
//! entropy) and deterministic (`rnd = 0`). This wrapper uses the
//! deterministic variant unconditionally so signature output is
//! byte-reproducible given the same `(sk_seed, msg)`. That matches
//! `paideia-pq-sign::mldsa::MlDsa65Marker::sign`, whose comment
//! explicitly wires `let rnd = [0u8; 32]`. Consumers that need
//! hedged sign land on `paideia-pq-sign` instead.
//!
//! # Verify contract
//!
//! `MlDsa65::verify(msg, sig, pk)` returns `true` iff the signature
//! authenticates `msg` under `pk`. All decode failures (malformed
//! `pk`, wrong-length `sig`) surface as `Err(SigError::InvalidInput)`,
//! not as `Ok(false)` — the FFI thunk in [`crate::ffi::ml_dsa_65`]
//! translates both `Err` and `Ok(false)` to the `mldsa65_verify` u32
//! return `1 = FAIL`, per the task spec.

use alloc::vec::Vec;

use ml_dsa::{MlDsa65 as MlDsa65Core, Signature as MlDsaSig, SigningKey, VerifyingKey};
use ml_dsa::SignatureEncoding as _;

/// KeyGen seed length in bytes (ξ) — 32.
pub const MLDSA65_SEED_LEN: usize = 32;

/// Encoded verifying-key length in bytes — 1952.
pub const MLDSA65_PK_LEN: usize = 1952;

/// Encoded signature length in bytes — 3309.
pub const MLDSA65_SIG_LEN: usize = 3309;

/// Error surface for the trait-level sign / verify calls.
///
/// The FFI thunks translate every variant to their own compact
/// return-code contract (`mldsa65_verify → u32`, `mldsa65_sign →
/// u64`); this enum is what the Rust-level trait exposes so a future
/// non-FFI Rust consumer inside the workspace (unit tests, an offline
/// signer that prefers this crate over `paideia-pq-sign`) can branch
/// on the failure mode.
#[derive(Debug)]
pub enum SigError {
    /// A required input pointer was NULL or a length did not match the
    /// spec (`pk != 1952`, `sig != 3309`).
    InvalidInput,
    /// The underlying `ml-dsa` primitive rejected the inputs
    /// internally (unreachable on well-formed fixed-length inputs on
    /// the current backend; kept as an escape hatch for future
    /// paideia-native impls that may fail more granularly).
    Primitive,
}

/// ML-DSA-65 marker type. Same shape as [`crate::kem::MlKem768`].
pub struct MlDsa65;

impl MlDsa65 {
    /// Verify an ML-DSA-65 signature.
    ///
    /// `pk` must be exactly [`MLDSA65_PK_LEN`] bytes and `sig`
    /// exactly [`MLDSA65_SIG_LEN`] bytes. Any other length is an
    /// `Err(SigError::InvalidInput)`. A decode failure inside the
    /// underlying `ml-dsa` crate maps to `Ok(false)` — a semantically
    /// malformed signature that decodes to the right length still
    /// fails verification cleanly.
    pub fn verify(msg: &[u8], sig: &[u8], pk: &[u8]) -> Result<bool, SigError> {
        let pk_arr = <[u8; MLDSA65_PK_LEN]>::try_from(pk)
            .map_err(|_| SigError::InvalidInput)?;
        let sig_arr = <[u8; MLDSA65_SIG_LEN]>::try_from(sig)
            .map_err(|_| SigError::InvalidInput)?;

        let vk = VerifyingKey::<MlDsa65Core>::decode(&pk_arr.into());

        let ml_dsa_sig = match MlDsaSig::<MlDsa65Core>::try_from(sig_arr.as_ref()) {
            Ok(s) => s,
            // Malformed signature encoding — not an input shape error
            // (length was right) so surface as a clean "not valid".
            Err(_) => return Ok(false),
        };

        Ok(vk.verify_internal(msg, &ml_dsa_sig))
    }

    /// Deterministic ML-DSA-65 sign.
    ///
    /// `sk` is the compact 32-byte seed form; the wrapper re-derives
    /// the expanded signing key at each call via
    /// [`SigningKey::from_seed`]. This matches
    /// `paideia-pq-sign::mldsa::MlDsa65Marker::sign` — callers hold the
    /// seed, not the expanded key.
    ///
    /// Returns the encoded signature as a heap `Vec<u8>` of length
    /// [`MLDSA65_SIG_LEN`]. The FFI thunk copies these bytes into a
    /// caller-owned output buffer and returns the count.
    pub fn sign(msg: &[u8], sk: &[u8]) -> Result<Vec<u8>, SigError> {
        let seed_arr = <[u8; MLDSA65_SEED_LEN]>::try_from(sk)
            .map_err(|_| SigError::InvalidInput)?;

        let signing_key = SigningKey::<MlDsa65Core>::from_seed(&seed_arr.into());

        // Deterministic variant — matches the paideia-pq-sign wrapper.
        // FIPS 204 §7.2 permits either fresh entropy or all-zero rnd;
        // the paideia-as-crypto surface commits to zero for
        // reproducibility.
        let rnd = [0u8; 32];
        let sig = signing_key
            .expanded_key()
            .sign_internal(&[msg], (&rnd).into());

        Ok(sig.to_bytes().to_vec())
    }
}

#[cfg(test)]
mod tests {
    //! Round-trip sign→verify tests. NIST ACVP known-answer vectors
    //! for ML-DSA-65 live on the `paideia-pq-sign::mldsa::kat` surface
    //! (behind the `kat` cargo feature) — reproducing them here would
    //! duplicate the corpus without adding coverage. The round-trip
    //! tests below pin the wrapper's behavior on random-shape inputs.

    use super::*;

    // A representative seed value for the round-trip tests. Its bit
    // pattern is arbitrary — the underlying ml-dsa keygen expands any
    // 32-byte seed into a valid keypair.
    const TEST_SEED: [u8; MLDSA65_SEED_LEN] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
        0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
        0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
        0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
    ];

    /// Derive the encoded verifying key from the test seed via the
    /// same `SigningKey::from_seed → as_ref::<VerifyingKey>` path
    /// paideia-pq-sign::mldsa uses.
    fn test_pk() -> Vec<u8> {
        let sk = SigningKey::<MlDsa65Core>::from_seed(&TEST_SEED.into());
        let vk: &VerifyingKey<MlDsa65Core> = sk.as_ref();
        vk.encode().to_vec()
    }

    #[test]
    fn round_trip_sign_verify_succeeds() {
        let msg = b"paideia-as wave upsilon round-trip";
        let sig = MlDsa65::sign(msg, &TEST_SEED).expect("sign");
        assert_eq!(sig.len(), MLDSA65_SIG_LEN);

        let pk = test_pk();
        assert_eq!(pk.len(), MLDSA65_PK_LEN);

        let ok = MlDsa65::verify(msg, &sig, &pk).expect("verify");
        assert!(ok, "genuine signature should verify");
    }

    #[test]
    fn verify_rejects_flipped_signature() {
        let msg = b"paideia-as wave upsilon flip";
        let mut sig = MlDsa65::sign(msg, &TEST_SEED).expect("sign");
        sig[0] ^= 0x01;

        let pk = test_pk();
        let ok = MlDsa65::verify(msg, &sig, &pk).expect("verify");
        assert!(!ok, "tampered signature must not verify");
    }

    #[test]
    fn verify_rejects_wrong_message() {
        let sig = MlDsa65::sign(b"original", &TEST_SEED).expect("sign");
        let pk = test_pk();
        let ok = MlDsa65::verify(b"different", &sig, &pk).expect("verify");
        assert!(!ok, "signature over different message must not verify");
    }

    #[test]
    fn verify_rejects_wrong_pk_length() {
        let sig = MlDsa65::sign(b"msg", &TEST_SEED).expect("sign");
        let short_pk = [0u8; MLDSA65_PK_LEN - 1];
        assert!(matches!(
            MlDsa65::verify(b"msg", &sig, &short_pk),
            Err(SigError::InvalidInput),
        ));
    }

    #[test]
    fn verify_rejects_wrong_sig_length() {
        let pk = test_pk();
        let short_sig = [0u8; MLDSA65_SIG_LEN - 1];
        assert!(matches!(
            MlDsa65::verify(b"msg", &short_sig, &pk),
            Err(SigError::InvalidInput),
        ));
    }

    #[test]
    fn sign_rejects_wrong_seed_length() {
        let short_seed = [0u8; MLDSA65_SEED_LEN - 1];
        assert!(matches!(
            MlDsa65::sign(b"msg", &short_seed),
            Err(SigError::InvalidInput),
        ));
    }

    #[test]
    fn sign_is_deterministic() {
        let msg = b"paideia-as deterministic sign";
        let a = MlDsa65::sign(msg, &TEST_SEED).expect("sign a");
        let b = MlDsa65::sign(msg, &TEST_SEED).expect("sign b");
        assert_eq!(a, b, "deterministic sign must produce identical output");
    }
}
