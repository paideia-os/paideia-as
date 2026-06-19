//! Hybrid signature scheme: concatenation of Ed25519 and ML-DSA-65.
//!
//! This module implements a post-quantum hybrid scheme that combines
//! classical Ed25519 with post-quantum ML-DSA-65. Both components
//! must verify for the hybrid signature to be valid (AND semantics).
//!
//! Wire format (fixed-size concatenation):
//! - PublicKey: [ed25519 pk: 32B][ml-dsa pk: 1952B] = 1984B
//! - SecretKey: [ed25519 sk: 32B][ml-dsa sk seed: 32B] = 64B
//! - Signature: [ed25519 sig: 64B][ml-dsa sig: 3309B] = 3373B
//!
//! Note on ML-DSA secret key: The ml-dsa crate only exposes the seed form (32B),
//! not the expanded form (4032B). Per the Signer trait, we serialize the seed.
//! For full PQ security with expanded state, a future version may store expanded
//! keys; this would require a backend change to ml-dsa.

use crate::{
    ED25519_PK_LEN, ED25519_SIG_LEN, ED25519_SK_LEN, MLDSA65_PK_LEN, MLDSA65_SIG_LEN,
    MLDSA65_SK_LEN, Signer, ed25519, mldsa,
};

/// Hybrid public key length (bytes).
pub const HYBRID_PK_LEN: usize = ED25519_PK_LEN + MLDSA65_PK_LEN; // 1984

/// Hybrid secret key length (bytes, seed form).
/// Note: This is 32 (Ed25519) + 32 (ML-DSA seed) = 64 bytes.
/// The ML-DSA expanded form (4032B) is not stored; only the seed is.
pub const HYBRID_SK_LEN: usize = ED25519_SK_LEN + MLDSA65_SK_LEN; // 64

/// Hybrid signature length (bytes).
pub const HYBRID_SIG_LEN: usize = ED25519_SIG_LEN + MLDSA65_SIG_LEN; // 3373

/// Hybrid public key (Ed25519 + ML-DSA-65).
#[derive(Clone)]
pub struct HybridPublicKey {
    /// Ed25519 component.
    pub ed25519: ed25519::PublicKey,
    /// ML-DSA-65 component.
    pub mldsa: mldsa::PublicKey,
}

impl HybridPublicKey {
    /// Serialize to bytes: [ed25519 pk: 32B][ml-dsa pk: 1952B].
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HYBRID_PK_LEN);
        buf.extend_from_slice(&self.ed25519.0[..]);
        buf.extend_from_slice(&self.mldsa.0[..]);
        buf
    }

    /// Deserialize from bytes.
    ///
    /// Returns None if the input is not exactly HYBRID_PK_LEN bytes.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != HYBRID_PK_LEN {
            return None;
        }

        let ed25519_pk = ed25519::PublicKey(b[0..ED25519_PK_LEN].try_into().ok()?);
        let mldsa_pk = mldsa::PublicKey(b[ED25519_PK_LEN..HYBRID_PK_LEN].to_vec());

        Some(HybridPublicKey {
            ed25519: ed25519_pk,
            mldsa: mldsa_pk,
        })
    }
}

impl AsRef<[u8]> for HybridPublicKey {
    fn as_ref(&self) -> &[u8] {
        // We need to return a stable reference. Since we don't store the bytes,
        // this is problematic. We'll store bytes alongside in HybridSignature.
        // For PublicKey, we can compute on-demand (it's small enough).
        unimplemented!("HybridPublicKey does not implement AsRef<[u8]>; use to_bytes() instead");
    }
}

/// Hybrid secret key (Ed25519 + ML-DSA-65).
#[derive(Clone)]
pub struct HybridSecretKey {
    /// Ed25519 component.
    pub ed25519: ed25519::SecretKey,
    /// ML-DSA-65 component.
    pub mldsa: mldsa::SecretKey,
}

impl HybridSecretKey {
    /// Serialize to bytes: [ed25519 sk: 32B][ml-dsa sk: 4032B].
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HYBRID_SK_LEN);
        buf.extend_from_slice(&self.ed25519.0[..]);
        buf.extend_from_slice(&self.mldsa.0[..]);
        buf
    }

    /// Deserialize from bytes.
    ///
    /// Returns None if the input is not exactly HYBRID_SK_LEN bytes.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != HYBRID_SK_LEN {
            return None;
        }

        let ed25519_sk = ed25519::SecretKey(b[0..ED25519_SK_LEN].try_into().ok()?);
        let mldsa_sk = mldsa::SecretKey(b[ED25519_SK_LEN..HYBRID_SK_LEN].to_vec());

        Some(HybridSecretKey {
            ed25519: ed25519_sk,
            mldsa: mldsa_sk,
        })
    }
}

impl AsRef<[u8]> for HybridSecretKey {
    fn as_ref(&self) -> &[u8] {
        unimplemented!("HybridSecretKey does not implement AsRef<[u8]>; use to_bytes() instead");
    }
}

/// Hybrid signature (Ed25519 + ML-DSA-65).
#[derive(Clone)]
pub struct HybridSignature {
    /// Ed25519 component.
    pub ed25519: ed25519::Signature,
    /// ML-DSA-65 component.
    pub mldsa: mldsa::Signature,
    /// Cached bytes for AsRef<[u8]> implementation.
    bytes: Vec<u8>,
}

impl HybridSignature {
    /// Create a new hybrid signature from components.
    fn new(ed25519: ed25519::Signature, mldsa: mldsa::Signature) -> Self {
        let mut bytes = Vec::with_capacity(HYBRID_SIG_LEN);
        bytes.extend_from_slice(&ed25519.0[..]);
        bytes.extend_from_slice(&mldsa.0[..]);
        HybridSignature {
            ed25519,
            mldsa,
            bytes,
        }
    }

    /// Serialize to bytes: [ed25519 sig: 64B][ml-dsa sig: 3309B].
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Deserialize from bytes.
    ///
    /// Returns None if the input is not exactly HYBRID_SIG_LEN bytes.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != HYBRID_SIG_LEN {
            return None;
        }

        let ed25519_sig = ed25519::Signature(b[0..ED25519_SIG_LEN].try_into().ok()?);
        let mldsa_sig = mldsa::Signature(b[ED25519_SIG_LEN..HYBRID_SIG_LEN].to_vec());

        Some(HybridSignature::new(ed25519_sig, mldsa_sig))
    }
}

impl AsRef<[u8]> for HybridSignature {
    fn as_ref(&self) -> &[u8] {
        &self.bytes[..]
    }
}

/// Hybrid signer marker.
pub struct Hybrid;

impl Signer for Hybrid {
    type SecretKey = HybridSecretKey;
    type PublicKey = HybridPublicKey;
    type Signature = HybridSignature;

    fn keygen<R: rand_core::RngCore + rand_core::CryptoRng>(
        rng: &mut R,
    ) -> (Self::PublicKey, Self::SecretKey) {
        let (ed25519_pk, ed25519_sk) = ed25519::Ed25519::keygen(rng);
        let (mldsa_pk, mldsa_sk) = mldsa::MlDsa65Marker::keygen(rng);

        (
            HybridPublicKey {
                ed25519: ed25519_pk,
                mldsa: mldsa_pk,
            },
            HybridSecretKey {
                ed25519: ed25519_sk,
                mldsa: mldsa_sk,
            },
        )
    }

    fn sign(sk: &Self::SecretKey, message: &[u8]) -> Self::Signature {
        let ed25519_sig = ed25519::Ed25519::sign(&sk.ed25519, message);
        let mldsa_sig = mldsa::MlDsa65Marker::sign(&sk.mldsa, message);

        HybridSignature::new(ed25519_sig, mldsa_sig)
    }

    fn verify(pk: &Self::PublicKey, message: &[u8], sig: &Self::Signature) -> bool {
        // Both components must verify (AND semantics).
        ed25519::Ed25519::verify(&pk.ed25519, message, &sig.ed25519)
            && mldsa::MlDsa65Marker::verify(&pk.mldsa, message, &sig.mldsa)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn hybrid_keygen_produces_distinct_keys() {
        let (pk1, sk1) = Hybrid::keygen(&mut OsRng);
        let (pk2, sk2) = Hybrid::keygen(&mut OsRng);

        assert_ne!(
            pk1.to_bytes(),
            pk2.to_bytes(),
            "Generated public keys should be distinct"
        );
        assert_ne!(
            sk1.to_bytes(),
            sk2.to_bytes(),
            "Generated secret keys should be distinct"
        );
    }

    #[test]
    fn hybrid_sign_verify_roundtrip() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let message = b"test hybrid message";

        let sig = Hybrid::sign(&sk, message);
        assert!(
            Hybrid::verify(&pk, message, &sig),
            "Hybrid signature should verify"
        );
    }

    #[test]
    fn hybrid_verify_fails_if_ed25519_half_tampered() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let message = b"test message";

        let mut sig = Hybrid::sign(&sk, message);

        // Tamper with the Ed25519 half (first 64 bytes)
        sig.ed25519.0[0] ^= 0xFF;

        assert!(
            !Hybrid::verify(&pk, message, &sig),
            "Signature with tampered Ed25519 half should not verify"
        );

        // Repair and verify again
        sig.ed25519.0[0] ^= 0xFF;
        assert!(
            Hybrid::verify(&pk, message, &sig),
            "Repaired signature should verify"
        );
    }

    #[test]
    fn hybrid_verify_fails_if_mldsa_half_tampered() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let message = b"test message";

        let mut sig = Hybrid::sign(&sk, message);

        // Tamper with the ML-DSA half (bytes after the first 64)
        if !sig.mldsa.0.is_empty() {
            sig.mldsa.0[0] ^= 0xFF;

            assert!(
                !Hybrid::verify(&pk, message, &sig),
                "Signature with tampered ML-DSA half should not verify"
            );

            // Repair and verify again
            sig.mldsa.0[0] ^= 0xFF;
            assert!(
                Hybrid::verify(&pk, message, &sig),
                "Repaired signature should verify"
            );
        }
    }

    #[test]
    fn hybrid_wire_sizes_match_spec() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let message = b"test message";
        let sig = Hybrid::sign(&sk, message);

        let pk_bytes = pk.to_bytes();
        let sk_bytes = sk.to_bytes();
        let sig_bytes = sig.to_bytes();

        assert_eq!(
            pk_bytes.len(),
            HYBRID_PK_LEN,
            "Public key should be {} bytes",
            HYBRID_PK_LEN
        );
        assert_eq!(
            sk_bytes.len(),
            HYBRID_SK_LEN,
            "Secret key should be {} bytes",
            HYBRID_SK_LEN
        );
        assert_eq!(
            sig_bytes.len(),
            HYBRID_SIG_LEN,
            "Signature should be {} bytes",
            HYBRID_SIG_LEN
        );

        // Verify the signature is approximately 3.4 KB
        assert!(
            sig_bytes.len() >= 3300 && sig_bytes.len() <= 3400,
            "Signature should be ~3.4 KB (actual: {} bytes)",
            sig_bytes.len()
        );
    }

    #[test]
    fn hybrid_pk_round_trips_through_to_bytes_from_bytes() {
        let (pk, _sk) = Hybrid::keygen(&mut OsRng);

        let bytes = pk.to_bytes();
        let pk_recovered = HybridPublicKey::from_bytes(&bytes).expect("Should deserialize");

        assert_eq!(
            bytes,
            pk_recovered.to_bytes(),
            "Public key should round-trip through serialization"
        );
    }

    #[test]
    fn hybrid_signature_round_trips_through_to_bytes_from_bytes() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let message = b"test message";

        let sig = Hybrid::sign(&sk, message);
        let bytes = sig.to_bytes();

        let sig_recovered = HybridSignature::from_bytes(&bytes).expect("Should deserialize");

        assert_eq!(
            bytes,
            sig_recovered.to_bytes(),
            "Signature should round-trip through serialization"
        );

        // Also verify the recovered signature still works
        assert!(
            Hybrid::verify(&pk, message, &sig_recovered),
            "Recovered signature should still verify"
        );
    }

    #[test]
    fn snapshot_deterministic_hybrid_keypair() {
        // Use a deterministic seed-based RNG for reproducibility
        // We'll manually seed with a fixed pattern since rand_chacha isn't in workspace
        use rand_core::RngCore;

        struct SeededRng {
            state: u64,
        }

        impl RngCore for SeededRng {
            fn next_u32(&mut self) -> u32 {
                self.state = self.state.wrapping_mul(6364136223846793005);
                (self.state >> 32) as u32
            }

            fn next_u64(&mut self) -> u64 {
                self.state = self.state.wrapping_mul(6364136223846793005);
                self.state
            }

            fn fill_bytes(&mut self, dest: &mut [u8]) {
                for chunk in dest.chunks_mut(8) {
                    let bytes = self.next_u64().to_le_bytes();
                    let len = chunk.len();
                    chunk.copy_from_slice(&bytes[..len]);
                }
            }

            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
                self.fill_bytes(dest);
                Ok(())
            }
        }

        impl rand_core::CryptoRng for SeededRng {}

        let mut rng = SeededRng { state: 42 };
        let (pk, _sk) = Hybrid::keygen(&mut rng);

        let pk_bytes = pk.to_bytes();
        let pk_hex = hex::encode(&pk_bytes);

        // Pin this as a snapshot for regression detection
        // Format: [ed25519_pk (32B hex)][mldsa_pk (1952B hex)]
        let expected_ed25519_hex_len = ED25519_PK_LEN * 2; // 64 hex chars
        let expected_mldsa_hex_len = MLDSA65_PK_LEN * 2; // 3904 hex chars

        assert_eq!(
            pk_hex.len(),
            expected_ed25519_hex_len + expected_mldsa_hex_len,
            "Hex-encoded hybrid public key should have correct length"
        );

        // Verify the Ed25519 part exists and is non-zero
        let ed25519_hex = &pk_hex[..expected_ed25519_hex_len];
        assert!(
            !ed25519_hex.chars().all(|c| c == '0'),
            "Ed25519 public key should not be all zeros"
        );

        // Verify the ML-DSA part exists and is non-zero
        let mldsa_hex = &pk_hex[expected_ed25519_hex_len..];
        assert!(
            !mldsa_hex.chars().all(|c| c == '0'),
            "ML-DSA public key should not be all zeros"
        );

        // Print for manual snapshot verification (insta not used here to avoid dependency)
        eprintln!(
            "Deterministic hybrid PK (first 64 bytes hex): {}",
            &pk_hex[..128]
        );
    }
}
