//! PAX-specific signer/verifier helpers.
//!
//! High-level wrappers for signing and verifying 32-byte PAX content hashes
//! using the hybrid scheme (Ed25519 + ML-DSA-65).

use crate::Signer;
use crate::hybrid::{Hybrid, HybridPublicKey, HybridSecretKey, HybridSignature};

/// Sign a 32-byte PAX content hash with a hybrid keypair.
pub fn sign_pax_hash(sk: &HybridSecretKey, content_hash: &[u8; 32]) -> HybridSignature {
    Hybrid::sign(sk, content_hash.as_ref())
}

/// Verify a hybrid signature against a 32-byte PAX content hash.
pub fn verify_pax_hash(
    pk: &HybridPublicKey,
    content_hash: &[u8; 32],
    sig: &HybridSignature,
) -> bool {
    Hybrid::verify(pk, content_hash.as_ref(), sig)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn sign_verify_roundtrip_on_pax_hash() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let content_hash: [u8; 32] = [0x42u8; 32];

        let sig = sign_pax_hash(&sk, &content_hash);
        assert!(
            verify_pax_hash(&pk, &content_hash, &sig),
            "Signature over PAX hash should verify"
        );
    }

    #[test]
    fn verify_fails_on_tampered_content_hash() {
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let hash_a: [u8; 32] = [0x42u8; 32];
        let hash_b: [u8; 32] = [0x99u8; 32];

        let sig = sign_pax_hash(&sk, &hash_a);
        assert!(
            !verify_pax_hash(&pk, &hash_b, &sig),
            "Signature over different hash should not verify"
        );
    }

    #[test]
    fn end_to_end_sign_and_verify_via_pax_emitter() {
        use paideia_as_emitter_pax::{
            Architecture, PaxHeader, SectionTable, embed_signature_hash,
            header_signature_hash_matches, pax_message_to_sign,
        };

        // Build a minimal PAX
        let mut header = PaxHeader::new(Architecture::X86_64);
        let table = SectionTable::new();
        let contents: [&[u8]; 0] = [];

        // Compute the canonical content hash
        let content_hash = pax_message_to_sign(&header, &table, &contents);

        // Generate keypair and sign the hash
        let (pk, sk) = Hybrid::keygen(&mut OsRng);
        let sig = sign_pax_hash(&sk, &content_hash);
        let sig_bytes = sig.to_bytes();

        // Embed the signature hash into the header
        embed_signature_hash(&mut header, &sig_bytes);

        // Verify side: recompute content hash, lookup signature bytes, verify
        let recomputed_hash = pax_message_to_sign(&header, &table, &contents);
        assert_eq!(
            recomputed_hash, content_hash,
            "Recomputed hash should match original"
        );

        // Verify the signature
        assert!(
            verify_pax_hash(&pk, &recomputed_hash, &sig),
            "Signature should verify against recomputed hash"
        );

        // Verify that the header slot matches the signature bytes
        assert!(
            header_signature_hash_matches(&header, &sig_bytes),
            "Header signature hash slot should match signature bytes"
        );
    }
}
