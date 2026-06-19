# PQ Trust Root — Phase 2 Outcome

**Status:** Phase 2 m7 deliverable closure.
**Scope:** Documents the post-quantum signing infrastructure shipped in m7-001 through m7-007 (PRs #424–#431) and resolves the open questions §5 / §12 / §13 in the upstream `pq-trust-root.md` specification.

## 0. Pipeline overview

`paideia-pq-sign` ships seven cooperating modules:

1. **ed25519** — `Signer` impl over `ed25519-dalek` 2.x. `verify_strict` rejects malleable / small-order forms.
2. **mldsa** — `Signer` impl over `ml-dsa` 0.1.1 (RustCrypto FIPS-204 final). Uses `sign_internal` / `verify_internal` with deterministic rnd so sign and verify halves agree.
3. **hybrid** — concatenates ed25519 + mldsa keys, secret keys, and signatures. Verification is the AND of the two halves: a hybrid signature is valid only if BOTH halves verify. Secure against either component being broken.
4. **pax** — `sign_pax_hash` / `verify_pax_hash` thin wrappers that bind the hybrid signer to a 32-byte PAX content hash (the m4-007 BLAKE3 over header + section table + content).
5. **scope_check** — `KeyScope` + `check_delegation_scope` enforces `pax.effects ⊆ key.scope` by reading the `.paideia.effects` section (m4-004). Emits Q0901 on insufficiency.
6. **release** — `sign_release_artifact` + `verify_detached_signature` produce / verify detached `.sig` files alongside release tarballs.
7. **soft_hsm** — `SoftHsmFile` (Argon2id KDF + ChaCha20-Poly1305 AEAD) for development. Production uses hardware HSM via a separate implementation that won't share API.

## 1. Wire formats

| Type            | Size (bytes) | Composition                                |
|-----------------|--------------|--------------------------------------------|
| Ed25519 PK      | 32           | ed25519-dalek `VerifyingKey`               |
| Ed25519 SK      | 32           | ed25519-dalek `SigningKey` seed            |
| Ed25519 Sig     | 64           | ed25519-dalek `Signature`                  |
| ML-DSA-65 PK    | 1952         | FIPS-204 Table 1                           |
| ML-DSA-65 SK    | 4032 (full) / 32 (seed) | FIPS-204 Table 1 / ml-dsa public API      |
| ML-DSA-65 Sig   | 3309         | FIPS-204 Table 1                           |
| Hybrid PK       | 1984         | Ed25519 PK ‖ ML-DSA PK                     |
| Hybrid SK       | 64           | Ed25519 SK ‖ ML-DSA SK seed                |
| Hybrid Sig      | 3373 ≈ 3.4 KB | Ed25519 Sig ‖ ML-DSA Sig                  |

## 2. PAX two-tier signature storage

The PAX header (m4-001) has a 32-byte `pq_signature_placeholder` slot. The hybrid signature is 3373 bytes. The mismatch is resolved by two-tier storage:

- The **header slot** stores `BLAKE3(hybrid_signature)` — 32 bytes.
- The **`.paideia.sig` section** stores the actual 3373-byte signature.

A verifier:
1. Reads the PAX from disk.
2. Recomputes the content hash from header + section table + content (zeroing the header's hash and sig slots before hashing).
3. Reads the signature from `.paideia.sig`.
4. Verifies hybrid signature against the content hash.
5. Optionally cross-checks `BLAKE3(signature) == header.pq_signature_placeholder` — guards against signature swapping at the section level.

## 3. Diagnostic code

| Code  | Source                       | Meaning                                                  |
|-------|------------------------------|----------------------------------------------------------|
| Q0901 | scope_check: insufficient    | Signing key's scope does not subsume the artifact's effects. |

Q-codes live under `Category::Q` (post-quantum, 0900–0999). Added to `paideia-as-diagnostics::code.rs` + the catalog + the SARIF snapshot in m7-004.

## 4. Resolved questions

### §5 — Key management for development

**Resolved:** SoftHsmFile with Argon2id KDF + ChaCha20-Poly1305 AEAD. Versioned PDX-HSM\0 file format. DEVELOPMENT-ONLY caveat documented in `docs/release-signing.md`. Production-grade hardware HSM integration is **explicitly out of scope** for phase 2 and will ship as a separate implementation that won't share API.

### §12 — Delegation scope check

**Resolved:** `KeyScope` (BTreeSet<u32>) + `check_delegation_scope` reads the m4-004 `.paideia.effects` section, computes the union of every entry's `fixed_effects`, and verifies `pax_effects ⊆ key.scope`. Open row tails (row-polymorphic functions) are ignored at the scope-check layer — this is a phase-2-m7-004 simplification documented inline. Future PRs may extend to row-polymorphic scope.

### §13 — Rank-5-elaborator-reflection use case

**Resolved:** Q0901's full message includes the required set, authorized set, and the missing-set difference. The signer reads the `.paideia.effects` content (which the elaborator populates from the m4-004 emitter) and reflects on it at sign time. This is the load-bearing use case for elaborator reflection — the m2 reflection track and the m7 signing track meet here.

## 5. Phase-2-m7 deferrals

- **Hardware HSM**: deferred to a future track. The soft-HSM API is `pub trait`-shaped enough that a hardware backend can implement the same interface.
- **NIST ACVP test vectors for ML-DSA**: the m7-001 KAT uses a deterministic-rnd vector instead of the full NIST ACVP test-vector set. Adequate for round-trip; the broader vector set should land when the ml-dsa crate ships them upstream.
- **Row-polymorphic scope subsumption**: row variables in `.paideia.effects` (m3 row-poly) are ignored at scope-check time. A strict implementation would treat any open row as "unbounded scope required" and reject; the lenient implementation here trusts the elaborator's signature check.
- **Signature timestamping / revocation**: not in scope for m7. Phase 3 may add an in-band timestamp section + revocation registry.

## 6. References

- `docs/release-signing.md` — operational guide for `paideia-pq-sign release` / `hsm init` / `hsm release`.
- `tests/pq-corpus/` — m7-007 verification corpus (6 happy + 4 failure tests).
- `crates/paideia-pq-sign/` — implementation.
- PRs #424–#431 — the m7 deliverable.
- Upstream `pq-trust-root.md` §5 / §12 / §13 — the original questions this appendix resolves.
