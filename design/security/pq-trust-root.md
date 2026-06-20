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

**Resolved:** `KeyScope` (BTreeSet<u32>) + `check_delegation_scope` reads the m4-004 `.paideia.effects` section, computes the union of every entry's `fixed_effects`, and verifies `pax_effects ⊆ key.scope`. Phase 3 m7-004 extended the check to row-polymorphic functions via `check_scope_subsumption_with_row_poly`: a function with an open row tail (`!{Io | e}`) is accepted under a strictly larger key scope, provided its fixed effects are a subset of the key's scope. See "Phase 3 m7 update" below for the full subsumption rule.

### §13 — Rank-5-elaborator-reflection use case

**Resolved:** Q0901's full message includes the required set, authorized set, and the missing-set difference. The signer reads the `.paideia.effects` content (which the elaborator populates from the m4-004 emitter) and reflects on it at sign time. This is the load-bearing use case for elaborator reflection — the m2 reflection track and the m7 signing track meet here.

## 5. Phase-2-m7 deferrals

- **Hardware HSM**: deferred to a future track. The soft-HSM API is `pub trait`-shaped enough that a hardware backend can implement the same interface.
- **NIST ACVP test vectors for ML-DSA**: the m7-001 KAT uses a deterministic-rnd vector instead of the full NIST ACVP test-vector set. Adequate for round-trip; the broader vector set should land when the ml-dsa crate ships them upstream.
- ~~**Row-polymorphic scope subsumption**~~: **RESOLVED in Phase 3 m7-004 (PR #581).** See "Phase 3 m7 update" below.
- **Signature timestamping / revocation**: not in scope for m7. Phase 3 may add an in-band timestamp section + revocation registry.

## Phase 3 m8: Signature lifecycle

This section consolidates the Phase 3 m8 deliverables: RFC 3161 timestamping (m8-001), revocation list format + check (m8-002), and the NIST ACVP test-vector status (m8-003 — staying open per its own AC).

### RFC 3161 timestamping (m8-001 / PR #583)

`paideia-pq-sign::timestamp` adds the client side:

- `TimestampRequest` carries a message imprint (BLAKE3-hashed in the phase-3 minimum; SHA-256/384/512 OIDs documented).
- `TimestampToken` carries (tsa_name, gen_time_seconds, serial_number, message_imprint, signature).
- `build_request(data, algo)` hashes input.
- `fetch_token(request, tsa_url)` — phase-3 scaffold returns a synthetic empty token until reqwest integration lands in a follow-up. The CLI subcommand `paideia-pq-sign timestamp --tsa-url --input` exposes the scaffold today.

The token is intended to attach to `.paideia.sig` as an additional sub-record so verifiers can replay the TSA → release-artifact chain. The PAX-section attachment landing is m8-005 closure work.

### Revocation list (m8-002 / PR #584)

`paideia-pq-sign::revocation` adds JSON-lines-format revocation:

- `RevocationEntry { key_id, revoked_at (ISO 8601), reason }`.
- `RevocationList`: HashMap-keyed, O(1) lookup, line-number-aware parser.
- CLI: `paideia-pq-sign verify --artifact --revocation-list [--ignore-revocation]`.
- key_id computation: BLAKE3 of the hybrid public key, hex prefix.

When the artifact's signing-key key_id is in the revocation list and the operator hasn't passed `--ignore-revocation`, verification rejects the artifact.

### NIST ACVP test vectors (m8-003 / PR #585) — STAYS OPEN

The `ml-dsa` Rust crate (FIPS-204 ML-DSA-65 implementation) does not yet ship the NIST ACVP test-vector corpus. Per the m8-003 issue's AC bullet 2 ("If upstream hasn't shipped by the m8 cut, document the upstream issue link; task stays open."), m8-003 documents the deferral in `tests/pq-corpus/ML_DSA_ACVP_STATUS.md` and leaves the GitHub issue OPEN. Phase-3 m8 closes with this one issue intentionally open — it activates with the upstream landing.

Existing KAT coverage: `happy_mldsa65_keygen_sign_verify_roundtrip` (generative) + the hybrid path in `happy_hybrid_keygen_sign_verify_roundtrip`. Adequate for catching regressions; ACVP coverage is gold-standard and complementary.

### Operational impact summary

Per the m8 closure:

- Signing pipelines may optionally fetch an RFC 3161 timestamp during the m7-005 `release` flow.
- Verifiers may optionally consult a revocation list at verify time.
- The ACVP coverage gap stays as the single Phase-3 deferral inside m8; the phase-2 KAT remains the operational baseline.

## Phase 3 m7 update: row-polymorphic scope subsumption

Phase 2 m7-004 deferred the row-polymorphic case (the "Row-polymorphic scope subsumption" D-row above). Phase 3 m7-004 (PR #581) discharges it.

### The subsumption rule

For a function with effect row `row_fn = !{fixed_fn | tail_fn?}` and a key with effect row `row_key = !{fixed_key | tail_key?}`, `row_fn ⊑ row_key` iff:

1. `fixed_fn ⊆ fixed_key` — the function's fixed effects are a subset of the key's.
2. If `tail_fn` is a fresh row variable (open tail), the key may have additional fixed effects beyond the function's fixed effects (the open tail "absorbs" them).
3. If `tail_fn` is absent (closed row), `fixed_fn` must equal `fixed_key` exactly — the Phase 2 "exact match only" rule applies to this case alone.

### Examples

| `row_fn`             | `row_key`            | Accepted? | Reasoning                                             |
|----------------------|----------------------|-----------|-------------------------------------------------------|
| `!{Io \| e}`         | `@{Io.read, Mmio.write}` | yes       | Open tail; `{Io} ⊆ {Io.read, Mmio.write}` after resolving effect/cap.  |
| `!{Io \| e}`         | `@{Io.read, Foo, Bar}` | yes       | Open tail absorbs `Foo, Bar`.                          |
| `!{Io}`              | `@{Io.read, Mmio.write}` | NO        | Closed row; key carries `Mmio.write` not in `fixed_fn`. |
| `!{Io, Mmio \| e}`   | `@{Io.read}`         | NO        | `fixed_fn = {Io, Mmio}` is not a subset of `{Io.read}`. |

### Implementation

`paideia-as-elaborator::check_handler::check_scope_subsumption_with_row_poly`. The function is called by the handler-installation check; it composes with the existing Q0901 emission so the resulting diagnostic gives the operator a precise reason.

Phase 2's "exact match only" caveat in the §12 paragraph above is retired; the §12 entry now points at this section.

## Phase 3 m6: Hardware HSM landing

This section consolidates the Phase 3 m6 milestone deliverables: the two hardware backends (m6-001 PKCS#11, m6-002 YubiHSM2), the composition rule (m6-003 HybridSigner), and the `Q0902 hsm-no-pq-support` diagnostic.

### Backends

| Backend          | Crate path                              | Ed25519       | ML-DSA-65     | is_hardware | Phase 3 issue |
|------------------|-----------------------------------------|---------------|---------------|-------------|---------------|
| `SoftHsm`        | `paideia-pq-sign::soft_hsm`             | software      | software      | false       | (m7-006)      |
| `Pkcs11Signer`   | `paideia-pq-sign::hsm::pkcs11`          | HSM (cryptoki)| HSM (cryptoki)| true        | m6-001        |
| `YubiHsmSigner`  | `paideia-pq-sign::hsm::yubihsm`         | YubiHSM2 fw   | (soft fallback) | true      | m6-002        |
| `HybridSigner<H,S>` | `paideia-pq-sign::hsm::hybrid`       | hardware (H)  | soft (S)      | H.is_hardware() | m6-003   |

The `HybridSigner<H, S>` is the canonical composer for the YubiHSM2 case where ML-DSA-65 isn't supported in firmware. Operator opt-in via `--opt-in-hybrid-fallback` is required (see `Q0902` below).

PKCS#11 ships ML-DSA-65 in hardware **when the underlying token supports it** (e.g., post-quantum-capable HSMs). SoftHSM2 (the test backend) does not — same fallback story; explicitly noted in the cryptoki integration.

### Q0902 — `hsm-no-pq-support`

Severity: warning. Category: `Q` (post-quantum trust).

Fires at HSM init time when the configured backend doesn't support ML-DSA-65 in hardware AND the operator hasn't passed `--opt-in-hybrid-fallback`. Without the opt-in, the init fails with exit 1 and the diagnostic surfaces the rationale: the operator must explicitly acknowledge that the PQ leg is software-protected.

The diagnostic carries a reference to this section so operators can verify the hybrid contract before opting in.

### Hardware-lane test corpus (m6-004)

`tests/pq-corpus/tests/hardware_lane.rs` ships 4 `#[ignore]`'d tests, one per backend init / opt-in path. Manual reactivation (with env vars per `docs/release-signing.md` "Hardware HSM backends (Phase 3 m6)" section) exercises the hardware path against SoftHSM2 (for the PKCS#11 lane) or a real YubiHSM2 device (for the YubiHSM2 lane).

### Composition rule (m6-003)

paideia-as composes signers through the `HsmSigner` trait. For the
common YubiHSM2 case where ML-DSA-65 isn't supported in firmware,
the canonical composition is:

```rust
HybridSigner {
    hardware: YubiHsmSigner,   // Ed25519 in firmware
    soft: SoftHsm,              // ML-DSA-65 wrapped with
                                // Argon2id + ChaCha20-Poly1305
}
```

The hybrid signature validates ONLY if both Ed25519 and ML-DSA-65
verify. The trust root carries:
- Hardware-rooted Ed25519 key (YubiHSM2 firmware).
- Software-protected ML-DSA-65 key (passphrase-derived encryption
  under the operator's control).

Forging a hybrid signature requires compromising BOTH legs. The
attacker would need to (a) exfiltrate the soft-HSM-protected
ML-DSA-65 key AND (b) extract the YubiHSM2-protected Ed25519 key,
the latter being the highest-assurance defense.

The `is_hardware()` predicate reports the Ed25519 leg's status only;
ML-DSA-65 is implicitly soft in the hybrid composition.

Operator opt-in is required via `--opt-in-hybrid-fallback` per
the Q0902 contract from m6-002.

### Phase 3 m6 HSM trait additions

The `HsmSigner` trait now includes:

```rust
pub trait HsmSigner: Send + Sync {
    fn sign_ed25519(&self, msg: &[u8]) -> Result<Vec<u8>, HsmSignerError>;
    fn sign_mldsa65(&self, msg: &[u8]) -> Result<Vec<u8>, HsmSignerError>;
    
    /// Returns true if Ed25519 keys are protected by hardware (HSM,
    /// TPM, or YubiHSM2 firmware). Phase-3 m6-003: ML-DSA-65 is
    /// always soft today; this returns the Ed25519-leg's hardware
    /// status only.
    fn is_hardware(&self) -> bool;
}
```

Implementations:
- **SoftHsm**: `is_hardware()` → false (Argon2id + ChaCha20-Poly1305)
- **Pkcs11Signer**: `is_hardware()` → true (PKCS#11 backend for HSMs)
- **YubiHsmSigner**: `is_hardware()` → true (YubiHSM2 firmware Ed25519)
- **HybridSigner<H, S>**: `is_hardware()` → H.is_hardware() (delegates to hardware leg)

## 6. References

- `docs/release-signing.md` — operational guide for `paideia-pq-sign release` / `hsm init` / `hsm release`.
- `tests/pq-corpus/` — m7-007 verification corpus (6 happy + 4 failure tests).
- `crates/paideia-pq-sign/` — implementation.
- PRs #424–#431 — the m7 deliverable.
- Upstream `pq-trust-root.md` §5 / §12 / §13 — the original questions this appendix resolves.
