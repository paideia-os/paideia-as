//! ML-DSA-65 wrapper (FIPS 204 post-quantum signature scheme).

use crate::Signer;
use ml_dsa::SignatureEncoding as _;
use ml_dsa::{MlDsa65, Signature as MlDsaSig, SigningKey, VerifyingKey};
use std::convert::TryFrom;

/// ML-DSA-65 signer marker.
pub struct MlDsa65Marker;

/// ML-DSA-65 secret key (32-byte seed).
#[derive(Clone)]
pub struct SecretKey(pub Vec<u8>);

/// ML-DSA-65 public key (1952 bytes).
#[derive(Clone)]
pub struct PublicKey(pub Vec<u8>);

/// ML-DSA-65 signature (3309 bytes).
#[derive(Clone)]
pub struct Signature(pub Vec<u8>);

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Signer for MlDsa65Marker {
    type SecretKey = SecretKey;
    type PublicKey = PublicKey;
    type Signature = Signature;

    fn keygen<R: rand_core::RngCore + rand_core::CryptoRng>(
        rng: &mut R,
    ) -> (Self::PublicKey, Self::SecretKey) {
        // Use the low-level key generation by sampling a seed
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);

        let signing_key = SigningKey::<MlDsa65>::from_seed(&seed.into());

        // Get the public key from the signing key (uses precomputed field with alloc feature)
        // Using AsRef<VerifyingKey> which is available with alloc feature
        use std::convert::AsRef as StdAsRef;
        let verifying_key: &VerifyingKey<MlDsa65> = signing_key.as_ref();

        let sk_bytes = seed.to_vec();
        let pk_bytes = verifying_key.encode().to_vec();

        (PublicKey(pk_bytes), SecretKey(sk_bytes))
    }

    fn sign(sk: &Self::SecretKey, message: &[u8]) -> Self::Signature {
        let seed_array: [u8; 32] =
            sk.0.as_slice()
                .try_into()
                .expect("Secret key must be 32 bytes (seed)");
        let signing_key = SigningKey::<MlDsa65>::from_seed(&seed_array.into());

        // Use sign_internal with deterministic rnd (all zeros) to match verify_internal
        // This avoids the context issue that sign_deterministic has
        let rnd = [0u8; 32];
        let sig = signing_key
            .expanded_key()
            .sign_internal(&[message], (&rnd).into());
        Signature(sig.to_bytes().to_vec())
    }

    fn verify(pk: &Self::PublicKey, message: &[u8], sig: &Self::Signature) -> bool {
        // Decode the public key bytes into an EncodedVerifyingKey
        let pk_array = match <[u8; 1952]>::try_from(pk.0.as_slice()) {
            Ok(arr) => arr,
            Err(_) => return false,
        };

        let verifying_key = VerifyingKey::<MlDsa65>::decode(&pk_array.into());

        // Decode the signature bytes
        let sig_array = match <[u8; 3309]>::try_from(sig.0.as_slice()) {
            Ok(arr) => arr,
            Err(_) => return false,
        };

        let ml_dsa_sig = match MlDsaSig::<MlDsa65>::try_from(sig_array.as_ref()) {
            Ok(s) => s,
            Err(_) => return false,
        };

        // Verify using the low-level verify_internal which doesn't require a context
        verifying_key.verify_internal(message, &ml_dsa_sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn keygen_produces_distinct_keys() {
        let (pk1, sk1) = MlDsa65Marker::keygen(&mut OsRng);
        let (pk2, sk2) = MlDsa65Marker::keygen(&mut OsRng);

        assert_ne!(pk1.0, pk2.0, "Generated public keys should be distinct");
        assert_ne!(sk1.0, sk2.0, "Generated secret keys should be distinct");
    }

    #[test]
    fn sign_verify_roundtrip() {
        let (pk, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let message = b"test message";

        let sig = MlDsa65Marker::sign(&sk, message);
        assert!(
            MlDsa65Marker::verify(&pk, message, &sig),
            "Signature should verify"
        );
    }

    #[test]
    fn tampered_signature_rejected() {
        let (pk, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let message = b"test message";

        let mut sig = MlDsa65Marker::sign(&sk, message);

        // Flip one byte in signature
        if !sig.0.is_empty() {
            sig.0[0] ^= 0xFF;
            assert!(
                !MlDsa65Marker::verify(&pk, message, &sig),
                "Tampered signature should not verify"
            );

            // Flip back and verify again
            sig.0[0] ^= 0xFF;
            assert!(
                MlDsa65Marker::verify(&pk, message, &sig),
                "Restored signature should verify"
            );
        }

        // Tamper with message
        let mut tampered_msg = message.to_vec();
        tampered_msg[0] ^= 0xFF;
        assert!(
            !MlDsa65Marker::verify(&pk, &tampered_msg, &sig),
            "Signature over tampered message should not verify"
        );
    }

    #[test]
    fn mldsa65_kat_deterministic() {
        // Use deterministic keygen by signing with the same key twice
        // and verifying consistency. Since our sign() uses deterministic rnd (all zeros),
        // the signatures should be identical.

        let (pk, sk) = MlDsa65Marker::keygen(&mut OsRng);
        let message = b"deterministic test message";

        let sig1 = MlDsa65Marker::sign(&sk, message);
        let sig2 = MlDsa65Marker::sign(&sk, message);

        // Signatures are deterministic when using the same rnd value (all zeros)
        assert_eq!(sig1.0, sig2.0, "Same message should produce same signature");
        assert!(
            MlDsa65Marker::verify(&pk, message, &sig1),
            "First signature should verify"
        );
        assert!(
            MlDsa65Marker::verify(&pk, message, &sig2),
            "Second signature should verify"
        );

        // Different message should not verify
        let different_msg = b"different message";
        assert!(
            !MlDsa65Marker::verify(&pk, different_msg, &sig1),
            "Signature should not verify for different message"
        );
    }
}
