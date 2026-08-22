# ML-DSA-65 ACVP Test Vector Status

**Status:** **RESOLVED** (2026-08-22, issue #525) via the "vendor NIST JSON
directly" path — see "Landed" below. Historical audit trail preserved for
reference and to guide future refreshes.

## What this file documents

Phase 2 m7-001 shipped an ML-DSA-65 KAT (known-answer test) using a
deterministic-rnd vector. The Phase 2 deferral list (`design/security/
pq-trust-root.md` §5 "Phase-2-m7 deferrals") flagged this:

> **NIST ACVP test vectors for ML-DSA**: the m7-001 KAT uses a
> deterministic-rnd vector instead of the full NIST ACVP test-vector
> set. Adequate for round-trip; the broader vector set should land when
> the ml-dsa crate ships them upstream.

Phase 3 m8-003 (issue #525) audited upstream status; the resolution
below vendors the NIST JSON directly rather than continue to block on
`ml-dsa` exposing them.

## Landed (2026-08-22, issue #525)

Vendored the ML-DSA-65 subset of NIST ACVP FIPS-204 vectors and wired
them into a full test driver:

- **60 vectors** total (see `vectors/README.md` for the census).
  - 25 keyGen (seed → pk derivation).
  - 20 sigGen (10 deterministic + 10 hedged, both variants byte-exact).
  - 15 sigVer (3 valid + 12 tampered with ACVP-supplied `reason`).
- Sources pinned at `usnistgov/ACVP-Server @ 65370b8` — the same commit
  RustCrypto's `ml-dsa` crate tracks upstream.
- Driver: `tests/pq-corpus/tests/acvp_ml_dsa_65.rs` (four test
  functions, one per corpus file plus a wrapper-verify cross-check).
- Loader: `tests/pq-corpus/src/acvp.rs` (serde structs + hex helpers).
- ML-DSA-65 wrapper additions: two KAT entry points gated behind a new
  `paideia-pq-sign/kat` cargo feature —
  - `kat::keygen_pk_from_seed(&[u8; 32]) -> Vec<u8>`
  - `kat::sign_expanded_with_rnd(&[u8; 4032], &[u8], &[u8; 32]) -> Vec<u8>`
  - `kat::verifying_key_from_expanded_sk(&[u8; 4032]) -> Vec<u8>`
  Production surface is unchanged (feature-gated).

The pre-existing deterministic-rnd round-trip test in
`tests/corpus.rs::happy_mldsa65_keygen_sign_verify_roundtrip` is
retained — it exercises the paideia RNG-driven keygen path, which the
ACVP vectors do not cover.

## Historical upstream status (superseded by the vendoring above)

The Rust ecosystem's `ml-dsa` crate implements FIPS-204 ML-DSA-65
directly but does **not** ship the NIST ACVP test-vector corpus in
the crate itself. The vectors live at
<https://csrc.nist.gov/projects/cryptographic-algorithm-validation-program/post-quantum>
under the "ML-DSA" track.

### Refresh 2026-07-03 (paideia-os R14 take-stock review)

Rechecked upstream `RustCrypto/signatures` at `master :: ml-dsa/`:

- **NIST ACVP JSON is now present upstream**: `tests/key-gen.json`
  (~882 KB), `tests/sig-gen.json` (~1.37 MB), `tests/sig-ver.json`
  (~366 KB). Sourced from `usnistgov/ACVP-Server @ 65370b8` per
  `ml-dsa/tests/README.md`.
- **The published crate excludes them.** `ml-dsa/Cargo.toml` carries an
  explicit `exclude = [...]` list covering all three JSON files and all
  three driver `.rs` files. Latest publish is v0.1.1 (2026-06-05); v0.1.1's
  CHANGELOG shipped a `module-lattice/alloc` feature fix and does not
  mention vector exposure.
- **No `ml-dsa-acvp` sibling crate exists.** No ACVP feature flag on the
  `ml-dsa` crate.
- Consequence: a downstream depending on `ml-dsa = "0.1"` cannot get
  ACVP coverage transitively.

### Refresh 2026-08-22 (issue #525 resolution)

Rechecked `ml-dsa` on crates.io: still v0.1.1 (unchanged since
2026-06-05), still excludes vectors. Rather than continue to block on
upstream — path (a) of the trigger conditions below, unmoved for 10+
weeks — the project executed trigger path (b): **vendor NIST JSON
directly**. Vendored files live under `vectors/`; provenance in
`vectors/README.md`.

### Trigger conditions (for future reference)

Original re-open conditions from the July audit:

- (a) `ml-dsa` publishes a version that no longer excludes the vectors,
  or exposes them via a feature flag or a sibling `ml-dsa-acvp` crate.
- (b) The project decides to vendor NIST JSON directly and eat the
  sync cost.

Executed path (b) on 2026-08-22.

## Corpus refresh cadence

Refresh whenever:

- NIST reissues the ACVP-Server FIPS-204 vectors (bump the pinned commit
  in `vectors/README.md`), **or**
- The paideia workspace bumps `ml-dsa` and the new version's tracked
  ACVP commit differs from `65370b8`.

Refresh mechanics are documented in `vectors/README.md`; the process is
a mechanical filter over the upstream JSON (no domain interpretation).

## Upstream tracking

Watch:

- [RustCrypto/signatures](https://github.com/RustCrypto/signatures) — the
  organisational home of the `ml-dsa` crate.
- [NIST ACVP-Server](https://github.com/usnistgov/ACVP-Server) — for
  ML-DSA test-vector release notes and reissues.

## Remaining coverage (unchanged by this landing)

Existing round-trip coverage in `tests/pq-corpus/tests/corpus.rs`:

- `happy_mldsa65_keygen_sign_verify_roundtrip` — RNG-driven round-trip
  (exercises the wrapper's `keygen`, which the ACVP vectors do not).
- The hybrid path in `happy_hybrid_keygen_sign_verify_roundtrip` exercises
  ML-DSA-65 inside the canonical signing flow.

These stay in place — ACVP is byte-exact conformance to a specific set
of derived keys and signatures; the round-trip tests fuzz the RNG path.

## Bugs surfaced

Recorded here by the resolution PR after `cargo test -p pq-corpus
--test acvp_ml_dsa_65` runs green in CI/main. Because the paideia
wrapper delegates to `ml-dsa 0.1.1` (whose own upstream tests these
same vectors), no wrapper-level failures are expected — but the point
of this landing is that paideia's *wrapper* is now covered too, so any
future regression at the wrapper boundary will surface here.

If a vector fails, note the `tcId` + parameter set here and file a
separate issue rather than fixing inline — the corpus is
authoritative.
