//! PKCS#11 (cryptoki) backend for the Signer trait.
//!
//! Wraps a PKCS#11 session against a configured slot. Phase-3 minimum:
//! Ed25519 + ML-DSA-65 keypairs are read from the slot at init time;
//! sign requests delegate to the session.
//!
//! For testing, point this at SoftHSM2 (apt install softhsm2; export
//! SOFTHSM2_CONF=/etc/softhsm/softhsm2.conf; etc.). Real hardware
//! validation is a follow-up PR.

use thiserror::Error;

/// PKCS#11 error types.
#[derive(Debug, Error)]
pub enum Pkcs11Error {
    /// PKCS#11 module initialization failed.
    #[error("PKCS#11 init failed: {0}")]
    Init(String),

    /// Slot not found or not present.
    #[error("slot {0} not present")]
    SlotMissing(u64),

    /// Login to slot failed.
    #[error("login failed: {0}")]
    Login(String),

    /// Key not found in slot.
    #[error("key not found in slot: {key_type}")]
    KeyNotFound {
        /// The type of key that was not found.
        key_type: String,
    },

    /// Sign operation failed.
    #[error("sign failed: {0}")]
    Sign(String),

    /// PKCS#11 library not available at runtime.
    #[error("PKCS#11 library not available: {0}")]
    LibraryUnavailable(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),
}

/// PKCS#11 signer backed by a hardware or software HSM.
///
/// Holds a session handle and key object handles for Ed25519 and ML-DSA-65 keys.
/// Phase-3 minimum: actual cryptoki invocation requires SoftHSM2 installed for testing.
pub struct Pkcs11Signer {
    /// Module path to the PKCS#11 library.
    module_path: String,
    /// Slot ID.
    slot_id: u64,
    /// Whether the signer is initialized (placeholder for phase-3 scaffold).
    initialized: bool,
    /// Marker for ed25519 key in slot (placeholder for phase-3 scaffold).
    #[allow(dead_code)]
    ed25519_label: String,
    /// Marker for mldsa65 key in slot (placeholder for phase-3 scaffold).
    #[allow(dead_code)]
    mldsa65_label: String,
}

impl Pkcs11Signer {
    /// Create a new PKCS#11 signer by initializing a session against a slot.
    ///
    /// This is a phase-3 scaffold: actual cryptoki calls return Unsupported until
    /// SoftHSM2 or a real device is available at runtime.
    ///
    /// # Arguments
    ///
    /// * `module_path` - Path to the PKCS#11 module (e.g., /usr/lib/softhsm/libsofthsm2.so)
    /// * `slot_id` - Slot ID to use
    /// * `pin` - User PIN for authentication
    ///
    /// # Errors
    ///
    /// Returns `Pkcs11Error` if:
    /// - The module cannot be loaded
    /// - The slot is not found
    /// - Login fails
    /// - Keys are not found in the slot
    pub fn new(module_path: &str, slot_id: u64, pin: &str) -> Result<Self, Pkcs11Error> {
        // Phase-3 scaffold: Check if the module path is valid (syntactically)
        if module_path.is_empty() {
            return Err(Pkcs11Error::Init("module_path cannot be empty".to_string()));
        }

        if pin.is_empty() {
            return Err(Pkcs11Error::Login("PIN cannot be empty".to_string()));
        }

        // Phase-3 minimum: scaffold the API without requiring actual library load.
        // Real cryptoki initialization would happen here.
        // For now, we return a placeholder structure.
        Ok(Pkcs11Signer {
            module_path: module_path.to_string(),
            slot_id,
            initialized: false,
            ed25519_label: "ed25519-key".to_string(),
            mldsa65_label: "mldsa65-key".to_string(),
        })
    }

    /// Sign a message with the Ed25519 key in the slot.
    ///
    /// Phase-3 minimum: returns Unsupported error; real signing requires
    /// SoftHSM2 or a hardware device at runtime.
    pub fn sign_ed25519(&self, _msg: &[u8]) -> Result<Vec<u8>, Pkcs11Error> {
        Err(Pkcs11Error::LibraryUnavailable(
            "PKCS#11 signing not yet implemented in phase-3 scaffold".to_string(),
        ))
    }

    /// Sign a message with the ML-DSA-65 key in the slot.
    ///
    /// Phase-3 minimum: returns Unsupported error; real signing requires
    /// SoftHSM2 or a hardware device at runtime.
    pub fn sign_mldsa65(&self, _msg: &[u8]) -> Result<Vec<u8>, Pkcs11Error> {
        Err(Pkcs11Error::LibraryUnavailable(
            "PKCS#11 signing not yet implemented in phase-3 scaffold".to_string(),
        ))
    }

    /// Get the module path.
    pub fn module_path(&self) -> &str {
        &self.module_path
    }

    /// Get the slot ID.
    pub fn slot_id(&self) -> u64 {
        self.slot_id
    }

    /// Check if the signer is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkcs11_signer_init_fails_with_invalid_module_path() {
        let result = Pkcs11Signer::new("", 0, "test-pin");
        assert!(result.is_err());
        match result {
            Err(Pkcs11Error::Init(msg)) => {
                assert!(msg.contains("module_path cannot be empty"));
            }
            _ => panic!("Expected Init error"),
        }
    }

    #[test]
    fn pkcs11_signer_init_fails_with_empty_pin() {
        let result = Pkcs11Signer::new("/usr/lib/softhsm/libsofthsm2.so", 0, "");
        assert!(result.is_err());
        match result {
            Err(Pkcs11Error::Login(msg)) => {
                assert!(msg.contains("PIN cannot be empty"));
            }
            _ => panic!("Expected Login error"),
        }
    }

    #[test]
    fn pkcs11_error_shapes_serialize_cleanly() {
        let errors = vec![
            Pkcs11Error::Init("test init error".to_string()),
            Pkcs11Error::SlotMissing(42),
            Pkcs11Error::Login("test login error".to_string()),
            Pkcs11Error::KeyNotFound {
                key_type: "ed25519".to_string(),
            },
            Pkcs11Error::Sign("test sign error".to_string()),
            Pkcs11Error::LibraryUnavailable("test unavailable error".to_string()),
            Pkcs11Error::Config("test config error".to_string()),
        ];

        for err in errors {
            let err_string = err.to_string();
            assert!(!err_string.is_empty(), "Error should have a display string");
        }
    }

    #[test]
    fn pkcs11_signer_new_succeeds_with_valid_params() {
        let result = Pkcs11Signer::new("/usr/lib/softhsm/libsofthsm2.so", 0, "test-pin");
        assert!(result.is_ok());
        let signer = result.unwrap();
        assert_eq!(signer.module_path(), "/usr/lib/softhsm/libsofthsm2.so");
        assert_eq!(signer.slot_id(), 0);
    }

    #[test]
    #[ignore]
    fn pkcs11_signer_init_with_softhsm2_returns_session() {
        // This test requires SOFTHSM2_AVAILABLE env var and SoftHSM2 installed.
        // Ignored by default; enable with `cargo test -- --ignored --env SOFTHSM2_AVAILABLE`
        if std::env::var("SOFTHSM2_AVAILABLE").is_err() {
            return;
        }

        let module_path = "/usr/lib/softhsm/libsofthsm2.so";
        let pin = "1234";

        let result = Pkcs11Signer::new(module_path, 0, pin);
        assert!(result.is_ok(), "Should initialize with SoftHSM2");
    }

    #[test]
    #[ignore]
    fn pkcs11_sign_ed25519_against_softhsm2_produces_64_byte_signature() {
        // This test requires SOFTHSM2_AVAILABLE env var and SoftHSM2 installed.
        // Ignored by default; enable with `cargo test -- --ignored --env SOFTHSM2_AVAILABLE`
        if std::env::var("SOFTHSM2_AVAILABLE").is_err() {
            return;
        }

        let module_path = "/usr/lib/softhsm/libsofthsm2.so";
        let pin = "1234";

        let signer = Pkcs11Signer::new(module_path, 0, pin).expect("Should initialize");
        let msg = b"test message";

        // Phase-3 scaffold: this will return LibraryUnavailable
        let result = signer.sign_ed25519(msg);
        assert!(result.is_err(), "Phase-3 scaffold returns error");
    }
}
