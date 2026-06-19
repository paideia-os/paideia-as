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

The current CLI uses a **deterministic test keypair** (seeded with `[7u8; 32]`). This is suitable for development and testing but **not suitable for production release signing**.

Phase-2-m7-006 will integrate HSM-backed keys for operational-tier security. The signing API remains unchanged.

## Future

- **m7-006**: HSM integration (e.g., YubiHSM, AWS CloudHSM).
- **git tag --sign analog**: Sign git tags directly.
- **Verification CLI subcommand**: `paideia-pq-sign verify <artifact>`.
- **Artifact manifest**: JSON manifest with checksums and signatures for multiple artifacts.
