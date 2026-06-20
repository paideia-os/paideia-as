# ML-DSA-65 ACVP Test Vector Status

**Status:** Phase 3 m8-003 documentation deferral. Task remains OPEN.

## What this file documents

Phase 2 m7-001 shipped an ML-DSA-65 KAT (known-answer test) using a
deterministic-rnd vector. The Phase 2 deferral list (`design/security/
pq-trust-root.md` §5 "Phase-2-m7 deferrals") flagged this:

> **NIST ACVP test vectors for ML-DSA**: the m7-001 KAT uses a
> deterministic-rnd vector instead of the full NIST ACVP test-vector
> set. Adequate for round-trip; the broader vector set should land when
> the ml-dsa crate ships them upstream.

Phase 3 m8-003 audits the upstream status and decides whether to land
the NIST ACVP vectors or document the deferral.

## Upstream status (as of Phase 3 m8 review)

The Rust ecosystem's `ml-dsa` crate (used at `crates/paideia-pq-sign/
src/mldsa.rs:5`) implements FIPS-204 ML-DSA-65 directly but does **not**
ship the NIST ACVP test-vector corpus in the crate. The vectors live
at <https://csrc.nist.gov/projects/cryptographic-algorithm-validation-program/post-quantum>
under the "ML-DSA" track.

Manual integration would require:

1. Downloading the ACVP test-vector JSON (siggen, sigver, keygen).
2. Writing a JSON parser for the ACVP test-group structure.
3. Threading each test group through `MlDsa65Marker::keygen / sign /
   verify` and asserting per-vector equality.

This is doable today but the effort is not load-bearing for Phase 3
substrate (the existing KAT validates round-trip; ACVP vectors are
gold-standard verification, not blocking).

## Decision (Phase 3 m8-003)

The task stays OPEN per the issue's AC bullet 2: "If upstream hasn't
shipped by the m8 cut, document the upstream issue link; task stays
open." When the `ml-dsa` crate adds ACVP-corpus support (or a sibling
crate like `ml-dsa-acvp` lands), a follow-up PR will:

1. Add the new dep.
2. Replace the existing KAT in `crates/paideia-pq-sign/src/mldsa.rs`
   with a vector-driven test loop.
3. Mark the Phase-2-m7 deferral RESOLVED in `pq-trust-root.md` §5.

## Upstream tracking

Watch:
- [RustCrypto/signatures](https://github.com/RustCrypto/signatures) — the
  organisational home of the `ml-dsa` crate.
- [NIST ACVP](https://csrc.nist.gov/projects/cryptographic-algorithm-validation-program/post-quantum)
  for ML-DSA test-vector release notes.

## What remains as the current KAT

Existing coverage in `tests/pq-corpus/tests/corpus.rs`:

- `happy_mldsa65_keygen_sign_verify_roundtrip` — generative round-trip.
- The hybrid path in `happy_hybrid_keygen_sign_verify_roundtrip` exercises
  ML-DSA-65 inside the canonical signing flow.

Both tests use the `ml-dsa` crate's own random vector generation — adequate
for catching regressions in the wrapper but not equivalent to ACVP coverage.

## Open issue

`paideia-os/paideia-as#525` (this issue) stays in `phase3-m8-signature-
lifecycle` until upstream ships and the dep can land cleanly. The phase3-m8
milestone closes with this issue OPEN — that's the AC.
