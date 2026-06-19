//! Soft-HSM: file-based key storage for development.
//!
//! DEVELOPMENT-ONLY. Production signing uses a hardware HSM via a
//! separate implementation. The soft-HSM stores hybrid keypairs
//! encrypted at rest with Argon2id KDF + ChaCha20-Poly1305.
//!
//! File format (versioned for future migrations):
//!
//! ```text
//! magic       [u8; 8]   = b"PDX-HSM\0"
//! version     u8        = 1
//! kdf         u8        = 1 (Argon2id)
//! cipher      u8        = 1 (ChaCha20-Poly1305)
//! _reserved   u8        = 0
//! kdf_salt    [u8; 16]
//! nonce       [u8; 12]   // ChaCha20-Poly1305 nonce
//! ciphertext  Vec<u8>    // encrypted HybridSecretKey + auth tag
//! hpk         [u8; 1984] // unencrypted hybrid public key
//! ```

use crate::Signer;
use crate::hybrid::{HYBRID_PK_LEN, HybridPublicKey, HybridSecretKey};
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use rand_core::{CryptoRng, RngCore};
use std::io;
use std::path::Path;

/// Soft-HSM file magic bytes: "PDX-HSM\0"
pub const SOFT_HSM_MAGIC: [u8; 8] = *b"PDX-HSM\0";
/// Soft-HSM file format version
pub const SOFT_HSM_VERSION: u8 = 1;
/// KDF type: Argon2id
pub const KDF_ARGON2ID: u8 = 1;
/// Cipher type: ChaCha20-Poly1305
pub const CIPHER_CHACHA20_POLY1305: u8 = 1;
/// KDF salt length (bytes)
pub const KDF_SALT_LEN: usize = 16;
/// ChaCha20-Poly1305 nonce length (bytes)
pub const NONCE_LEN: usize = 12;
/// ChaCha20-Poly1305 authentication tag length (bytes)
pub const AUTH_TAG_LEN: usize = 16;

/// Soft-HSM file structure containing an encrypted secret key and unencrypted public key.
pub struct SoftHsmFile {
    /// Unencrypted hybrid public key.
    pub public_key: HybridPublicKey,
    /// Encrypted secret key (ciphertext + 16-byte auth tag).
    encrypted_secret: Vec<u8>,
    /// KDF salt for Argon2id.
    kdf_salt: [u8; KDF_SALT_LEN],
    /// Nonce for ChaCha20-Poly1305.
    nonce: [u8; NONCE_LEN],
}

impl SoftHsmFile {
    /// Generate a new keypair and encrypt the secret key with the supplied password.
    pub fn generate<R: RngCore + CryptoRng>(rng: &mut R, password: &[u8]) -> Self {
        let (public_key, secret_key) = crate::hybrid::Hybrid::keygen(rng);

        let mut kdf_salt = [0u8; KDF_SALT_LEN];
        rng.fill_bytes(&mut kdf_salt);

        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill_bytes(&mut nonce_bytes);

        let encryption_key = Self::derive_key(password, &kdf_salt);
        let secret_bytes = secret_key.to_bytes();

        let cipher =
            ChaCha20Poly1305::new_from_slice(&encryption_key).expect("key is correct length");

        let nonce_obj = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce_obj, Payload::from(secret_bytes.as_slice()))
            .expect("encryption failed");

        SoftHsmFile {
            public_key,
            encrypted_secret: ciphertext,
            kdf_salt,
            nonce: nonce_bytes,
        }
    }

    /// Derive the encryption key from password + salt via Argon2id.
    fn derive_key(password: &[u8], salt: &[u8; KDF_SALT_LEN]) -> [u8; 32] {
        // Use conservative Argon2id parameters suitable for development.
        // These are not performance-optimized; production would adjust per threat model.
        use argon2::Argon2;

        let mut output = [0u8; 32];

        // Use hash_password_into for direct output hashing
        // Argon2::default() uses Argon2id with reasonable defaults
        let argon2 = Argon2::default();

        argon2
            .hash_password_into(password, salt, &mut output)
            .expect("Argon2id key derivation failed");

        output
    }

    /// Decrypt the stored secret key with the supplied password.
    /// Returns None on wrong password or corrupted file.
    pub fn unlock(&self, password: &[u8]) -> Option<HybridSecretKey> {
        let encryption_key = Self::derive_key(password, &self.kdf_salt);
        let cipher =
            ChaCha20Poly1305::new_from_slice(&encryption_key).expect("key is correct length");

        let nonce_obj = Nonce::from_slice(&self.nonce);

        match cipher.decrypt(nonce_obj, Payload::from(self.encrypted_secret.as_slice())) {
            Ok(secret_bytes) => HybridSecretKey::from_bytes(&secret_bytes),
            Err(_) => None,
        }
    }

    /// Serialize to bytes for on-disk storage.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Header
        buf.extend_from_slice(&SOFT_HSM_MAGIC);
        buf.push(SOFT_HSM_VERSION);
        buf.push(KDF_ARGON2ID);
        buf.push(CIPHER_CHACHA20_POLY1305);
        buf.push(0u8); // reserved

        // KDF salt
        buf.extend_from_slice(&self.kdf_salt);

        // Nonce
        buf.extend_from_slice(&self.nonce);

        // Encrypted secret (variable length, prefixed with u32 length)
        buf.extend_from_slice(&(self.encrypted_secret.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.encrypted_secret);

        // Public key (fixed size)
        buf.extend_from_slice(&self.public_key.to_bytes());

        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len()
            < 8 + 1 + 1 + 1 + 1 + KDF_SALT_LEN + NONCE_LEN + 4 + AUTH_TAG_LEN + HYBRID_PK_LEN
        {
            return None;
        }

        let mut offset = 0;

        // Check magic
        if bytes[offset..offset + 8] != SOFT_HSM_MAGIC {
            return None;
        }
        offset += 8;

        // Check version
        if bytes[offset] != SOFT_HSM_VERSION {
            return None;
        }
        offset += 1;

        // Check KDF type
        if bytes[offset] != KDF_ARGON2ID {
            return None;
        }
        offset += 1;

        // Check cipher type
        if bytes[offset] != CIPHER_CHACHA20_POLY1305 {
            return None;
        }
        offset += 1;

        // Reserved byte
        offset += 1;

        // Extract KDF salt
        let mut kdf_salt = [0u8; KDF_SALT_LEN];
        kdf_salt.copy_from_slice(&bytes[offset..offset + KDF_SALT_LEN]);
        offset += KDF_SALT_LEN;

        // Extract nonce
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&bytes[offset..offset + NONCE_LEN]);
        offset += NONCE_LEN;

        // Extract encrypted secret length
        let ciphertext_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        // Extract encrypted secret
        if offset + ciphertext_len > bytes.len() - HYBRID_PK_LEN {
            return None;
        }
        let encrypted_secret = bytes[offset..offset + ciphertext_len].to_vec();
        offset += ciphertext_len;

        // Extract public key
        let public_key = HybridPublicKey::from_bytes(&bytes[offset..offset + HYBRID_PK_LEN])?;

        Some(SoftHsmFile {
            public_key,
            encrypted_secret,
            kdf_salt,
            nonce,
        })
    }

    /// Save to disk at the given path.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.to_bytes())
    }

    /// Load from disk.
    pub fn load(path: &Path) -> io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Invalid HSM file format"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HYBRID_SK_LEN;
    use rand_core::OsRng;
    use tempfile::TempDir;

    #[test]
    fn generate_produces_distinct_keypair() {
        let hsm1 = SoftHsmFile::generate(&mut OsRng, b"password1");
        let hsm2 = SoftHsmFile::generate(&mut OsRng, b"password2");

        assert_ne!(
            hsm1.public_key.to_bytes(),
            hsm2.public_key.to_bytes(),
            "Generated public keys should be distinct"
        );
    }

    #[test]
    fn unlock_with_correct_password_returns_secret_key() {
        let password = b"correct_password";
        let hsm = SoftHsmFile::generate(&mut OsRng, password);

        let secret_key = hsm
            .unlock(password)
            .expect("Should unlock with correct password");

        assert_eq!(
            secret_key.to_bytes().len(),
            HYBRID_SK_LEN,
            "Secret key should have correct length"
        );
    }

    #[test]
    fn unlock_with_wrong_password_returns_none() {
        let hsm = SoftHsmFile::generate(&mut OsRng, b"correct_password");

        let result = hsm.unlock(b"wrong_password");
        assert!(result.is_none(), "Wrong password should return None");
    }

    #[test]
    fn roundtrip_to_bytes_and_from_bytes() {
        let password = b"test_password";
        let hsm_original = SoftHsmFile::generate(&mut OsRng, password);

        let bytes = hsm_original.to_bytes();
        let hsm_recovered = SoftHsmFile::from_bytes(&bytes).expect("Should deserialize correctly");

        // Public keys should match
        assert_eq!(
            hsm_original.public_key.to_bytes(),
            hsm_recovered.public_key.to_bytes(),
            "Public keys should match after roundtrip"
        );

        // Should still be able to decrypt with same password
        let secret_key = hsm_recovered
            .unlock(password)
            .expect("Should still decrypt after roundtrip");
        assert_eq!(
            secret_key.to_bytes().len(),
            HYBRID_SK_LEN,
            "Decrypted secret key should have correct length"
        );
    }

    #[test]
    fn roundtrip_save_and_load() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let hsm_path = temp_dir.path().join("test.hsm");
        let password = b"test_password";

        let hsm_original = SoftHsmFile::generate(&mut OsRng, password);
        hsm_original.save(&hsm_path).expect("Should save to disk");

        assert!(hsm_path.exists(), "HSM file should exist after save");

        let hsm_loaded = SoftHsmFile::load(&hsm_path).expect("Should load from disk");

        assert_eq!(
            hsm_original.public_key.to_bytes(),
            hsm_loaded.public_key.to_bytes(),
            "Public keys should match after save/load"
        );

        let secret_key = hsm_loaded
            .unlock(password)
            .expect("Should still decrypt after load");
        assert_eq!(
            secret_key.to_bytes().len(),
            HYBRID_SK_LEN,
            "Decrypted secret key should have correct length"
        );
    }

    #[test]
    fn from_bytes_rejects_bad_magic() {
        let bad_bytes = vec![0u8; 100];
        let result = SoftHsmFile::from_bytes(&bad_bytes);
        assert!(result.is_none(), "Should reject bytes with bad magic");
    }
}
