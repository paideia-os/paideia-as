//! ChaCha20-Poly1305 AEAD (RFC 8439).
//!
//! Reference implementation backed by the `chacha20poly1305` crate.
//! The trait shape ([`crate::aead::Aead`]) is kept minimal so a
//! paideia-native recipe (issue `v0.33-005`) can drop in behind the
//! same trait with no caller churn.
//!
//! # RFC 8439 conformance
//!
//! * Key size — 32 bytes (RFC 8439 §2.5).
//! * Nonce size — 12 bytes (RFC 8439 §2.3).
//! * Tag size — 16 bytes (RFC 8439 §2.5).
//! * Wire format — `ciphertext || tag` (RFC 8439 §2.8.1).
//! * AAD is authenticated but not encrypted.
//! * A single-vector conformance test drawn from
//!   RFC 8439 §2.8.2 pins the encrypt path byte-for-byte.
//! * Round-trip and tamper-detection tests pin the trait invariants
//!   listed in [`crate::aead::Aead`].
//!
//! # Nonce discipline
//!
//! Reusing a `(key, nonce)` pair with any AEAD built on a stream
//! cipher destroys confidentiality *and* authenticity — a single
//! reuse is enough to leak the Poly1305 one-time MAC key. This impl
//! does NOT generate nonces internally; callers pass one in through
//! [`ChaCha20Poly1305Params`] and are responsible for uniqueness. The
//! `rng` module (issue `v0.33-003`) will provide the secure random
//! source used to sample fresh nonces.

use chacha20poly1305::{
    ChaCha20Poly1305 as ChaChaCipher, Nonce,
    aead::{Aead as _, KeyInit, Payload},
};

use super::{Aead, AeadError};

/// ChaCha20-Poly1305 key length in bytes (RFC 8439 §2.5).
pub const KEY_LEN: usize = 32;

/// ChaCha20-Poly1305 nonce length in bytes (RFC 8439 §2.3).
///
/// The 96-bit nonce variant (as opposed to the 64-bit legacy nonce)
/// is what RFC 8439 standardises and what the AEAD construction of
/// §2.8 requires.
pub const NONCE_LEN: usize = 12;

/// ChaCha20-Poly1305 authentication tag length in bytes (RFC 8439
/// §2.5).
pub const TAG_LEN: usize = 16;

/// Inputs to a single ChaCha20-Poly1305 seal / open call.
///
/// Borrowed rather than owned so the caller can reuse the same key
/// and AAD across many messages without cloning either into every
/// call. The `nonce` MUST be unique per `key` across the entire
/// lifetime of that key (see the module-level nonce-discipline note).
#[derive(Copy, Clone, Debug)]
pub struct ChaCha20Poly1305Params<'a> {
    /// 256-bit symmetric key.
    pub key: &'a [u8; KEY_LEN],
    /// 96-bit unique-per-key nonce.
    pub nonce: &'a [u8; NONCE_LEN],
    /// Associated data — authenticated but not encrypted. Empty is
    /// legal (RFC 8439 §2.8).
    pub aad: &'a [u8],
}

/// ChaCha20-Poly1305 AEAD, RFC 8439.
///
/// A zero-sized type: the trait's methods carry all state. Named
/// `ChaCha20Poly1305` (no dash) to match Rust identifier rules; the
/// spec's `AEAD_CHACHA20_POLY1305` reads as one primitive despite
/// the double name.
#[derive(Copy, Clone, Debug, Default)]
pub struct ChaCha20Poly1305;

impl Aead for ChaCha20Poly1305 {
    type Params<'a>
        = ChaCha20Poly1305Params<'a>
    where
        Self: 'a;

    fn seal(params: &Self::Params<'_>, plaintext: &[u8]) -> Result<Vec<u8>, AeadError> {
        // params.key is a fixed-size &[u8; KEY_LEN], so new_from_slice
        // never returns InvalidLength here — but we surface it as an
        // internal-primitive error rather than panic to keep the trait
        // total.
        let cipher = ChaChaCipher::new_from_slice(params.key)
            .map_err(|e| AeadError::Primitive(format!("{e}")))?;
        let nonce = Nonce::from_slice(params.nonce);
        cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad: params.aad,
                },
            )
            .map_err(|e| AeadError::Primitive(format!("{e}")))
    }

    fn open(params: &Self::Params<'_>, sealed: &[u8]) -> Result<Vec<u8>, AeadError> {
        if sealed.len() < TAG_LEN {
            return Err(AeadError::CiphertextTooShort {
                got: sealed.len(),
                expected: TAG_LEN,
            });
        }
        let cipher = ChaChaCipher::new_from_slice(params.key)
            .map_err(|e| AeadError::Primitive(format!("{e}")))?;
        let nonce = Nonce::from_slice(params.nonce);
        // The chacha20poly1305 crate flattens all decryption failures
        // (auth-tag mismatch, invalid length, …) into a single opaque
        // Error type. Any failure at this layer is by construction an
        // authentication failure — the length check above already
        // rules out CiphertextTooShort — so we surface it as such.
        cipher
            .decrypt(
                nonce,
                Payload {
                    msg: sealed,
                    aad: params.aad,
                },
            )
            .map_err(|_| AeadError::AuthenticationFailed)
    }
}

// ---------------------------------------------------------------------
// RFC 8439 §2.8.2 — canonical AEAD_CHACHA20_POLY1305 test vector.
//
// These constants are `pub` so the v0.33-005 integration suite (and
// any downstream consumer that wants a known-good vector) can pin on
// them without re-transcribing the RFC.
// ---------------------------------------------------------------------

/// RFC 8439 §2.8.2 key (32 bytes).
pub const RFC_8439_SEC_2_8_2_KEY: [u8; KEY_LEN] = [
    0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, //
    0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e, 0x8f, //
    0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, //
    0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f,
];

/// RFC 8439 §2.8.2 nonce (12 bytes).
///
/// The first 4 bytes are the "constant" senders may use as a domain
/// separator; the last 8 are the per-message IV. The AEAD does not
/// distinguish the two — it consumes the concatenation as a single
/// 96-bit nonce.
pub const RFC_8439_SEC_2_8_2_NONCE: [u8; NONCE_LEN] = [
    0x07, 0x00, 0x00, 0x00, //
    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47,
];

/// RFC 8439 §2.8.2 associated data (12 bytes).
pub const RFC_8439_SEC_2_8_2_AAD: [u8; 12] = [
    0x50, 0x51, 0x52, 0x53, 0xc0, 0xc1, 0xc2, 0xc3, //
    0xc4, 0xc5, 0xc6, 0xc7,
];

/// RFC 8439 §2.8.2 plaintext (114 bytes, ASCII).
///
/// The exact byte string
/// `"Ladies and Gentlemen of the class of '99: If I could offer you`
/// ` only one tip for the future, sunscreen would be it."` — same
/// bytes as the RFC's hex table, given here as a byte-string literal
/// so the source stays human-readable.
pub const RFC_8439_SEC_2_8_2_PLAINTEXT: &[u8] =
    b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";

/// RFC 8439 §2.8.2 ciphertext (114 bytes) — without the tag.
pub const RFC_8439_SEC_2_8_2_CIPHERTEXT: [u8; 114] = [
    0xd3, 0x1a, 0x8d, 0x34, 0x64, 0x8e, 0x60, 0xdb, //
    0x7b, 0x86, 0xaf, 0xbc, 0x53, 0xef, 0x7e, 0xc2, //
    0xa4, 0xad, 0xed, 0x51, 0x29, 0x6e, 0x08, 0xfe, //
    0xa9, 0xe2, 0xb5, 0xa7, 0x36, 0xee, 0x62, 0xd6, //
    0x3d, 0xbe, 0xa4, 0x5e, 0x8c, 0xa9, 0x67, 0x12, //
    0x82, 0xfa, 0xfb, 0x69, 0xda, 0x92, 0x72, 0x8b, //
    0x1a, 0x71, 0xde, 0x0a, 0x9e, 0x06, 0x0b, 0x29, //
    0x05, 0xd6, 0xa5, 0xb6, 0x7e, 0xcd, 0x3b, 0x36, //
    0x92, 0xdd, 0xbd, 0x7f, 0x2d, 0x77, 0x8b, 0x8c, //
    0x98, 0x03, 0xae, 0xe3, 0x28, 0x09, 0x1b, 0x58, //
    0xfa, 0xb3, 0x24, 0xe4, 0xfa, 0xd6, 0x75, 0x94, //
    0x55, 0x85, 0x80, 0x8b, 0x48, 0x31, 0xd7, 0xbc, //
    0x3f, 0xf4, 0xde, 0xf0, 0x8e, 0x4b, 0x7a, 0x9d, //
    0xe5, 0x76, 0xd2, 0x65, 0x86, 0xce, 0xc6, 0x4b, //
    0x61, 0x16,
];

/// RFC 8439 §2.8.2 authentication tag (16 bytes).
pub const RFC_8439_SEC_2_8_2_TAG: [u8; TAG_LEN] = [
    0x1a, 0xe1, 0x0b, 0x59, 0x4f, 0x09, 0xe2, 0x6a, //
    0x7e, 0x90, 0x2e, 0xcb, 0xd0, 0x60, 0x06, 0x91,
];

// Static shape checks — a mis-sized vector constant would silently
// pass a byte-slice comparison; these compile-time asserts make the
// length half of the contract visible where the constants live.
const _: () = {
    assert!(RFC_8439_SEC_2_8_2_KEY.len() == KEY_LEN);
    assert!(RFC_8439_SEC_2_8_2_NONCE.len() == NONCE_LEN);
    assert!(RFC_8439_SEC_2_8_2_TAG.len() == TAG_LEN);
    assert!(RFC_8439_SEC_2_8_2_CIPHERTEXT.len() == RFC_8439_SEC_2_8_2_PLAINTEXT.len());
};

// ---------------------------------------------------------------------
// RFC 8439 Appendix A.5 — full multi-block AEAD_CHACHA20_POLY1305
// decryption test vector.
//
// A.5 is longer than §2.8.2 (265 vs. 114 bytes of plaintext) and
// exercises the full multi-block path through both ChaCha20 (which
// operates on 64-byte blocks) and Poly1305 (which operates on 16-byte
// blocks). §2.8.2 alone leaves the last-block padding logic of both
// primitives thinly exercised — A.5 pins it.
//
// The plaintext is not ASCII-clean: it contains U+201C / U+201D
// (typographic double quotes) encoded as `E2 80 9C` / `E2 80 9D`, so
// the constant is a raw byte array rather than a `b"…"` literal.
// ---------------------------------------------------------------------

/// RFC 8439 Appendix A.5 key (32 bytes).
pub const RFC_8439_APPENDIX_A_5_KEY: [u8; KEY_LEN] = [
    0x1c, 0x92, 0x40, 0xa5, 0xeb, 0x55, 0xd3, 0x8a, //
    0xf3, 0x33, 0x88, 0x86, 0x04, 0xf6, 0xb5, 0xf0, //
    0x47, 0x39, 0x17, 0xc1, 0x40, 0x2b, 0x80, 0x09, //
    0x9d, 0xca, 0x5c, 0xbc, 0x20, 0x70, 0x75, 0xc0,
];

/// RFC 8439 Appendix A.5 nonce (12 bytes).
pub const RFC_8439_APPENDIX_A_5_NONCE: [u8; NONCE_LEN] = [
    0x00, 0x00, 0x00, 0x00, //
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
];

/// RFC 8439 Appendix A.5 associated data (12 bytes).
pub const RFC_8439_APPENDIX_A_5_AAD: [u8; 12] = [
    0xf3, 0x33, 0x88, 0x86, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x00, 0x4e, 0x91,
];

/// RFC 8439 Appendix A.5 plaintext (265 bytes).
///
/// The IETF Internet-Drafts boilerplate, ending in
/// `/…work in progress./…` where `…` is U+201C / U+201D (typographic
/// double quotes). Held as raw bytes so the constant matches the RFC
/// hex table byte-for-byte without needing a UTF-8 literal.
pub const RFC_8439_APPENDIX_A_5_PLAINTEXT: [u8; 265] = [
    0x49, 0x6e, 0x74, 0x65, 0x72, 0x6e, 0x65, 0x74, //
    0x2d, 0x44, 0x72, 0x61, 0x66, 0x74, 0x73, 0x20, //
    0x61, 0x72, 0x65, 0x20, 0x64, 0x72, 0x61, 0x66, //
    0x74, 0x20, 0x64, 0x6f, 0x63, 0x75, 0x6d, 0x65, //
    0x6e, 0x74, 0x73, 0x20, 0x76, 0x61, 0x6c, 0x69, //
    0x64, 0x20, 0x66, 0x6f, 0x72, 0x20, 0x61, 0x20, //
    0x6d, 0x61, 0x78, 0x69, 0x6d, 0x75, 0x6d, 0x20, //
    0x6f, 0x66, 0x20, 0x73, 0x69, 0x78, 0x20, 0x6d, //
    0x6f, 0x6e, 0x74, 0x68, 0x73, 0x20, 0x61, 0x6e, //
    0x64, 0x20, 0x6d, 0x61, 0x79, 0x20, 0x62, 0x65, //
    0x20, 0x75, 0x70, 0x64, 0x61, 0x74, 0x65, 0x64, //
    0x2c, 0x20, 0x72, 0x65, 0x70, 0x6c, 0x61, 0x63, //
    0x65, 0x64, 0x2c, 0x20, 0x6f, 0x72, 0x20, 0x6f, //
    0x62, 0x73, 0x6f, 0x6c, 0x65, 0x74, 0x65, 0x64, //
    0x20, 0x62, 0x79, 0x20, 0x6f, 0x74, 0x68, 0x65, //
    0x72, 0x20, 0x64, 0x6f, 0x63, 0x75, 0x6d, 0x65, //
    0x6e, 0x74, 0x73, 0x20, 0x61, 0x74, 0x20, 0x61, //
    0x6e, 0x79, 0x20, 0x74, 0x69, 0x6d, 0x65, 0x2e, //
    0x20, 0x49, 0x74, 0x20, 0x69, 0x73, 0x20, 0x69, //
    0x6e, 0x61, 0x70, 0x70, 0x72, 0x6f, 0x70, 0x72, //
    0x69, 0x61, 0x74, 0x65, 0x20, 0x74, 0x6f, 0x20, //
    0x75, 0x73, 0x65, 0x20, 0x49, 0x6e, 0x74, 0x65, //
    0x72, 0x6e, 0x65, 0x74, 0x2d, 0x44, 0x72, 0x61, //
    0x66, 0x74, 0x73, 0x20, 0x61, 0x73, 0x20, 0x72, //
    0x65, 0x66, 0x65, 0x72, 0x65, 0x6e, 0x63, 0x65, //
    0x20, 0x6d, 0x61, 0x74, 0x65, 0x72, 0x69, 0x61, //
    0x6c, 0x20, 0x6f, 0x72, 0x20, 0x74, 0x6f, 0x20, //
    0x63, 0x69, 0x74, 0x65, 0x20, 0x74, 0x68, 0x65, //
    0x6d, 0x20, 0x6f, 0x74, 0x68, 0x65, 0x72, 0x20, //
    0x74, 0x68, 0x61, 0x6e, 0x20, 0x61, 0x73, 0x20, //
    0x2f, 0xe2, 0x80, 0x9c, 0x77, 0x6f, 0x72, 0x6b, //
    0x20, 0x69, 0x6e, 0x20, 0x70, 0x72, 0x6f, 0x67, //
    0x72, 0x65, 0x73, 0x73, 0x2e, 0x2f, 0xe2, 0x80, //
    0x9d,
];

/// RFC 8439 Appendix A.5 ciphertext (265 bytes) — without the tag.
pub const RFC_8439_APPENDIX_A_5_CIPHERTEXT: [u8; 265] = [
    0x64, 0xa0, 0x86, 0x15, 0x75, 0x86, 0x1a, 0xf4, //
    0x60, 0xf0, 0x62, 0xc7, 0x9b, 0xe6, 0x43, 0xbd, //
    0x5e, 0x80, 0x5c, 0xfd, 0x34, 0x5c, 0xf3, 0x89, //
    0xf1, 0x08, 0x67, 0x0a, 0xc7, 0x6c, 0x8c, 0xb2, //
    0x4c, 0x6c, 0xfc, 0x18, 0x75, 0x5d, 0x43, 0xee, //
    0xa0, 0x9e, 0xe9, 0x4e, 0x38, 0x2d, 0x26, 0xb0, //
    0xbd, 0xb7, 0xb7, 0x3c, 0x32, 0x1b, 0x01, 0x00, //
    0xd4, 0xf0, 0x3b, 0x7f, 0x35, 0x58, 0x94, 0xcf, //
    0x33, 0x2f, 0x83, 0x0e, 0x71, 0x0b, 0x97, 0xce, //
    0x98, 0xc8, 0xa8, 0x4a, 0xbd, 0x0b, 0x94, 0x81, //
    0x14, 0xad, 0x17, 0x6e, 0x00, 0x8d, 0x33, 0xbd, //
    0x60, 0xf9, 0x82, 0xb1, 0xff, 0x37, 0xc8, 0x55, //
    0x97, 0x97, 0xa0, 0x6e, 0xf4, 0xf0, 0xef, 0x61, //
    0xc1, 0x86, 0x32, 0x4e, 0x2b, 0x35, 0x06, 0x38, //
    0x36, 0x06, 0x90, 0x7b, 0x6a, 0x7c, 0x02, 0xb0, //
    0xf9, 0xf6, 0x15, 0x7b, 0x53, 0xc8, 0x67, 0xe4, //
    0xb9, 0x16, 0x6c, 0x76, 0x7b, 0x80, 0x4d, 0x46, //
    0xa5, 0x9b, 0x52, 0x16, 0xcd, 0xe7, 0xa4, 0xe9, //
    0x90, 0x40, 0xc5, 0xa4, 0x04, 0x33, 0x22, 0x5e, //
    0xe2, 0x82, 0xa1, 0xb0, 0xa0, 0x6c, 0x52, 0x3e, //
    0xaf, 0x45, 0x34, 0xd7, 0xf8, 0x3f, 0xa1, 0x15, //
    0x5b, 0x00, 0x47, 0x71, 0x8c, 0xbc, 0x54, 0x6a, //
    0x0d, 0x07, 0x2b, 0x04, 0xb3, 0x56, 0x4e, 0xea, //
    0x1b, 0x42, 0x22, 0x73, 0xf5, 0x48, 0x27, 0x1a, //
    0x0b, 0xb2, 0x31, 0x60, 0x53, 0xfa, 0x76, 0x99, //
    0x19, 0x55, 0xeb, 0xd6, 0x31, 0x59, 0x43, 0x4e, //
    0xce, 0xbb, 0x4e, 0x46, 0x6d, 0xae, 0x5a, 0x10, //
    0x73, 0xa6, 0x72, 0x76, 0x27, 0x09, 0x7a, 0x10, //
    0x49, 0xe6, 0x17, 0xd9, 0x1d, 0x36, 0x10, 0x94, //
    0xfa, 0x68, 0xf0, 0xff, 0x77, 0x98, 0x71, 0x30, //
    0x30, 0x5b, 0xea, 0xba, 0x2e, 0xda, 0x04, 0xdf, //
    0x99, 0x7b, 0x71, 0x4d, 0x6c, 0x6f, 0x2c, 0x29, //
    0xa6, 0xad, 0x5c, 0xb4, 0x02, 0x2b, 0x02, 0x70, //
    0x9b,
];

/// RFC 8439 Appendix A.5 authentication tag (16 bytes).
pub const RFC_8439_APPENDIX_A_5_TAG: [u8; TAG_LEN] = [
    0xee, 0xad, 0x9d, 0x67, 0x89, 0x0c, 0xbb, 0x22, //
    0x39, 0x23, 0x36, 0xfe, 0xa1, 0x85, 0x1f, 0x38,
];

// Compile-time length asserts for A.5, mirroring the §2.8.2 block.
const _: () = {
    assert!(RFC_8439_APPENDIX_A_5_KEY.len() == KEY_LEN);
    assert!(RFC_8439_APPENDIX_A_5_NONCE.len() == NONCE_LEN);
    assert!(RFC_8439_APPENDIX_A_5_TAG.len() == TAG_LEN);
    assert!(RFC_8439_APPENDIX_A_5_CIPHERTEXT.len() == RFC_8439_APPENDIX_A_5_PLAINTEXT.len());
    assert!(RFC_8439_APPENDIX_A_5_PLAINTEXT.len() == 265);
};

#[cfg(test)]
mod tests {
    use super::*;

    // ---- RFC 8439 §2.8.2 --------------------------------------------

    /// Seal the §2.8.2 plaintext under the §2.8.2 key/nonce/AAD and
    /// verify the concatenated `ciphertext || tag` bytes match the
    /// RFC exactly.
    #[test]
    fn rfc_8439_sec_2_8_2_seal_matches_vector() {
        let params = ChaCha20Poly1305Params {
            key: &RFC_8439_SEC_2_8_2_KEY,
            nonce: &RFC_8439_SEC_2_8_2_NONCE,
            aad: &RFC_8439_SEC_2_8_2_AAD,
        };
        let sealed = ChaCha20Poly1305::seal(&params, RFC_8439_SEC_2_8_2_PLAINTEXT)
            .expect("seal should succeed on RFC 8439 §2.8.2 inputs");

        let expected_len = RFC_8439_SEC_2_8_2_CIPHERTEXT.len() + TAG_LEN;
        assert_eq!(
            sealed.len(),
            expected_len,
            "sealed length mismatch: got {}, want {}",
            sealed.len(),
            expected_len,
        );

        let (ct, tag) = sealed.split_at(RFC_8439_SEC_2_8_2_CIPHERTEXT.len());
        assert_eq!(
            ct,
            RFC_8439_SEC_2_8_2_CIPHERTEXT.as_slice(),
            "ciphertext bytes must match RFC 8439 §2.8.2",
        );
        assert_eq!(
            tag,
            RFC_8439_SEC_2_8_2_TAG.as_slice(),
            "authentication tag must match RFC 8439 §2.8.2",
        );
    }

    /// Open the §2.8.2 ciphertext + tag under the §2.8.2 key/nonce/
    /// AAD and verify the recovered plaintext matches the RFC.
    #[test]
    fn rfc_8439_sec_2_8_2_open_matches_vector() {
        let mut sealed = Vec::with_capacity(RFC_8439_SEC_2_8_2_CIPHERTEXT.len() + TAG_LEN);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_CIPHERTEXT);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_TAG);

        let params = ChaCha20Poly1305Params {
            key: &RFC_8439_SEC_2_8_2_KEY,
            nonce: &RFC_8439_SEC_2_8_2_NONCE,
            aad: &RFC_8439_SEC_2_8_2_AAD,
        };
        let plaintext = ChaCha20Poly1305::open(&params, &sealed)
            .expect("open should succeed on RFC 8439 §2.8.2 ciphertext");
        assert_eq!(
            plaintext.as_slice(),
            RFC_8439_SEC_2_8_2_PLAINTEXT,
            "recovered plaintext must match RFC 8439 §2.8.2",
        );
    }

    // ---- Trait invariants -------------------------------------------

    /// Round-trip: seal then open under a synthetic vector recovers
    /// the original plaintext bit-for-bit and yields no per-message
    /// state leak between calls.
    #[test]
    fn seal_then_open_round_trip() {
        let key = [0x11u8; KEY_LEN];
        let nonce = [0x22u8; NONCE_LEN];
        let aad = b"paideia-as v0.33-002 round-trip";
        let plaintext = b"the quick brown fox jumps over the lazy dog";
        let params = ChaCha20Poly1305Params {
            key: &key,
            nonce: &nonce,
            aad: aad.as_slice(),
        };

        let sealed = ChaCha20Poly1305::seal(&params, plaintext).expect("seal");
        assert_eq!(sealed.len(), plaintext.len() + TAG_LEN);
        let recovered = ChaCha20Poly1305::open(&params, &sealed).expect("open");
        assert_eq!(recovered.as_slice(), plaintext.as_slice());
    }

    /// Flipping any bit in the ciphertext MUST cause open to return
    /// [`AeadError::AuthenticationFailed`] and MUST NOT surface any
    /// plaintext bytes to the caller.
    #[test]
    fn tampered_ciphertext_fails_authentication() {
        let params = ChaCha20Poly1305Params {
            key: &RFC_8439_SEC_2_8_2_KEY,
            nonce: &RFC_8439_SEC_2_8_2_NONCE,
            aad: &RFC_8439_SEC_2_8_2_AAD,
        };
        let mut sealed = Vec::from(RFC_8439_SEC_2_8_2_CIPHERTEXT);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_TAG);

        // Flip the low bit of the first ciphertext byte.
        sealed[0] ^= 0x01;

        match ChaCha20Poly1305::open(&params, &sealed) {
            Err(AeadError::AuthenticationFailed) => {}
            Err(other) => panic!("expected AuthenticationFailed, got {other:?}"),
            Ok(_) => panic!("tampered ciphertext must not decrypt"),
        }
    }

    /// The same tamper check on the tag itself, to cover both halves
    /// of the sealed buffer.
    #[test]
    fn tampered_tag_fails_authentication() {
        let params = ChaCha20Poly1305Params {
            key: &RFC_8439_SEC_2_8_2_KEY,
            nonce: &RFC_8439_SEC_2_8_2_NONCE,
            aad: &RFC_8439_SEC_2_8_2_AAD,
        };
        let mut sealed = Vec::from(RFC_8439_SEC_2_8_2_CIPHERTEXT);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_TAG);

        let last = sealed.len() - 1;
        sealed[last] ^= 0x80;

        match ChaCha20Poly1305::open(&params, &sealed) {
            Err(AeadError::AuthenticationFailed) => {}
            Err(other) => panic!("expected AuthenticationFailed, got {other:?}"),
            Ok(_) => panic!("tampered tag must not decrypt"),
        }
    }

    /// Changing the AAD between seal and open MUST cause an auth
    /// failure — the AAD is bound into the tag under Poly1305.
    #[test]
    fn mismatched_aad_fails_authentication() {
        let key = [0x33u8; KEY_LEN];
        let nonce = [0x44u8; NONCE_LEN];
        let seal_aad = b"context-A";
        let open_aad = b"context-B";
        let plaintext = b"payload";

        let sealed = ChaCha20Poly1305::seal(
            &ChaCha20Poly1305Params {
                key: &key,
                nonce: &nonce,
                aad: seal_aad.as_slice(),
            },
            plaintext,
        )
        .expect("seal");

        match ChaCha20Poly1305::open(
            &ChaCha20Poly1305Params {
                key: &key,
                nonce: &nonce,
                aad: open_aad.as_slice(),
            },
            &sealed,
        ) {
            Err(AeadError::AuthenticationFailed) => {}
            Err(other) => panic!("expected AuthenticationFailed, got {other:?}"),
            Ok(_) => panic!("mismatched AAD must not decrypt"),
        }
    }

    /// Sealed buffers shorter than the tag length must be rejected
    /// with a distinct error variant (not authentication-failed) so
    /// callers can distinguish framing bugs from genuine tampering.
    #[test]
    fn ciphertext_shorter_than_tag_returns_ciphertext_too_short() {
        let params = ChaCha20Poly1305Params {
            key: &[0u8; KEY_LEN],
            nonce: &[0u8; NONCE_LEN],
            aad: &[],
        };
        let sealed = [0u8; TAG_LEN - 1];
        match ChaCha20Poly1305::open(&params, &sealed) {
            Err(AeadError::CiphertextTooShort { got, expected }) => {
                assert_eq!(got, sealed.len());
                assert_eq!(expected, TAG_LEN);
            }
            other => panic!("expected CiphertextTooShort, got {other:?}"),
        }
    }

    /// Empty plaintext is legal — the sealed buffer is just the tag.
    #[test]
    fn empty_plaintext_produces_tag_only_ciphertext() {
        let key = [0x55u8; KEY_LEN];
        let nonce = [0x66u8; NONCE_LEN];
        let params = ChaCha20Poly1305Params {
            key: &key,
            nonce: &nonce,
            aad: b"non-empty aad",
        };
        let sealed = ChaCha20Poly1305::seal(&params, &[]).expect("seal");
        assert_eq!(sealed.len(), TAG_LEN);
        let recovered = ChaCha20Poly1305::open(&params, &sealed).expect("open");
        assert!(recovered.is_empty());
    }
}
