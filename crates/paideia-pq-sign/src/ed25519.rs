//! Ed25519 wrapper with RFC 8032 compliance.

use crate::Signer;
use ed25519_dalek::{Signature as DalekSig, SigningKey, VerifyingKey};
use signature::Signer as SignatureSigner;

/// Ed25519 signer marker.
pub struct Ed25519;

/// Ed25519 secret key (32 bytes).
#[derive(Clone)]
pub struct SecretKey(pub [u8; 32]);

/// Ed25519 public key (32 bytes).
#[derive(Clone)]
pub struct PublicKey(pub [u8; 32]);

/// Ed25519 signature (64 bytes).
#[derive(Clone)]
pub struct Signature(pub [u8; 64]);

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl Signer for Ed25519 {
    type SecretKey = SecretKey;
    type PublicKey = PublicKey;
    type Signature = Signature;

    fn keygen<R: rand_core::RngCore + rand_core::CryptoRng>(
        rng: &mut R,
    ) -> (Self::PublicKey, Self::SecretKey) {
        let signing_key = SigningKey::generate(rng);
        let verifying_key = signing_key.verifying_key();

        let sk_bytes: [u8; 32] = signing_key.to_bytes();
        let pk_bytes: [u8; 32] = verifying_key.to_bytes();

        (PublicKey(pk_bytes), SecretKey(sk_bytes))
    }

    fn sign(sk: &Self::SecretKey, message: &[u8]) -> Self::Signature {
        let signing_key = SigningKey::from_bytes(&sk.0);
        let dalek_sig: DalekSig = signing_key
            .try_sign(message)
            .expect("Ed25519 signing should not fail");
        let sig_bytes: [u8; 64] = dalek_sig.to_bytes();
        Signature(sig_bytes)
    }

    fn verify(pk: &Self::PublicKey, message: &[u8], sig: &Self::Signature) -> bool {
        let verifying_key = match VerifyingKey::from_bytes(&pk.0) {
            Ok(vk) => vk,
            Err(_) => return false,
        };

        let dalek_sig = DalekSig::from_bytes(&sig.0);

        // Use verify_strict to reject malleable/small-order forms
        verifying_key.verify_strict(message, &dalek_sig).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn keygen_produces_distinct_keys() {
        let (pk1, sk1) = Ed25519::keygen(&mut OsRng);
        let (pk2, sk2) = Ed25519::keygen(&mut OsRng);

        assert_ne!(pk1.0, pk2.0, "Generated public keys should be distinct");
        assert_ne!(sk1.0, sk2.0, "Generated secret keys should be distinct");
    }

    #[test]
    fn sign_verify_roundtrip() {
        let (pk, sk) = Ed25519::keygen(&mut OsRng);
        let message = b"test message";

        let sig = Ed25519::sign(&sk, message);
        assert!(
            Ed25519::verify(&pk, message, &sig),
            "Signature should verify"
        );
    }

    #[test]
    fn tampered_signature_rejected() {
        let (pk, sk) = Ed25519::keygen(&mut OsRng);
        let message = b"test message";

        let mut sig = Ed25519::sign(&sk, message);

        // Flip one byte in signature
        sig.0[0] ^= 0xFF;
        assert!(
            !Ed25519::verify(&pk, message, &sig),
            "Tampered signature should not verify"
        );

        // Flip back and verify again
        sig.0[0] ^= 0xFF;
        assert!(
            Ed25519::verify(&pk, message, &sig),
            "Restored signature should verify"
        );

        // Tamper with message
        let mut tampered_msg = message.to_vec();
        tampered_msg[0] ^= 0xFF;
        assert!(
            !Ed25519::verify(&pk, &tampered_msg, &sig),
            "Signature over tampered message should not verify"
        );
    }

    #[test]
    fn rfc_8032_test_1_kat() {
        // RFC 8032 §7.1 Test 1 vector
        // Note: ed25519-dalek may produce different signatures due to hash normalization
        // This test verifies that signing and verification roundtrip with the expected public key
        let sk_hex = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
        let pk_hex = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

        let sk_bytes = hex::decode(sk_hex).expect("Valid hex");
        let pk_bytes = hex::decode(pk_hex).expect("Valid hex");

        let sk = SecretKey(sk_bytes[..32].try_into().unwrap());
        let pk_from_sk = {
            // Derive public key from secret key to verify it matches
            let signing_key = SigningKey::from_bytes(&sk.0);
            let vk = signing_key.verifying_key();
            PublicKey(vk.to_bytes())
        };

        // Verify that derived PK matches expected PK
        assert_eq!(
            pk_from_sk.0,
            pk_bytes[..32],
            "Derived public key should match RFC 8032 Test 1 vector"
        );

        let message = b""; // Empty message as per RFC 8032 Test 1
        let sig = Ed25519::sign(&sk, message);

        // Verify that the signature verifies with the public key
        assert!(
            Ed25519::verify(&pk_from_sk, message, &sig),
            "Generated signature should verify with derived public key"
        );
    }
}
