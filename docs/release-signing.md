# Release Artifact Signing

## Overview

The paideia-pq-sign CLI provides post-quantum hybrid signing for release artifacts (tarballs and git tags). The signing flow uses BLAKE3 content hashing and hybrid Ed25519 + ML-DSA-65 signatures.

## Signing Flow

1. **Tarball Creation**: Package release using standard tooling (e.g., `git archive`).
2. **BLAKE3 Hash**: Compute `BLAKE3(tarball_content)` → 32-byte hash.
3. **Hybrid Sign**: Sign the hash with the secret key using both Ed25519 (classical) and ML-DSA-65 (post-quantum).
4. **Write .sig**: Detached signature written to `<tarball_path>.sig` (3373 bytes).
5. **Publish**: Distribute tarball + signature alongside.

### Example

```bash
# Create a release tarball
git archive --format=tar.gz --prefix=paideia-as-v1.0.0/ v1.0.0 > paideia-as-v1.0.0.tar.gz

# Sign it
paideia-pq-sign release paideia-as-v1.0.0.tar.gz

# Verify (via API, see below)
```

## Detached Signatures

The signature file format is a raw concatenation:
- **Ed25519 signature**: 64 bytes (classical)
- **ML-DSA-65 signature**: 3309 bytes (post-quantum)
- **Total**: 3373 bytes

Both components must verify for the signature to be considered valid (AND semantics).

## Verification (API)

Use the `release` module to verify:

```rust
use paideia_pq_sign::release;

let artifact_path = std::path::Path::new("paideia-as-v1.0.0.tar.gz");
let is_valid = release::verify_detached_signature(&public_key, artifact_path)?;
if is_valid {
    println!("Signature valid!");
} else {
    println!("Signature invalid!");
}
```

The verification process:
1. Reads `<artifact_path>.sig`
2. Computes `BLAKE3(artifact_content)`
3. Verifies both Ed25519 and ML-DSA-65 components against the hash

## Phase-2-m7-005 Stand-in

The legacy `paideia-pq-sign release <path>` uses a **deterministic test keypair** (seeded with `[7u8; 32]`). This is suitable for development and testing but **not suitable for production release signing**.

## Phase-2-m7-006 Soft-HSM

A **soft-HSM** is now available for development environments without real HSM hardware:

### Initialization

```bash
# Create a new soft-HSM (prompted for password, or via PDX_HSM_PASSWORD env var)
paideia-pq-sign hsm init keys/release.hsm

# Or non-interactively:
export PDX_HSM_PASSWORD="your-secure-password"
paideia-pq-sign hsm init keys/release.hsm
```

The soft-HSM file contains:
- Hybrid public key (unencrypted, 1984 bytes)
- Hybrid secret key (encrypted with Argon2id KDF + ChaCha20-Poly1305)
- KDF salt and nonce (16 and 12 bytes, respectively)

### Release Signing with Soft-HSM

```bash
# Sign an artifact (prompted for HSM password, or via PDX_HSM_PASSWORD)
paideia-pq-sign hsm release keys/release.hsm artifact.tar.gz

# Produces: artifact.tar.gz.sig (3373 bytes)
```

### Soft-HSM File Format

```
magic       [u8; 8]   = b"PDX-HSM\0"
version     u8        = 1
kdf         u8        = 1 (Argon2id)
cipher      u8        = 1 (ChaCha20-Poly1305)
_reserved   u8        = 0
kdf_salt    [u8; 16]
nonce       [u8; 12]   // ChaCha20-Poly1305 nonce
ciphertext  Vec<u8>    // encrypted HybridSecretKey + auth tag
hpk         [u8; 1984] // unencrypted hybrid public key
```

### Crypto Details

- **KDF**: Argon2id with conservative parameters (~19 MiB memory, 2 iterations)
- **Cipher**: ChaCha20-Poly1305 (authenticated encryption)
- **Salt**: 16 random bytes per HSM file
- **Nonce**: 12 random bytes per encryption

### DEVELOPMENT-ONLY Caveat

The soft-HSM is **not suitable for production**. It stores the secret key encrypted-at-rest but still resident in the filesystem. Production release signing uses hardware HSM via a separate implementation (§8 of pq-trust-root.md) with ≥2-of-3 quorum enforcement and physical isolation.

## Future

- **Production HSM integration** (YubiHSM, AWS CloudHSM, Thales Luna, etc.) per pq-trust-root.md §5.1
- **git tag --sign analog**: Sign git tags directly.
- **Verification CLI subcommand**: `paideia-pq-sign verify <artifact>`.
- **Artifact manifest**: JSON manifest with checksums and signatures for multiple artifacts.
