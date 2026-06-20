//! HybridSigner: composes a hardware Ed25519 signer with a soft-HSM
//! ML-DSA-65 signer. This is the canonical composition the YubiHSM2
//! backend (m6-002) requires because YubiHSM2 firmware ≤ 2.6
//! doesn't support ML-DSA-65.
//!
//! The composition makes the explicit hybrid contract:
//! - Ed25519: hardware-protected.
//! - ML-DSA-65: soft-protected (Argon2id + ChaCha20-Poly1305).
//!
//! Validation requires BOTH legs.

use super::{HsmSigner, HsmSignerError};

/// HybridSigner: composes a hardware Ed25519 signer with a soft-HSM
/// ML-DSA-65 signer.
///
/// This signer type enables the common case where:
/// - Ed25519 keys are stored in a hardware HSM (e.g., YubiHSM2)
/// - ML-DSA-65 keys are stored in soft-HSM (Argon2id + ChaCha20-Poly1305)
///
/// Both legs must succeed for the hybrid signature to validate.
/// The trust root carries both an Ed25519 key rooted in the YubiHSM2
/// firmware AND an ML-DSA-65 key protected by the operator's passphrase.
#[derive(Clone)]
pub struct HybridSigner<H: HsmSigner, S: HsmSigner> {
    /// Hardware signer (Ed25519 leg).
    hardware: H,
    /// Soft-HSM signer (ML-DSA-65 leg).
    soft: S,
}

impl<H: HsmSigner, S: HsmSigner> HybridSigner<H, S> {
    /// Create a new HybridSigner by composing a hardware signer with a
    /// soft-HSM signer.
    ///
    /// # Arguments
    ///
    /// * `hardware` - Hardware signer (typically YubiHsmSigner or Pkcs11Signer)
    /// * `soft` - Soft-HSM signer (typically SoftHsm wrapper around SoftHsmFile)
    pub fn new(hardware: H, soft: S) -> Self {
        Self { hardware, soft }
    }

    /// Get a reference to the hardware signer (Ed25519 leg).
    pub fn hardware(&self) -> &H {
        &self.hardware
    }

    /// Get a reference to the soft-HSM signer (ML-DSA-65 leg).
    pub fn soft(&self) -> &S {
        &self.soft
    }

    /// Consume this HybridSigner and return the hardware and soft signers.
    pub fn into_parts(self) -> (H, S) {
        (self.hardware, self.soft)
    }
}

impl<H: HsmSigner, S: HsmSigner> HsmSigner for HybridSigner<H, S> {
    fn sign_ed25519(&self, msg: &[u8]) -> Result<Vec<u8>, HsmSignerError> {
        self.hardware.sign_ed25519(msg)
    }

    fn sign_mldsa65(&self, msg: &[u8]) -> Result<Vec<u8>, HsmSignerError> {
        self.soft.sign_mldsa65(msg)
    }

    fn is_hardware(&self) -> bool {
        // True only when the Ed25519 leg is hardware (the m6-002
        // canonical case). The PQ leg's soft-status is implicit
        // in the hybrid contract.
        self.hardware.is_hardware()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock hardware signer for testing.
    #[derive(Clone)]
    struct MockHardwareSigner;

    impl HsmSigner for MockHardwareSigner {
        fn sign_ed25519(&self, _msg: &[u8]) -> Result<Vec<u8>, HsmSignerError> {
            Ok(vec![0xAA; 64])
        }

        fn sign_mldsa65(&self, _msg: &[u8]) -> Result<Vec<u8>, HsmSignerError> {
            Err(HsmSignerError::Sign(
                "Hardware signer does not support ML-DSA-65".to_string(),
            ))
        }

        fn is_hardware(&self) -> bool {
            true
        }
    }

    /// Mock soft-HSM signer for testing.
    #[derive(Clone)]
    struct MockSoftSigner;

    impl HsmSigner for MockSoftSigner {
        fn sign_ed25519(&self, _msg: &[u8]) -> Result<Vec<u8>, HsmSignerError> {
            Err(HsmSignerError::Sign(
                "Soft signer does not support Ed25519".to_string(),
            ))
        }

        fn sign_mldsa65(&self, _msg: &[u8]) -> Result<Vec<u8>, HsmSignerError> {
            Ok(vec![0xBB; 3309])
        }

        fn is_hardware(&self) -> bool {
            false
        }
    }

    #[test]
    fn hybrid_signer_composes_two_signers_with_correct_is_hardware() {
        let hardware = MockHardwareSigner;
        let soft = MockSoftSigner;
        let hybrid = HybridSigner::new(hardware, soft);

        // Hybrid reports hardware=true because Ed25519 leg is hardware
        assert!(
            hybrid.is_hardware(),
            "Hybrid should report hardware=true when Ed25519 is hardware"
        );
    }

    #[test]
    fn hybrid_signer_sign_ed25519_delegates_to_hardware() {
        let hardware = MockHardwareSigner;
        let soft = MockSoftSigner;
        let hybrid = HybridSigner::new(hardware, soft);

        let msg = b"test message";
        let sig = hybrid.sign_ed25519(msg).expect("Should sign Ed25519");
        assert_eq!(sig.len(), 64, "Ed25519 signature should be 64 bytes");
        assert_eq!(sig, vec![0xAA; 64], "Should match mock hardware signature");
    }

    #[test]
    fn hybrid_signer_sign_mldsa65_delegates_to_soft() {
        let hardware = MockHardwareSigner;
        let soft = MockSoftSigner;
        let hybrid = HybridSigner::new(hardware, soft);

        let msg = b"test message";
        let sig = hybrid.sign_mldsa65(msg).expect("Should sign ML-DSA-65");
        assert_eq!(sig.len(), 3309, "ML-DSA-65 signature should be 3309 bytes");
        assert_eq!(sig, vec![0xBB; 3309], "Should match mock soft signature");
    }

    #[test]
    fn hybrid_signer_getters_work() {
        let hardware = MockHardwareSigner;
        let soft = MockSoftSigner;
        let hybrid = HybridSigner::new(hardware.clone(), soft.clone());

        assert!(
            hybrid.hardware().is_hardware(),
            "hardware() should return the hardware signer"
        );
        assert!(
            !hybrid.soft().is_hardware(),
            "soft() should return the soft signer"
        );
    }

    #[test]
    fn hybrid_signer_into_parts_consumes_signer() {
        let hardware = MockHardwareSigner;
        let soft = MockSoftSigner;
        let hybrid = HybridSigner::new(hardware, soft);

        let (h, s) = hybrid.into_parts();
        assert!(h.is_hardware(), "Hardware signer should be hardware");
        assert!(!s.is_hardware(), "Soft signer should not be hardware");
    }
}
