//! PQ signature verification corpus — 10 tests covering m7-001..006.
//!
//! Happy paths (6):
//! 1. Ed25519 keygen → sign → verify round-trip
//! 2. ML-DSA-65 keygen → sign → verify round-trip
//! 3. Hybrid keygen → sign → verify round-trip (AND semantics)
//! 4. Build minimal PAX → sign content hash → verify via emitter
//! 5. Scope-check passes when key scope ⊇ PAX effects
//! 6. Soft-HSM: generate → encrypt → unlock → sign
//!
//! Failure modes (4):
//! 7. Tampered hybrid signature rejected
//! 8. Wrong public key fails verification
//! 9. Scope-check fails when key scope ⊉ PAX effects (Q0901)
//! 10. Soft-HSM unlock fails with wrong password

use paideia_pq_sign::{
    Ed25519, Hybrid, KeyScope, MlDsa65Marker, Signer, check_delegation_scope, sign_pax_hash,
    soft_hsm::SoftHsmFile, verify_pax_hash,
};
use pq_corpus::build_minimal_pax;
use rand_core::OsRng;

// ============================================================================
// Happy Path Tests (6)
// ============================================================================

#[test]
fn happy_ed25519_keygen_sign_verify_roundtrip() {
    // m7-001: Generate Ed25519 keypair
    let (pk, sk) = Ed25519::keygen(&mut OsRng);

    // Sign a message
    let message = b"test message for Ed25519";
    let signature = Ed25519::sign(&sk, message);

    // Verify should succeed
    assert!(
        Ed25519::verify(&pk, message, &signature),
        "Ed25519 signature should verify"
    );

    // Verify with different message should fail
    let wrong_message = b"wrong message";
    assert!(
        !Ed25519::verify(&pk, wrong_message, &signature),
        "Ed25519 signature should not verify with wrong message"
    );
}

#[test]
fn happy_mldsa65_keygen_sign_verify_roundtrip() {
    // m7-001: Generate ML-DSA-65 keypair
    let (pk, sk) = MlDsa65Marker::keygen(&mut OsRng);

    // Sign a message
    let message = b"test message for ML-DSA-65";
    let signature = MlDsa65Marker::sign(&sk, message);

    // Verify should succeed
    assert!(
        MlDsa65Marker::verify(&pk, message, &signature),
        "ML-DSA-65 signature should verify"
    );

    // Verify with different message should fail
    let wrong_message = b"wrong message";
    assert!(
        !MlDsa65Marker::verify(&pk, wrong_message, &signature),
        "ML-DSA-65 signature should not verify with wrong message"
    );
}

#[test]
fn happy_hybrid_keygen_sign_verify_roundtrip() {
    // m7-002: Generate hybrid keypair (Ed25519 + ML-DSA-65)
    let (pk, sk) = Hybrid::keygen(&mut OsRng);

    // Sign a message
    let message = b"test message for hybrid";
    let signature = Hybrid::sign(&sk, message);

    // Verify should succeed (both components must verify)
    assert!(
        Hybrid::verify(&pk, message, &signature),
        "Hybrid signature should verify"
    );

    // Verify with different message should fail
    let wrong_message = b"wrong message";
    assert!(
        !Hybrid::verify(&pk, wrong_message, &signature),
        "Hybrid signature should not verify with wrong message"
    );

    // Serialize and deserialize to verify round-trip
    let sig_bytes = signature.to_bytes();
    let sig_recovered = paideia_pq_sign::HybridSignature::from_bytes(&sig_bytes)
        .expect("Should deserialize signature");
    assert!(
        Hybrid::verify(&pk, message, &sig_recovered),
        "Recovered signature should verify"
    );
}

#[test]
fn happy_pax_content_hash_sign_and_verify_via_emitter() {
    // m7-003: Build minimal PAX, sign content hash, verify via emitter API
    let pax_bytes = build_minimal_pax(64, "entry_point", 1);

    // Write to tempfile
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let pax_path = temp_dir.path().join("test.pax");
    std::fs::write(&pax_path, &pax_bytes).expect("Failed to write PAX");

    // Parse PAX header and section table to extract content hash
    let header =
        paideia_as_emitter_pax::PaxHeader::from_bytes(&pax_bytes).expect("Should parse header");
    let section_table_offset = header.section_table_offset as usize;
    let sections = paideia_as_emitter_pax::SectionTable::from_bytes(
        &pax_bytes[section_table_offset..],
        header.section_count,
    )
    .expect("Should parse section table");

    // Extract section contents for hash computation
    let mut section_contents = Vec::new();
    for section in &sections.sections {
        let start = section.content_offset as usize;
        let size = section.content_size as usize;
        if start + size <= pax_bytes.len() {
            section_contents.push(pax_bytes[start..start + size].to_vec());
        }
    }

    // Compute canonical content hash
    let content_hash = paideia_as_emitter_pax::compute_content_hash(
        &header,
        &sections,
        &section_contents
            .iter()
            .map(|v| v.as_slice())
            .collect::<Vec<_>>()[..],
    );

    // Generate keypair and sign the content hash
    let (pk, sk) = Hybrid::keygen(&mut OsRng);
    let signature = sign_pax_hash(&sk, &content_hash);

    // Verify signature
    assert!(
        verify_pax_hash(&pk, &content_hash, &signature),
        "PAX content hash signature should verify"
    );

    // Tamper with content hash and verify fails
    let mut tampered_hash = content_hash;
    tampered_hash[0] ^= 0xFF;
    assert!(
        !verify_pax_hash(&pk, &tampered_hash, &signature),
        "Signature should not verify with tampered hash"
    );
}

#[test]
fn happy_scope_check_succeeds_when_key_subsumes_pax_effects() {
    // m7-004: Build PAX with effect IDs {100, 101, 102}.
    // Create key scope that includes all these effects.
    // Scope-check should pass.

    let effect_ids = vec![100u32, 101, 102];
    let pax_bytes = pq_corpus::build_pax_with_effects(&effect_ids);

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let pax_path = temp_dir.path().join("test_scope.pax");
    std::fs::write(&pax_path, &pax_bytes).expect("Failed to write PAX");

    // Parse effects section
    let pax_bytes = std::fs::read(&pax_path).expect("Should read PAX");
    let header =
        paideia_as_emitter_pax::PaxHeader::from_bytes(&pax_bytes).expect("Should parse header");
    let section_table_offset = header.section_table_offset as usize;
    let sections = paideia_as_emitter_pax::SectionTable::from_bytes(
        &pax_bytes[section_table_offset..],
        header.section_count,
    )
    .expect("Should parse section table");

    // Find and parse effects section
    let mut effects_section = paideia_as_emitter_pax::EffectsSection::new();
    for section in &sections.sections {
        if section.ty == paideia_as_emitter_pax::SectionType::Effects {
            let start = section.content_offset as usize;
            let size = section.content_size as usize;
            if start + size <= pax_bytes.len()
                && let Some(effects) = paideia_as_emitter_pax::EffectsSection::from_bytes(
                    &pax_bytes[start..start + size],
                )
            {
                effects_section = effects;
            }
        }
    }

    // Create key scope that subsumes all PAX effects
    let mut key_scope = KeyScope::new();
    for eid in &effect_ids {
        key_scope.add(*eid);
    }

    // Scope-check should pass
    let mut diags = Vec::new();
    assert!(
        check_delegation_scope(&key_scope, &effects_section, &mut diags),
        "Scope-check should pass when key scope subsumes effects"
    );
    assert_eq!(diags.len(), 0, "Should emit no diagnostics");
}

#[test]
fn happy_soft_hsm_init_unlock_and_sign() {
    // m7-006: Generate HSM → encrypt with password → unlock → sign → verify

    // Generate and encrypt HSM file
    let password = b"correct_password";
    let hsm_file = SoftHsmFile::generate(&mut OsRng, password);

    // Serialize to bytes
    let hsm_bytes = hsm_file.to_bytes();

    // Write to tempfile
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let hsm_path = temp_dir.path().join("test.hsm");
    std::fs::write(&hsm_path, &hsm_bytes).expect("Failed to write HSM file");

    // Read back and deserialize
    let hsm_bytes_read = std::fs::read(&hsm_path).expect("Should read HSM file");
    let hsm_file_recovered =
        SoftHsmFile::from_bytes(&hsm_bytes_read).expect("Should deserialize HSM file");

    // Unlock with correct password
    let sk = hsm_file_recovered
        .unlock(password)
        .expect("Should unlock with correct password");

    // Sign a message
    let message = b"message to sign with HSM key";
    let signature = Hybrid::sign(&sk, message);

    // Verify with the HSM's public key
    let pk = &hsm_file_recovered.public_key;
    assert!(
        Hybrid::verify(pk, message, &signature),
        "Signature from HSM key should verify"
    );
}

// ============================================================================
// Failure Mode Tests (4)
// ============================================================================

#[test]
fn failure_tampered_signature_rejected_by_hybrid_verify() {
    // m7-002: Sign a message, tamper with signature, verify fails

    let (pk, sk) = Hybrid::keygen(&mut OsRng);
    let message = b"test message";

    // Sign
    let mut signature = Hybrid::sign(&sk, message);

    // Tamper with Ed25519 half
    signature.ed25519.0[0] ^= 0xFF;

    // Verify should fail
    assert!(
        !Hybrid::verify(&pk, message, &signature),
        "Tampered Ed25519 half should fail verification"
    );

    // Repair Ed25519 half, tamper with ML-DSA half
    signature.ed25519.0[0] ^= 0xFF;
    if !signature.mldsa.0.is_empty() {
        signature.mldsa.0[0] ^= 0xFF;
        assert!(
            !Hybrid::verify(&pk, message, &signature),
            "Tampered ML-DSA half should fail verification"
        );
    }
}

#[test]
fn failure_wrong_public_key_fails_verify() {
    // m7-002: Sign with key A, verify with key B → false

    let (pk_a, sk_a) = Hybrid::keygen(&mut OsRng);
    let (pk_b, _sk_b) = Hybrid::keygen(&mut OsRng);

    let message = b"test message";
    let signature = Hybrid::sign(&sk_a, message);

    // Verify with wrong key should fail
    assert!(
        !Hybrid::verify(&pk_b, message, &signature),
        "Signature should not verify with wrong public key"
    );

    // Verify with correct key should succeed
    assert!(
        Hybrid::verify(&pk_a, message, &signature),
        "Signature should verify with correct public key"
    );
}

#[test]
fn failure_scope_check_q0901_when_pax_demands_more() {
    // m7-004: PAX requires effects {100, 101, 102, 103}.
    // Key scope only authorizes {100, 101, 102}.
    // Scope-check should fail with Q0901.

    let pax_effects = vec![100u32, 101, 102, 103];
    let pax_bytes = pq_corpus::build_pax_with_effects(&pax_effects);

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let pax_path = temp_dir.path().join("test_scope_fail.pax");
    std::fs::write(&pax_path, &pax_bytes).expect("Failed to write PAX");

    // Parse effects section
    let pax_bytes = std::fs::read(&pax_path).expect("Should read PAX");
    let header =
        paideia_as_emitter_pax::PaxHeader::from_bytes(&pax_bytes).expect("Should parse header");
    let section_table_offset = header.section_table_offset as usize;
    let sections = paideia_as_emitter_pax::SectionTable::from_bytes(
        &pax_bytes[section_table_offset..],
        header.section_count,
    )
    .expect("Should parse section table");

    // Find and parse effects section
    let mut effects_section = paideia_as_emitter_pax::EffectsSection::new();
    for section in &sections.sections {
        if section.ty == paideia_as_emitter_pax::SectionType::Effects {
            let start = section.content_offset as usize;
            let size = section.content_size as usize;
            if start + size <= pax_bytes.len()
                && let Some(effects) = paideia_as_emitter_pax::EffectsSection::from_bytes(
                    &pax_bytes[start..start + size],
                )
            {
                effects_section = effects;
            }
        }
    }

    // Create key scope that does NOT subsume all PAX effects
    let mut key_scope = KeyScope::new();
    key_scope.add(100);
    key_scope.add(101);
    key_scope.add(102);
    // Missing 103

    // Scope-check should fail
    let mut diags = Vec::new();
    assert!(
        !check_delegation_scope(&key_scope, &effects_section, &mut diags),
        "Scope-check should fail when key scope insufficient"
    );

    // Should emit Q0901 diagnostic
    assert_eq!(diags.len(), 1, "Should emit one diagnostic");
    let code = diags[0].code();
    assert_eq!(code.number(), 901, "Should emit Q0901 (scope insufficient)");
}

#[test]
fn failure_soft_hsm_wrong_password_returns_none() {
    // m7-006: Generate HSM with password A, try to unlock with password B → None

    let password_a = b"correct_password";
    let password_b = b"wrong_password";

    let hsm_file = SoftHsmFile::generate(&mut OsRng, password_a);
    let hsm_bytes = hsm_file.to_bytes();

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let hsm_path = temp_dir.path().join("test_wrong_pwd.hsm");
    std::fs::write(&hsm_path, &hsm_bytes).expect("Failed to write HSM file");

    // Read back
    let hsm_bytes_read = std::fs::read(&hsm_path).expect("Should read HSM file");
    let hsm_file_recovered =
        SoftHsmFile::from_bytes(&hsm_bytes_read).expect("Should deserialize HSM file");

    // Try to unlock with wrong password
    let unlock_result = hsm_file_recovered.unlock(password_b);
    assert!(
        unlock_result.is_none(),
        "Unlock with wrong password should return None"
    );

    // Unlock with correct password should succeed
    let sk = hsm_file_recovered
        .unlock(password_a)
        .expect("Should unlock with correct password");
    assert!(!sk.ed25519.0.is_empty() || !sk.mldsa.0.is_empty());
}
