//! Release-artifact signing.
//!
//! Signs detached signatures over release tarballs and git tags.

use std::path::Path;

use crate::hybrid::{HybridPublicKey, HybridSecretKey, HybridSignature};
use crate::{Hybrid, Signer};

/// Compute the BLAKE3 hash of a file's contents.
pub fn hash_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let data = std::fs::read(path)?;
    Ok(blake3::hash(&data).into())
}

/// Sign a file's content hash, return a detached signature.
pub fn sign_release_artifact(
    sk: &HybridSecretKey,
    artifact_path: &Path,
) -> std::io::Result<HybridSignature> {
    let hash = hash_file(artifact_path)?;
    Ok(Hybrid::sign(sk, &hash))
}

/// Write the detached signature to `<artifact_path>.sig`.
pub fn write_detached_signature(
    artifact_path: &Path,
    sig: &HybridSignature,
) -> std::io::Result<()> {
    let sig_path = artifact_path.with_extension("sig");
    std::fs::write(sig_path, sig.to_bytes())
}

/// Verify a detached signature against a file. Reads `<artifact_path>.sig`
/// and verifies it matches BLAKE3(artifact).
pub fn verify_detached_signature(
    pk: &HybridPublicKey,
    artifact_path: &Path,
) -> std::io::Result<bool> {
    let hash = hash_file(artifact_path)?;
    let sig_path = artifact_path.with_extension("sig");
    let sig_bytes = std::fs::read(sig_path)?;

    let sig = match HybridSignature::from_bytes(&sig_bytes) {
        Some(s) => s,
        None => return Ok(false),
    };

    Ok(Hybrid::verify(pk, &hash, &sig))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn hash_file_returns_blake3_of_content() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("test.bin");

        let content = b"Hello, paideia-as release signing!";
        let mut file = std::fs::File::create(&file_path).expect("Failed to create file");
        file.write_all(content).expect("Failed to write file");

        let hash = hash_file(&file_path).expect("Failed to hash file");
        let expected_hash: [u8; 32] = blake3::hash(content).into();

        assert_eq!(hash, expected_hash, "Hash should match BLAKE3 of content");
    }

    #[test]
    fn sign_and_verify_release_artifact_roundtrip() {
        use crate::Signer;
        use rand_core::OsRng;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let artifact_path = temp_dir.path().join("release-v1.0.0.tar.gz");

        let artifact_content = b"This is a release tarball";
        std::fs::write(&artifact_path, artifact_content).expect("Failed to write artifact");

        let (_pk, sk) = crate::Hybrid::keygen(&mut OsRng);
        let (pk, _sk2) = crate::Hybrid::keygen(&mut OsRng);

        let sig = sign_release_artifact(&sk, &artifact_path).expect("Failed to sign");
        write_detached_signature(&artifact_path, &sig).expect("Failed to write signature");

        let verified = verify_detached_signature(&pk, &artifact_path).expect("Failed to verify");
        assert!(!verified, "Different key should not verify");

        let verified = verify_detached_signature(&_pk, &artifact_path).expect("Failed to verify");
        assert!(verified, "Same key should verify signature");
    }

    #[test]
    fn tampered_artifact_fails_detached_verify() {
        use crate::Signer;
        use rand_core::OsRng;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let artifact_path = temp_dir.path().join("release-v1.0.0.tar.gz");

        let artifact_content = b"This is a release tarball";
        std::fs::write(&artifact_path, artifact_content).expect("Failed to write artifact");

        let (pk, sk) = crate::Hybrid::keygen(&mut OsRng);
        let sig = sign_release_artifact(&sk, &artifact_path).expect("Failed to sign");
        write_detached_signature(&artifact_path, &sig).expect("Failed to write signature");

        let verified = verify_detached_signature(&pk, &artifact_path).expect("Failed to verify");
        assert!(verified, "Original artifact should verify");

        // Tamper with the artifact
        std::fs::write(&artifact_path, b"This is tampering!").expect("Failed to tamper");

        let verified = verify_detached_signature(&pk, &artifact_path).expect("Failed to verify");
        assert!(!verified, "Tampered artifact should fail verification");
    }

    #[test]
    fn write_detached_signature_places_sig_alongside() {
        use crate::Signer;
        use rand_core::OsRng;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let artifact_path = temp_dir.path().join("release-v1.0.0.tar.gz");

        std::fs::write(&artifact_path, b"artifact content").expect("Failed to write artifact");

        let (_pk, sk) = crate::Hybrid::keygen(&mut OsRng);
        let sig = sign_release_artifact(&sk, &artifact_path).expect("Failed to sign");
        write_detached_signature(&artifact_path, &sig).expect("Failed to write signature");

        let expected_sig_path = artifact_path.with_extension("sig");
        assert!(
            expected_sig_path.exists(),
            "Signature file should exist at {}",
            expected_sig_path.display()
        );

        let sig_bytes = std::fs::read(&expected_sig_path).expect("Failed to read sig file");
        assert_eq!(
            sig_bytes.len(),
            crate::HYBRID_SIG_LEN,
            "Signature should be {} bytes",
            crate::HYBRID_SIG_LEN
        );
    }
}
