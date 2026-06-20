//! YubiHSM2 backend wrapping hardware-backed signing for Ed25519.
//!
//! YubiHSM2 firmware supports Ed25519 in hardware. ML-DSA-65 is NOT
//! supported as of firmware ≤ 2.6 (the latest at Phase 3 m6 review
//! time). Hybrid signing therefore takes a HYBRID FALLBACK path:
//!
//!   - Ed25519: HARDWARE (YubiHSM2 firmware).
//!   - ML-DSA-65: SOFT-HSM (Argon2id + ChaCha20-Poly1305 wrapper
//!     from m7-006).
//!
//! This hybrid is INTENTIONAL: the Ed25519 leg gains hardware-rooted
//! protection while ML-DSA-65 keeps the post-quantum claim. Both legs
//! must succeed for the hybrid signature to validate. The PQ leg's
//! storage is software-protected; an attacker who exfiltrates the
//! ML-DSA-65 soft-HSM blob would also need to compromise the
//! YubiHSM2's Ed25519 key to forge a signature.
//!
//! The Q0902 diagnostic ("hsm-no-pq-support") fires at init time
//! unless the operator passed --opt-in-hybrid-fallback to acknowledge.

use super::{HsmSigner, HsmSignerError};
use thiserror::Error;

/// YubiHSM2 error types.
#[derive(Debug, Error)]
pub enum YubiHsmError {
    /// YubiHSM2 connector initialization failed.
    #[error("YubiHSM2 connection failed: {0}")]
    Connection(String),

    /// Ed25519 key not found in YubiHSM2.
    #[error("Ed25519 key {0} not present in YubiHSM2")]
    KeyNotFound(u16),

    /// Sign operation failed.
    #[error("sign failed: {0}")]
    Sign(String),

    /// Operator opt-in required for hybrid fallback.
    #[error("operator opt-in required for hybrid fallback (--opt-in-hybrid-fallback); fires Q0902")]
    OptInRequired,

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),
}

/// YubiHSM2 signer with hybrid fallback for post-quantum leg.
///
/// Holds connector URL and Ed25519 key ID for hardware signing,
/// plus a soft-HSM instance for the ML-DSA-65 fallback.
/// Phase-3 minimum: actual yubihsm crate integration is a follow-up;
/// this scaffold documents the hybrid approach and enforces opt-in.
pub struct YubiHsmSigner {
    /// YubiHSM2 connector URL.
    connector_url: String,
    /// Ed25519 key ID in YubiHSM2.
    ed25519_key_id: u16,
    /// Marker: opt-in flag was checked at init.
    opt_in_confirmed: bool,
}

impl YubiHsmSigner {
    /// Create a new YubiHSM2 signer with hybrid fallback.
    ///
    /// Requires explicit operator opt-in via `opt_in_hybrid_fallback`.
    /// Without it, returns OptInRequired error (triggers Q0902).
    ///
    /// # Arguments
    ///
    /// * `connector_url` - URL for YubiHSM2 connector (e.g., "http://localhost:12345")
    /// * `ed25519_key_id` - Key ID (0-65535) for Ed25519 key in YubiHSM2
    /// * `opt_in_hybrid_fallback` - Must be true to proceed (acknowledges PQ fallback)
    ///
    /// # Errors
    ///
    /// Returns `YubiHsmError::OptInRequired` if opt-in is not provided.
    /// Returns `YubiHsmError::Connection` if connector URL is invalid.
    /// Returns `YubiHsmError::Config` if parameters are invalid.
    pub fn new(
        connector_url: &str,
        ed25519_key_id: u16,
        opt_in_hybrid_fallback: bool,
    ) -> Result<Self, YubiHsmError> {
        if !opt_in_hybrid_fallback {
            return Err(YubiHsmError::OptInRequired);
        }

        if connector_url.is_empty() {
            return Err(YubiHsmError::Config(
                "connector_url cannot be empty".to_string(),
            ));
        }

        // Phase-3 scaffold: validate URL format minimally.
        // Real implementation would establish connection and verify key presence.
        if !connector_url.starts_with("http://") && !connector_url.starts_with("https://") {
            return Err(YubiHsmError::Config(
                "connector_url must start with http:// or https://".to_string(),
            ));
        }

        Ok(YubiHsmSigner {
            connector_url: connector_url.to_string(),
            ed25519_key_id,
            opt_in_confirmed: true,
        })
    }

    /// Sign a message with the Ed25519 key in YubiHSM2 (hardware).
    ///
    /// Phase-3 minimum: returns Connection error; real signing requires
    /// yubihsm crate and a running YubiHSM2 at runtime.
    pub fn sign_ed25519(&self, _msg: &[u8]) -> Result<Vec<u8>, YubiHsmError> {
        Err(YubiHsmError::Connection(
            "YubiHSM2 signing not yet implemented in phase-3 scaffold (requires yubihsm crate)"
                .to_string(),
        ))
    }

    /// Sign a message with ML-DSA-65 via soft-HSM (fallback).
    ///
    /// Phase-3 minimum: returns Config error; real signing requires
    /// a configured soft-HSM instance at init time.
    pub fn sign_mldsa65(&self, _msg: &[u8]) -> Result<Vec<u8>, YubiHsmError> {
        Err(YubiHsmError::Config(
            "ML-DSA-65 soft-HSM signing not yet implemented in phase-3 scaffold".to_string(),
        ))
    }

    /// Get the connector URL.
    pub fn connector_url(&self) -> &str {
        &self.connector_url
    }

    /// Get the Ed25519 key ID.
    pub fn ed25519_key_id(&self) -> u16 {
        self.ed25519_key_id
    }

    /// Check if opt-in was confirmed.
    pub fn opt_in_confirmed(&self) -> bool {
        self.opt_in_confirmed
    }
}

impl HsmSigner for YubiHsmSigner {
    fn sign_ed25519(&self, msg: &[u8]) -> Result<Vec<u8>, HsmSignerError> {
        self.sign_ed25519(msg).map_err(HsmSignerError::YubiHsm)
    }

    fn sign_mldsa65(&self, msg: &[u8]) -> Result<Vec<u8>, HsmSignerError> {
        self.sign_mldsa65(msg).map_err(HsmSignerError::YubiHsm)
    }

    fn is_hardware(&self) -> bool {
        // YubiHSM2 provides hardware-backed Ed25519 signing.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yubihsm_init_without_opt_in_returns_opt_in_required_error() {
        let result = YubiHsmSigner::new("http://localhost:12345", 42, false);
        assert!(result.is_err());
        match result {
            Err(YubiHsmError::OptInRequired) => {
                // Expected
            }
            _ => panic!("Expected OptInRequired error"),
        }
    }

    #[test]
    fn yubihsm_init_with_opt_in_succeeds_in_scaffold_mode() {
        let result = YubiHsmSigner::new("http://localhost:12345", 42, true);
        assert!(result.is_ok());
        let signer = result.unwrap();
        assert_eq!(signer.connector_url(), "http://localhost:12345");
        assert_eq!(signer.ed25519_key_id(), 42);
        assert!(signer.opt_in_confirmed());
    }

    #[test]
    fn yubihsm_init_with_https_connector_url_succeeds() {
        let result = YubiHsmSigner::new("https://hsm.example.com:12345", 99, true);
        assert!(result.is_ok());
        let signer = result.unwrap();
        assert_eq!(signer.connector_url(), "https://hsm.example.com:12345");
        assert_eq!(signer.ed25519_key_id(), 99);
    }

    #[test]
    fn yubihsm_init_without_scheme_returns_config_error() {
        let result = YubiHsmSigner::new("localhost:12345", 42, true);
        assert!(result.is_err());
        match result {
            Err(YubiHsmError::Config(msg)) => {
                assert!(msg.contains("http"));
            }
            _ => panic!("Expected Config error"),
        }
    }

    #[test]
    fn yubihsm_init_with_empty_connector_url_returns_config_error() {
        let result = YubiHsmSigner::new("", 42, true);
        assert!(result.is_err());
        match result {
            Err(YubiHsmError::Config(msg)) => {
                assert!(msg.contains("cannot be empty"));
            }
            _ => panic!("Expected Config error"),
        }
    }

    #[test]
    fn yubihsm_error_shapes_serialize_cleanly() {
        let errors = vec![
            YubiHsmError::Connection("test connection error".to_string()),
            YubiHsmError::KeyNotFound(42),
            YubiHsmError::Sign("test sign error".to_string()),
            YubiHsmError::OptInRequired,
            YubiHsmError::Config("test config error".to_string()),
        ];

        for err in errors {
            let err_string = err.to_string();
            assert!(!err_string.is_empty(), "Error should have a display string");
        }
    }

    #[test]
    fn yubihsm_q0902_diagnostic_mentions_opt_in() {
        // This test verifies that the OptInRequired error message
        // mentions Q0902, as documented in the brief.
        let err = YubiHsmError::OptInRequired;
        let err_string = err.to_string();
        assert!(
            err_string.contains("Q0902"),
            "OptInRequired error should reference Q0902 diagnostic"
        );
    }

    #[test]
    fn signer_trait_is_hardware_true_for_yubihsm() {
        let signer =
            YubiHsmSigner::new("http://localhost:12345", 42, true).expect("Should create signer");
        assert!(
            signer.is_hardware(),
            "YubiHSM signer should report hardware=true"
        );
    }
}
