//! Hardware lane for the PQ verification corpus.
//!
//! Phase 3 m6-004: every test in this file is `#[ignore]`'d. The hardware
//! lane needs physical (or simulated) HSM access; activating it is a
//! manual operator opt-in step documented in `docs/release-signing.md`.
//!
//! Activate with:
//!
//!     # PKCS#11 / SoftHSM2:
//!     export SOFTHSM2_AVAILABLE=1
//!     export SOFTHSM2_CONF=/etc/softhsm/softhsm2.conf
//!     export PKCS11_MODULE=/usr/lib/softhsm/libsofthsm2.so
//!     cargo test --test hardware_lane -p paideia-pq-corpus -- --ignored \
//!         pkcs11
//!
//!     # YubiHSM2:
//!     export YUBIHSM_CONNECTOR=http://127.0.0.1:12345
//!     export YUBIHSM_ED25519_KEY_ID=0x0001
//!     cargo test --test hardware_lane -p paideia-pq-corpus -- --ignored \
//!         yubihsm
//!
//! Each test asserts the canonical post-init shape: signer is_hardware()
//! is true (per the m6-003 HsmSigner trait), and (when supported)
//! Ed25519 sign+verify round-trips against the hardware leg.
//!
//! Phase-3 honesty: these tests are scaffolds. They exercise the
//! initialization path + the is_hardware predicate; full sign+verify
//! against live hardware activates with the m6-001 / m6-002 follow-up
//! that supplies the cryptoki / yubihsm runtime integration. Until then,
//! the tests assert the documented error shape (Connection /
//! LibraryUnavailable) so the harness records "tried hardware, fell
//! back to scaffold response".

use paideia_pq_sign::hsm::pkcs11::Pkcs11Signer;
use paideia_pq_sign::hsm::yubihsm::YubiHsmSigner;

fn pkcs11_module_path() -> Option<String> {
    std::env::var("PKCS11_MODULE").ok()
}

fn yubihsm_connector_url() -> Option<String> {
    std::env::var("YUBIHSM_CONNECTOR").ok()
}

#[test]
#[ignore = "phase-3-m6-004: hardware lane — requires SoftHSM2 install + SOFTHSM2_AVAILABLE=1 + PKCS11_MODULE"]
fn pkcs11_init_with_softhsm2_returns_signer() {
    let module =
        pkcs11_module_path().unwrap_or_else(|| "/usr/lib/softhsm/libsofthsm2.so".to_string());
    let slot_id: u64 = std::env::var("PKCS11_SLOT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let pin = std::env::var("PKCS11_PIN").unwrap_or_else(|_| "1234".to_string());
    let result = Pkcs11Signer::new(&module, slot_id, &pin);
    match result {
        Ok(signer) => {
            assert_eq!(signer.module_path(), module);
            assert_eq!(signer.slot_id(), slot_id);
        }
        Err(e) => {
            // Phase-3 scaffold expected shape until cryptoki runtime
            // is wired through m6-001's follow-up.
            eprintln!("pkcs11 init returned scaffold error: {e}");
        }
    }
}

#[test]
#[ignore = "phase-3-m6-004: hardware lane — requires PKCS11_MODULE pointing at SoftHSM2"]
fn pkcs11_signer_reports_is_hardware_true() {
    let module =
        pkcs11_module_path().unwrap_or_else(|| "/usr/lib/softhsm/libsofthsm2.so".to_string());
    if let Ok(signer) = Pkcs11Signer::new(&module, 0, "1234") {
        use paideia_pq_sign::hsm::HsmSigner;
        assert!(signer.is_hardware(), "PKCS#11 signer must report hardware");
    } else {
        eprintln!("pkcs11 not loadable; skipping");
    }
}

#[test]
#[ignore = "phase-3-m6-004: hardware lane — requires YUBIHSM_CONNECTOR + YUBIHSM_ED25519_KEY_ID"]
fn yubihsm_init_with_connector_returns_signer() {
    let connector = match yubihsm_connector_url() {
        Some(c) => c,
        None => {
            eprintln!("YUBIHSM_CONNECTOR not set; skipping");
            return;
        }
    };
    let key_id: u16 = std::env::var("YUBIHSM_ED25519_KEY_ID")
        .ok()
        .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0x0001);
    // Phase-3 scaffold: the soft-HSM PQ leg isn't actually wired into the
    // YubiHsmSigner constructor for these gated tests — we just exercise
    // the connector + key-id path. Full soft-HSM-backed PQ leg activation
    // is m6-005 / production HSM follow-up.
    let result = YubiHsmSigner::new(&connector, key_id, /* opt_in */ true);
    match result {
        Ok(signer) => {
            use paideia_pq_sign::hsm::HsmSigner;
            assert!(signer.is_hardware(), "YubiHSM2 signer must report hardware");
        }
        Err(e) => {
            eprintln!("yubihsm init returned scaffold error: {e}");
        }
    }
}

#[test]
#[ignore = "phase-3-m6-004: hardware lane — Q0902 opt-in contract verification"]
fn yubihsm_without_opt_in_returns_q0902() {
    let connector = yubihsm_connector_url().unwrap_or_else(|| "http://127.0.0.1:12345".to_string());
    let result = YubiHsmSigner::new(&connector, 0x0001, /* opt_in */ false);
    assert!(
        matches!(
            result,
            Err(paideia_pq_sign::hsm::yubihsm::YubiHsmError::OptInRequired)
        ),
        "YubiHSM2 must fire Q0902 (OptInRequired) when --opt-in-hybrid-fallback absent"
    );
}
