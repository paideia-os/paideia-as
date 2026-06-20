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

## Hardware HSM backends (Phase 3 m6)

Phase 3 m6-001/002/003 added the `Pkcs11Signer` (cryptoki backend) and `YubiHsmSigner` (YubiHSM2 backend) implementations of the `HsmSigner` trait, plus the `HybridSigner<H, S>` composer for mixed hardware/soft signing.

### Hardware-lane test corpus

`tests/pq-corpus/tests/hardware_lane.rs` ships 4 `#[ignore]`'d tests, one per backend init / opt-in path. These are deliberately gated because activating them requires either:

- a SoftHSM2 install + PKCS#11 module path (for the PKCS#11 lane), or
- a YubiHSM2 device + connector URL + key id (for the YubiHSM2 lane).

#### Manual reactivation

**PKCS#11 / SoftHSM2 lane:**

```bash
# Install SoftHSM2 (Debian/Ubuntu):
sudo apt install softhsm2
sudo softhsm2-util --init-token --slot 0 --label paideia-pq-sign-test \
    --pin 1234 --so-pin 1234

# Env vars:
export SOFTHSM2_AVAILABLE=1
export SOFTHSM2_CONF=/etc/softhsm/softhsm2.conf
export PKCS11_MODULE=/usr/lib/softhsm/libsofthsm2.so
export PKCS11_SLOT=0
export PKCS11_PIN=1234

# Run:
cargo test --test hardware_lane -p paideia-pq-corpus -- --ignored pkcs11
```

**YubiHSM2 lane:**

```bash
# With a real YubiHSM2 attached + connector running:
export YUBIHSM_CONNECTOR=http://127.0.0.1:12345
export YUBIHSM_ED25519_KEY_ID=0x0001

# Run:
cargo test --test hardware_lane -p paideia-pq-corpus -- --ignored yubihsm
```

#### Hybrid-fallback contract (m6-002 / m6-003)

YubiHSM2 firmware (≤ 2.6) supports Ed25519 in hardware but **not** ML-DSA-65. The hybrid composition is therefore:

- Ed25519 leg: hardware-protected (YubiHSM2 firmware).
- ML-DSA-65 leg: soft-protected (Argon2id + ChaCha20-Poly1305 wrapper).

Operator opt-in is required via the `--opt-in-hybrid-fallback` CLI flag. Without it, `Q0902 hsm-no-pq-support` fires. See `design/security/pq-trust-root.md` Phase 3 m6 appendix for the trust-root analysis.

## Hardware lane activation (Phase 4 m3-005)

For PaideiaOS production release signing with hardware HSMs, the hardware-lane corpus tests activate uniformly via env-var early-return gates. Zero discovery cost on hosts without HSM hardware.

### PKCS#11 (SoftHSM2 for development, vendor module for production)

1. **Install SoftHSM2** (development):
   ```bash
   sudo apt install softhsm2
   ```

2. **Initialize a slot**:
   ```bash
   softhsm2-util --init-token --slot 0 --label paideia-release-key \
       --pin 1234 --so-pin 1234
   ```

3. **Import or generate keys**:
   ```bash
   pkcs11-tool --module /usr/lib/softhsm/libsofthsm2.so \
       --slot 0 --pin 1234 \
       --keypairgen --key-type EC:edwards25519 \
       --label paideia-ed25519
   ```

4. **Run hardware-lane corpus**:
   ```bash
   export SOFTHSM2_AVAILABLE=1
   export SOFTHSM2_CONF=/etc/softhsm/softhsm2.conf
   export PKCS11_MODULE=/usr/lib/softhsm/libsofthsm2.so
   export PKCS11_SLOT=0
   export PKCS11_PIN=1234
   
   cargo test --test hardware_lane -p paideia-pq-corpus -- --ignored pkcs11
   ```

### YubiHSM2

1. **Plug in YubiHSM2** and verify connector availability.

2. **Start connector** (if not already running):
   ```bash
   yubihsm-connector -d
   ```

3. **Generate or import Ed25519 key**:
   ```bash
   yubihsm-shell -p password -a generate-asymmetric-key \
       --key-id 1 --algorithm ed25519
   ```

4. **Run hardware-lane corpus**:
   ```bash
   export YUBIHSM_CONNECTOR=http://127.0.0.1:12345
   export YUBIHSM_ED25519_KEY_ID=0x0001
   
   cargo test --test hardware_lane -p paideia-pq-corpus -- --ignored yubihsm
   ```

5. **Opt-in for hybrid fallback** (required for production):
   ```bash
   paideia-pq-sign hsm yubihsm init \
       --connector http://127.0.0.1:12345 \
       --ed25519-key-id 0x0001 \
       --opt-in-hybrid-fallback
   ```
   The `--opt-in-hybrid-fallback` flag is mandatory; without it, `Q0902 hsm-no-pq-support` fires (YubiHSM2 firmware ≤ 2.6 lacks ML-DSA-65 support).

### Timestamping (TSA)

For timestamped release artifacts:

```bash
# Generate TSA token
paideia-pq-sign timestamp --tsa-url https://freetsa.org/tsr \
    --input release.tar.gz > release.tsa-token

# Verify with timestamp
paideia-pq-sign verify --artifact release.tar.gz --tsa-token release.tsa-token
```

## Future

- **Production HSM integration** (AWS CloudHSM, Thales Luna, etc.) per pq-trust-root.md §5.1.
- **git tag --sign analog**: Sign git tags directly.
- **Verification CLI subcommand**: `paideia-pq-sign verify <artifact>`.
- **Artifact manifest**: JSON manifest with checksums and signatures for multiple artifacts.
