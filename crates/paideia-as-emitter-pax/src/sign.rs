//! PAX signature integration.
//!
//! This module wires the canonical content hash (m4-007) with post-quantum
//! signatures (m7-002). The approach stores a BLAKE3 hash of the signature
//! in the 32-byte PAX header slot; the actual signature lives in a separate
//! `.paideia.sig` section due to size constraints (3373 bytes vs 32).

use crate::header::PaxHeader;
use crate::section::SectionTable;
use blake3;

/// Compute the digest that the signer signs.
///
/// This is the BLAKE3 content hash from m4-007 — the same one stored in
/// the header's blake3_content_hash slot. The signer signs THAT.
pub fn pax_message_to_sign(
    header: &PaxHeader,
    table: &SectionTable,
    section_contents: &[&[u8]],
) -> [u8; 32] {
    crate::hash::compute_content_hash(header, table, section_contents)
}

/// Write the BLAKE3 hash of the hybrid signature into the header's
/// pq_signature_placeholder slot. The actual signature is too big
/// (3373B) to fit; it lives in a separate .paideia.sig section.
pub fn embed_signature_hash(header: &mut PaxHeader, signature_bytes: &[u8]) {
    let hash = blake3::hash(signature_bytes);
    header
        .pq_signature_placeholder
        .copy_from_slice(hash.as_bytes());
}

/// Verify that the header's signature-hash slot matches the supplied
/// signature bytes. Returns false if the slot wasn't populated or the
/// hash doesn't match.
pub fn header_signature_hash_matches(header: &PaxHeader, signature_bytes: &[u8]) -> bool {
    let hash = blake3::hash(signature_bytes);
    header.pq_signature_placeholder == *hash.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::Architecture;

    #[test]
    fn pax_message_to_sign_matches_content_hash() {
        let header = PaxHeader::new(Architecture::X86_64);
        let table = SectionTable::new();
        let contents: [&[u8]; 0] = [];

        let msg = pax_message_to_sign(&header, &table, &contents);
        let direct = crate::hash::compute_content_hash(&header, &table, &contents);

        assert_eq!(
            msg, direct,
            "pax_message_to_sign should match compute_content_hash"
        );
    }

    #[test]
    fn embed_signature_hash_populates_header_slot() {
        let mut header = PaxHeader::new(Architecture::X86_64);
        let sig_blob = [0u8; 3373]; // Mock 3373-byte signature

        embed_signature_hash(&mut header, &sig_blob);

        // Verify that the slot is no longer all zeros
        assert_ne!(
            header.pq_signature_placeholder, [0u8; 32],
            "Signature hash should be populated"
        );

        // Verify it matches BLAKE3(sig_blob)
        let expected_hash = blake3::hash(&sig_blob);
        assert_eq!(
            header.pq_signature_placeholder,
            *expected_hash.as_bytes(),
            "Embedded hash should match BLAKE3(sig_blob)"
        );
    }

    #[test]
    fn header_signature_hash_matches_returns_true_on_correct_sig() {
        let mut header = PaxHeader::new(Architecture::X86_64);
        let sig_blob = [0u8; 3373];

        embed_signature_hash(&mut header, &sig_blob);
        assert!(
            header_signature_hash_matches(&header, &sig_blob),
            "Matching signature should return true"
        );
    }

    #[test]
    fn header_signature_hash_matches_returns_false_on_tampered_sig() {
        let mut header = PaxHeader::new(Architecture::X86_64);
        let sig_a = [0u8; 3373];
        let sig_b = [0xFF; 3373];

        embed_signature_hash(&mut header, &sig_a);

        // Verify against a different signature
        assert!(
            !header_signature_hash_matches(&header, &sig_b),
            "Tampered signature should return false"
        );
    }
}
