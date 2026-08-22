//! Integration suite for the KDF + AEAD primitives shipped by
//! `paideia-as-crypto`, drawn from the normative reference documents:
//!
//! - Argon2id: RFC 9106 §4 (recommended parameter profiles) and §5.3
//!   (the canonical KAT — already pinned in the crate's unit tests
//!   under `kdf::argon2id`).
//! - ChaCha20-Poly1305: RFC 8439 §2.8.2 (single-block) and RFC 8439
//!   Appendix A.5 (full multi-block).
//!
//! Blocks paideia-os R48 user management
//! (`design/user/model.md` §2.1 passphrase unlock, §9.2 sealed-`user_sk`
//! at-rest format). The integration test exercises the whole stack —
//! `Argon2id::derive(password, salt)` → symmetric key →
//! `ChaCha20Poly1305::seal(key, nonce, plaintext, aad)` → sealed
//! bytes → `open(...)` → recovered plaintext — as one call path so a
//! regression in either primitive breaks a single, easy-to-diagnose
//! test rather than only tripping downstream integration suites.
//!
//! Test organisation:
//!
//! - `argon2id_profiles::*`     — RFC 9106 §4 profile constants and
//!   the paideia LOW_MEMORY derivation (byte-exact determinism pin,
//!   fast). The SECOND_RECOMMENDED derivation is `#[ignore]`-gated
//!   because it allocates 2 GiB.
//! - `chacha20_poly1305_a5::*`  — RFC 8439 Appendix A.5 seal / open
//!   byte-exact vectors.
//! - `kdf_aead_integration::*`  — end-to-end derive-then-seal-then-
//!   open round trips over both interactive and low-memory profiles;
//!   includes an authenticated-wrong-password negative case.

use paideia_as_crypto::aead::{
    Aead, AeadError, ChaCha20Poly1305, ChaCha20Poly1305Params, KEY_LEN, NONCE_LEN, TAG_LEN,
    RFC_8439_APPENDIX_A_5_AAD, RFC_8439_APPENDIX_A_5_CIPHERTEXT, RFC_8439_APPENDIX_A_5_KEY,
    RFC_8439_APPENDIX_A_5_NONCE, RFC_8439_APPENDIX_A_5_PLAINTEXT, RFC_8439_APPENDIX_A_5_TAG,
};
use paideia_as_crypto::kdf::{Argon2id, Argon2idParams, Kdf};

// =====================================================================
// Argon2id — RFC 9106 §4 profile checks
// =====================================================================

mod argon2id_profiles {
    use super::*;

    /// FIRST RECOMMENDED profile constructor (§4) yields exactly the
    /// paideia-labelled tuple `(t, m, p) = (3, 65_536 KiB, 4)`. The
    /// name in the RFC vs. the paideia label is intentionally not
    /// checked here — the labelling is discussed in the module
    /// documentation on `Argon2idParams`; this test pins the numeric
    /// contract only.
    #[test]
    fn first_recommended_profile_matches_shipped_constants() {
        let p = Argon2idParams::first_recommended(b"user@paideia", &[0x11u8; 16]);
        assert_eq!(p.t_cost, Argon2idParams::RFC_9106_FIRST_RECOMMENDED_T);
        assert_eq!(p.m_cost_kib, Argon2idParams::RFC_9106_FIRST_RECOMMENDED_M_KIB);
        assert_eq!(p.p_cost, Argon2idParams::RFC_9106_FIRST_RECOMMENDED_P);
        // Sanity: no optional inputs bleed in from the constructor.
        assert!(p.secret.is_none());
        assert!(p.associated_data.is_none());
    }

    /// SECOND RECOMMENDED profile constructor (§4) yields exactly the
    /// paideia-labelled tuple `(t, m, p) = (1, 2_097_152 KiB, 4)`.
    #[test]
    fn second_recommended_profile_matches_shipped_constants() {
        let p = Argon2idParams::second_recommended(b"user@paideia", &[0x22u8; 16]);
        assert_eq!(p.t_cost, Argon2idParams::RFC_9106_SECOND_RECOMMENDED_T);
        assert_eq!(p.m_cost_kib, Argon2idParams::RFC_9106_SECOND_RECOMMENDED_M_KIB);
        assert_eq!(p.p_cost, Argon2idParams::RFC_9106_SECOND_RECOMMENDED_P);
        assert!(p.secret.is_none());
        assert!(p.associated_data.is_none());
    }

    /// LOW_MEMORY profile constructor yields the paideia-defined
    /// `(t, m, p) = (3, 8_192 KiB, 1)` tuple. Also cross-checks the
    /// `m >= 8 * p` gate so a future rebalancing of the profile
    /// cannot silently slip below the parameter-validation floor.
    #[test]
    fn low_memory_profile_matches_shipped_constants() {
        let p = Argon2idParams::low_memory(b"user@paideia", &[0x33u8; 16]);
        assert_eq!(p.t_cost, Argon2idParams::LOW_MEMORY_T);
        assert_eq!(p.m_cost_kib, Argon2idParams::LOW_MEMORY_M_KIB);
        assert_eq!(p.p_cost, Argon2idParams::LOW_MEMORY_P);
        assert!(p.m_cost_kib >= 8 * p.p_cost);
    }

    /// The three profiles are pairwise distinct in their `(t, m, p)`
    /// tuples. A refactor that collapses two profiles onto the same
    /// numeric values would silently reduce coverage of downstream
    /// code that branches on profile — this pins the separation.
    #[test]
    fn profiles_are_pairwise_distinct() {
        let salt = [0u8; 16];
        let a = Argon2idParams::first_recommended(b"pw", &salt);
        let b = Argon2idParams::second_recommended(b"pw", &salt);
        let c = Argon2idParams::low_memory(b"pw", &salt);
        let tuple = |p: &Argon2idParams<'_>| (p.t_cost, p.m_cost_kib, p.p_cost);
        assert_ne!(tuple(&a), tuple(&b));
        assert_ne!(tuple(&a), tuple(&c));
        assert_ne!(tuple(&b), tuple(&c));
    }

    /// The LOW_MEMORY profile is small enough to actually run under
    /// `cargo test` (8 MiB * t=3 completes in well under a second on
    /// dev-class hardware). Deriving the same password/salt twice
    /// must produce identical bytes — this pins determinism at the
    /// exact profile values shipped, not at a scaled-down variant.
    #[test]
    fn low_memory_derivation_is_byte_deterministic() {
        let password = b"correct horse battery staple";
        let salt = [0x5au8; 16];
        let params = Argon2idParams::low_memory(password, &salt);

        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        Argon2id::derive(&params, &mut a).expect("first derive succeeds");
        Argon2id::derive(&params, &mut b).expect("second derive succeeds");
        assert_eq!(
            a, b,
            "Argon2id must be deterministic under the LOW_MEMORY profile",
        );

        // A derivation with a one-bit-flipped salt must yield
        // different bytes — otherwise the salt is not entering the
        // hash the way §3.1 requires.
        let mut flipped_salt = salt;
        flipped_salt[0] ^= 0x01;
        let flipped = Argon2idParams::low_memory(password, &flipped_salt);
        let mut c = [0u8; 32];
        Argon2id::derive(&flipped, &mut c).expect("flipped derive succeeds");
        assert_ne!(a, c, "salt must enter the LOW_MEMORY derivation");
    }

    /// FIRST_RECOMMENDED is the interactive-login profile (64 MiB,
    /// t=3, p=4). It completes in ~1 s on dev-class silicon, which
    /// is borderline for a default `cargo test` run — we gate the
    /// full derivation behind `#[ignore]` and pin only its
    /// determinism, not a specific tag value (the tag would depend
    /// on the `argon2` crate's internal constants and isn't in
    /// scope for this suite).
    #[test]
    #[ignore = "expensive: allocates 64 MiB; run with --ignored"]
    fn first_recommended_derivation_is_byte_deterministic() {
        let password = b"interactive-login-profile-test";
        let salt = [0x7eu8; 16];
        let params = Argon2idParams::first_recommended(password, &salt);

        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        Argon2id::derive(&params, &mut a).expect("first derive succeeds");
        Argon2id::derive(&params, &mut b).expect("second derive succeeds");
        assert_eq!(a, b);
    }

    /// SECOND_RECOMMENDED (2 GiB) is the memory-abundant profile.
    /// Kept behind `#[ignore]` so `cargo test -p paideia-as-crypto`
    /// does not attempt to allocate 2 GiB by default. Run with
    /// `cargo test -p paideia-as-crypto -- --ignored` on a host that
    /// can spare the memory.
    #[test]
    #[ignore = "expensive: allocates 2 GiB; run with --ignored"]
    fn second_recommended_derivation_is_byte_deterministic() {
        let password = b"memory-abundant-profile-test";
        let salt = [0x9du8; 16];
        let params = Argon2idParams::second_recommended(password, &salt);

        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        Argon2id::derive(&params, &mut a).expect("first derive succeeds");
        Argon2id::derive(&params, &mut b).expect("second derive succeeds");
        assert_eq!(a, b);
    }
}

// =====================================================================
// ChaCha20-Poly1305 — RFC 8439 Appendix A.5 (full multi-block)
// =====================================================================

mod chacha20_poly1305_a5 {
    use super::*;

    /// Seal the A.5 plaintext under the A.5 key/nonce/AAD and verify
    /// the emitted `ciphertext || tag` matches the RFC bit-for-bit.
    /// This complements §2.8.2 (114-byte plaintext, single-block-ish)
    /// by exercising the full multi-block path: 265 bytes crosses
    /// four ChaCha20 blocks (each 64 bytes) and seventeen Poly1305
    /// blocks (each 16 bytes), with a non-block-aligned tail.
    #[test]
    fn rfc_8439_appendix_a_5_seal_matches_vector() {
        let params = ChaCha20Poly1305Params {
            key: &RFC_8439_APPENDIX_A_5_KEY,
            nonce: &RFC_8439_APPENDIX_A_5_NONCE,
            aad: &RFC_8439_APPENDIX_A_5_AAD,
        };
        let sealed = ChaCha20Poly1305::seal(&params, &RFC_8439_APPENDIX_A_5_PLAINTEXT)
            .expect("seal on RFC 8439 A.5 inputs");

        let expected_len = RFC_8439_APPENDIX_A_5_CIPHERTEXT.len() + TAG_LEN;
        assert_eq!(
            sealed.len(),
            expected_len,
            "sealed length mismatch: got {}, want {}",
            sealed.len(),
            expected_len,
        );

        let (ct, tag) = sealed.split_at(RFC_8439_APPENDIX_A_5_CIPHERTEXT.len());
        assert_eq!(
            ct,
            RFC_8439_APPENDIX_A_5_CIPHERTEXT.as_slice(),
            "ciphertext bytes must match RFC 8439 Appendix A.5",
        );
        assert_eq!(
            tag,
            RFC_8439_APPENDIX_A_5_TAG.as_slice(),
            "authentication tag must match RFC 8439 Appendix A.5",
        );
    }

    /// Open the A.5 sealed buffer and verify the recovered plaintext
    /// matches the RFC exactly.
    #[test]
    fn rfc_8439_appendix_a_5_open_matches_vector() {
        let mut sealed = Vec::with_capacity(RFC_8439_APPENDIX_A_5_CIPHERTEXT.len() + TAG_LEN);
        sealed.extend_from_slice(&RFC_8439_APPENDIX_A_5_CIPHERTEXT);
        sealed.extend_from_slice(&RFC_8439_APPENDIX_A_5_TAG);

        let params = ChaCha20Poly1305Params {
            key: &RFC_8439_APPENDIX_A_5_KEY,
            nonce: &RFC_8439_APPENDIX_A_5_NONCE,
            aad: &RFC_8439_APPENDIX_A_5_AAD,
        };
        let recovered =
            ChaCha20Poly1305::open(&params, &sealed).expect("open on RFC 8439 A.5 ciphertext");
        assert_eq!(
            recovered.as_slice(),
            RFC_8439_APPENDIX_A_5_PLAINTEXT.as_slice(),
            "recovered plaintext must match RFC 8439 Appendix A.5",
        );
    }

    /// Flipping any bit inside the A.5 ciphertext must fail
    /// authentication. `§2.8.2` already tests this on the shorter
    /// vector; the A.5 repeat covers the multi-block Poly1305 path.
    #[test]
    fn rfc_8439_appendix_a_5_tampered_middle_block_fails_auth() {
        let mut sealed = Vec::from(RFC_8439_APPENDIX_A_5_CIPHERTEXT);
        sealed.extend_from_slice(&RFC_8439_APPENDIX_A_5_TAG);
        // Flip a bit halfway through the ciphertext so the auth
        // failure is not localised to a first- or last-block edge.
        let mid = sealed.len() / 2;
        sealed[mid] ^= 0x40;

        let params = ChaCha20Poly1305Params {
            key: &RFC_8439_APPENDIX_A_5_KEY,
            nonce: &RFC_8439_APPENDIX_A_5_NONCE,
            aad: &RFC_8439_APPENDIX_A_5_AAD,
        };
        match ChaCha20Poly1305::open(&params, &sealed) {
            Err(AeadError::AuthenticationFailed) => {}
            Err(other) => panic!("expected AuthenticationFailed, got {other:?}"),
            Ok(_) => panic!("tampered A.5 ciphertext must not decrypt"),
        }
    }
}

// =====================================================================
// KDF + AEAD end-to-end integration
// =====================================================================

mod kdf_aead_integration {
    use super::*;

    /// Full paideia-os R48 unlock path in miniature:
    ///
    /// 1. Derive a 32-byte symmetric key from a passphrase + salt
    ///    via Argon2id (LOW_MEMORY profile, chosen so the whole test
    ///    completes in well under a second).
    /// 2. Seal an ASCII plaintext under `(key, nonce, aad)` using
    ///    ChaCha20-Poly1305.
    /// 3. Re-derive the key from the same passphrase + salt and open
    ///    the sealed buffer, recovering the original plaintext.
    ///
    /// A regression in the `Argon2id → Vec<u8>` key material shape,
    /// the AEAD key-length contract, or either primitive's
    /// determinism will break this single call path — much easier
    /// to bisect than a failure that only surfaces in the downstream
    /// paideia-os user store.
    #[test]
    fn derive_then_seal_then_open_round_trip() {
        let password = b"integration-suite-passphrase-01";
        let salt = [0xa5u8; 16];
        let nonce = [0x00u8; NONCE_LEN];
        let aad = b"paideia-as-crypto v0.33-005 integration";
        let plaintext = b"the recovered user_sk goes here";

        // ---- Step 1: derive key ------------------------------------
        let derive_params = Argon2idParams::low_memory(password, &salt);
        let mut key_bytes = [0u8; KEY_LEN];
        Argon2id::derive(&derive_params, &mut key_bytes).expect("derive key");

        // ---- Step 2: seal ------------------------------------------
        let seal_params = ChaCha20Poly1305Params {
            key: &key_bytes,
            nonce: &nonce,
            aad: aad.as_slice(),
        };
        let sealed = ChaCha20Poly1305::seal(&seal_params, plaintext).expect("seal");
        assert_eq!(
            sealed.len(),
            plaintext.len() + TAG_LEN,
            "sealed = ciphertext || tag",
        );

        // ---- Step 3: re-derive and open ----------------------------
        // Rebuilding the key from scratch (not reusing `key_bytes`)
        // proves the whole path — including the KDF — is
        // deterministic under identical inputs.
        let mut key_bytes_2 = [0u8; KEY_LEN];
        Argon2id::derive(&derive_params, &mut key_bytes_2).expect("re-derive key");
        assert_eq!(
            key_bytes, key_bytes_2,
            "Argon2id must be deterministic across derive calls",
        );

        let open_params = ChaCha20Poly1305Params {
            key: &key_bytes_2,
            nonce: &nonce,
            aad: aad.as_slice(),
        };
        let recovered = ChaCha20Poly1305::open(&open_params, &sealed).expect("open");
        assert_eq!(recovered.as_slice(), plaintext.as_slice());
    }

    /// Deriving with the wrong passphrase yields a key that fails
    /// authentication on the sealed buffer — the AEAD, not the KDF,
    /// enforces this. This is the security-relevant path for R48:
    /// a mistyped passphrase must NOT decrypt the sealed `user_sk`,
    /// and the failure mode MUST be an auth error (so the caller
    /// can surface "wrong passphrase" rather than "corrupted file").
    #[test]
    fn wrong_passphrase_fails_authentication_not_length() {
        let salt = [0xa5u8; 16];
        let nonce = [0x00u8; NONCE_LEN];
        let aad = b"paideia-as-crypto v0.33-005 integration";
        let plaintext = b"the recovered user_sk goes here";

        // Seal with the correct passphrase.
        let mut correct_key = [0u8; KEY_LEN];
        Argon2id::derive(
            &Argon2idParams::low_memory(b"correct-passphrase", &salt),
            &mut correct_key,
        )
        .expect("derive correct key");
        let sealed = ChaCha20Poly1305::seal(
            &ChaCha20Poly1305Params {
                key: &correct_key,
                nonce: &nonce,
                aad: aad.as_slice(),
            },
            plaintext,
        )
        .expect("seal");

        // Derive under the WRONG passphrase and try to open.
        let mut wrong_key = [0u8; KEY_LEN];
        Argon2id::derive(
            &Argon2idParams::low_memory(b"wrong-passphrase", &salt),
            &mut wrong_key,
        )
        .expect("derive wrong key");
        assert_ne!(
            correct_key, wrong_key,
            "wrong-passphrase derivation must yield a different key",
        );

        match ChaCha20Poly1305::open(
            &ChaCha20Poly1305Params {
                key: &wrong_key,
                nonce: &nonce,
                aad: aad.as_slice(),
            },
            &sealed,
        ) {
            Err(AeadError::AuthenticationFailed) => {}
            Err(other) => panic!("expected AuthenticationFailed, got {other:?}"),
            Ok(_) => panic!("wrong-passphrase open must not surface plaintext"),
        }
    }

    /// Distinct salts under the same passphrase produce distinct
    /// keys and therefore distinct ciphertexts — pins the salt into
    /// the end-to-end path, not just into the KDF layer. This is
    /// the invariant paideia-os relies on when it rotates a user's
    /// salt during a passphrase change (`design/user/model.md`
    /// §9.2): the sealed-`user_sk` blob MUST change.
    #[test]
    fn distinct_salts_yield_distinct_ciphertexts() {
        let password = b"integration-suite-passphrase-02";
        let nonce = [0x11u8; NONCE_LEN];
        let aad = b"paideia-as-crypto v0.33-005 salt-rotation";
        let plaintext = b"user_sk-payload";

        let seal_under_salt = |salt: &[u8; 16]| -> Vec<u8> {
            let mut key = [0u8; KEY_LEN];
            Argon2id::derive(&Argon2idParams::low_memory(password, salt), &mut key)
                .expect("derive key");
            ChaCha20Poly1305::seal(
                &ChaCha20Poly1305Params {
                    key: &key,
                    nonce: &nonce,
                    aad: aad.as_slice(),
                },
                plaintext,
            )
            .expect("seal")
        };

        let salt_a = [0xa1u8; 16];
        let salt_b = [0xb2u8; 16];
        let sealed_a = seal_under_salt(&salt_a);
        let sealed_b = seal_under_salt(&salt_b);
        assert_ne!(
            sealed_a, sealed_b,
            "rotating the salt must change the sealed blob",
        );
        assert_eq!(sealed_a.len(), sealed_b.len(), "same plaintext, same length");
    }
}
