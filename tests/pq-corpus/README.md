# PQ Signature Verification Corpus

## Overview

This crate implements a comprehensive test corpus for PaideiaOS post-quantum (PQ) signing infrastructure, covering m7-001 through m7-006 of the paideia-as phase-2 milestones.

## Scope

The corpus validates end-to-end PQ signing workflows through **10 deterministic tests**:

- **6 happy paths**: Verify correct behavior for key generation, signing, verification, PAX content-hash signing, scope-checking, and soft-HSM round-trips.
- **4 failure modes**: Confirm rejection of tampered signatures, wrong keys, insufficient scope, and wrong passwords.

## Milestones Covered

- **m7-001**: Ed25519 and ML-DSA-65 individual key generation and signing
- **m7-002**: Hybrid composition (Ed25519 + ML-DSA-65 with AND semantics)
- **m7-003**: PAX content-hash signing and verification via the emitter
- **m7-004**: Scope-checking (KeyScope ⊇ effects requirement)
- **m7-006**: Soft-HSM file format and round-trips (encrypt/decrypt with password)

## Tests

### Happy Paths (6)

1. **`happy_ed25519_keygen_sign_verify_roundtrip`** (m7-001)
   - Generate Ed25519 keypair, sign message, verify signature.

2. **`happy_mldsa65_keygen_sign_verify_roundtrip`** (m7-001)
   - Generate ML-DSA-65 keypair, sign message, verify signature.

3. **`happy_hybrid_keygen_sign_verify_roundtrip`** (m7-002)
   - Generate hybrid (Ed25519 + ML-DSA-65) keypair, sign, verify with AND semantics.
   - Verify serialization/deserialization round-trip.

4. **`happy_pax_content_hash_sign_and_verify_via_emitter`** (m7-003)
   - Build minimal PAX programmatically.
   - Compute canonical content hash via emitter.
   - Sign hash with hybrid key, verify signature.

5. **`happy_scope_check_succeeds_when_key_subsumes_pax_effects`** (m7-004)
   - Build PAX with effects {100, 101, 102}.
   - Create key scope authorizing all effects.
   - Verify scope-check passes.

6. **`happy_soft_hsm_init_unlock_and_sign`** (m7-006)
   - Generate soft-HSM keypair encrypted with password.
   - Unlock with correct password.
   - Sign artifact, verify signature with HSM public key.

### Failure Modes (4)

7. **`failure_tampered_signature_rejected_by_hybrid_verify`** (m7-002)
   - Flip byte in Ed25519 half → verify fails.
   - Flip byte in ML-DSA half → verify fails.

8. **`failure_wrong_public_key_fails_verify`** (m7-002)
   - Sign with key A, verify with key B → false.

9. **`failure_scope_check_q0901_when_pax_demands_more`** (m7-004)
   - PAX requires effects {100, 101, 102, 103}.
   - Key scope only authorizes {100, 101, 102}.
   - Scope-check fails with Q0901 diagnostic.

10. **`failure_soft_hsm_wrong_password_returns_none`** (m7-006)
    - Unlock with wrong password → None.
    - Unlock with correct password → succeeds.

## Test Fixtures

All PAX fixtures are **built programmatically** using the public API of `paideia-as-emitter-pax`. No pre-built `.pdx` source files are required.

Helper functions in `src/lib.rs`:
- `build_minimal_pax()`: Construct a PAX with .code, .symtab, .paideia.caps, .exports, .paideia.effects.
- `build_pax_with_effects()`: Build a PAX with a specified set of effect IDs.

All tests use `tempfile` to manage temporary files and clean up on exit.

## Quality Assurance

- All tests pass `cargo fmt --check` (format clean).
- All tests pass `cargo clippy --workspace --all-targets -- -D warnings` (linter clean).
- No stray files left behind (tempfile cleanup).
- No use of `unsafe` code (`#![forbid(unsafe_code)]`).
- Comprehensive coverage of m7-001..006 surface area.

## Future Work

- Phase 2 m7-008+: Additional signing scenarios (batch operations, revocation).
- Integration with keystore backends beyond soft-HSM.
