# paideia-as v0.33 Release Notes — post-quantum crypto substrate bundle

**Status:** Retrospective — bundle code already shipped; this document plus the matching
CHANGELOG.md entry closes the tracking issue, #1353.
**Bundle / Round:** v0.33 (Milestone: `v0.33 — post-quantum crypto substrate`, GitHub
milestone #124)
**Documented:** 2026-09-05

## Versioning note

Unlike the other Wave-0 bundle retrospectives in this series, `v0.33` **is** the live
`workspace.version` tag (current tip: `v0.33.1`) — this is not a milestone-slug-vs-tag
number mismatch. What is unusual: two of the milestone's three named primitives were already
implemented under earlier, differently-numbered issues before the `v0.33-001` / `v0.33-002`
tracking issues were even filed, so those two issues closed as "already covered by prior
work" without landing any new code; only the third primitive (ML-KEM-768) was genuinely new
work filed and landed under this bundle's own issue numbering. Tags `v0.33.0` and `v0.33.1`
additionally shipped substantial unrelated content — the v0.26-aml-substrate and
v0.30-vulkan-spirv bundles, a BLAKE3 hash intrinsic, and the paideia-as debt catalog — see
"Known Gap" below and RELEASE-NOTES-v0.26.md / RELEASE-NOTES-v0.30.md for those bundles.

## What Shipped

### Argon2id KDF (v0.33-001, #1350 — closed as duplicate, no new code)
Implemented earlier under #1302 (`Fix #1302: Argon2id KDF trait + RFC 9106 reference
vectors`, commit `b7c56bda8a8d923680ff5a9409691c83c8ed7755`, 2026-08-22), an extern-C thunk
under #1305 (`stdlib_lowering` recipes via extern-C thunks, commit
`5647d217c8c51a988e88b404e5dea71696dde072`, same day), and an integration suite under #1306
(commit `e1a05de357da0c7881b39ed34ce966988bb8cfed`, same day) — all landed untagged in the
`0.29.x` range (no `v0.29.0` tag exists). A satellite-linkable staticlib re-export followed
at `v0.29.1`. #1350 was filed later against an "encoder-conservative inline-asm" framing and
closed 2026-09-03 as covered by this prior work, with the paideia-native asm rewrite deferred
to Phase 6+ per `design/toolchain/rust-dep-gap-analysis.md`. RFC 9106 §5.3 canonical tag
pinned as `RFC_9106_ARGON2ID_TAG` in `crates/paideia-as-crypto/src/kdf/argon2id.rs`.

### ChaCha20-Poly1305 AEAD (v0.33-002, #1351 — closed as duplicate, no new code)
Same story as Argon2id: RFC 8439 seal/open landed under #1302/#1305/#1306's commits (trait +
FFI thunk + integration suite), untagged `0.29.x`, 2026-08-22. #1351 closed 2026-09-03 as
already covered — existing Rust path judged sufficient for R108 session encryption. RFC 8439
§2.8.2 canonical vector pinned as `RFC_8439_SEC_2_8_2_{KEY,NONCE,AAD,PLAINTEXT,CIPHERTEXT,
TAG}` in `crates/paideia-as-crypto/src/aead/chacha20_poly1305.rs`.

### ML-KEM-768 KEM (v0.33-003, #1352 — genuinely new work)
Tagged `v0.30.0`, commit `05af83f3299b74fc526a0240b38a5c7575a29748` (2026-09-03). New
`paideia-as-crypto::kem` module: `MlKem768::{keygen, encaps, decaps}` built on the
RustCrypto `ml-kem` v0.2 crate with the `deterministic` feature — fixed-size byte-buffer
surface (32 B seed, 1184 B `ek`, 2400 B `dk`, 1088 B `ct`, 32 B `ss`) per FIPS 203 §7. Three
extern-C FFI thunks, a `stdlib_lowering::cryptoops::MlKem768` dispatcher, and a `.pdx` trait
declaration. Pinned against three NIST ACVP vectors (ACVP-Server commit `65370b8`). This was
the only genuinely new primitive delivered under the v0.33 bundle's own issue numbering.

### Companion close-out rows (v0.33-M1-005/007/009, landed at v0.33.1)
Three companion rows closed the bundle at tag `v0.33.1`, commit
`e06452cda24a313498e8465620013c02bda5de0e` (2026-09-03):
- **M1-005** `mldsa65_verify_runtime_entry` fail-closed satellite stub, extending
  `paideia-satellite-runtime` symbol coverage (#1391, CLOSED; #1348 named alongside it in the
  same commit trailer but did not auto-close — see Known Gap).
- **M1-007** the real `paideia-as test` runner — retires the plain-text `#[test]` substring
  scan for a lexer + parser + elaborator-backed `TestRunner` that exits non-zero on a failed
  file (#1393, CLOSED; #1349 named alongside it, same auto-close gap).
- **M1-009** hw-smoke close-out — CHANGELOG `### Hardware smoke` subsection documenting the
  boot-smoke KAT reproduction protocol for all three primitives, pinning FFI-thunk register
  maps and `HWSMOKE_KAT_OK_*` serial markers per `tools/hw-smoke-v0.33.md` (#1394, #1395,
  #1353 all named in that commit; only #1395 auto-closed — this document plus its CHANGELOG
  entry is what finally discharges #1353).

### Boot-smoke KAT test harness (post-tag follow-on)
`tests/hw-smoke-crypto/` (commit `ead392a3c9fd4fbfb332cf959454e4cb70e69a41`, 2026-09-05)
exercises all six operations (`Argon2id::derive`, `ChaCha20Poly1305::{seal,open}`,
`MlKem768::{keygen,encaps,decaps}`) through the real `.pdx` → extern-C FFI boundary against
the same RFC/FIPS vectors. Landed after `v0.33.1`; not yet re-tagged (still
`workspace.version 0.33.1`). Not build/smoke-verified by the authoring agent (sub-agents
don't invoke `build.sh` / QEMU); see `tests/hw-smoke-crypto/README.md`.

## Breaking-change summary

None. All three primitives are pure additions: new crate modules
(`paideia-as-crypto::{kdf::argon2id, aead::chacha20_poly1305, kem}`), new extern-C symbols,
new `.pdx` trait declarations. No existing public API changed shape.

## RFC / FIPS test-vector coverage cross-reference

| Primitive | Spec | Vectors | Pinned at |
| --- | --- | --- | --- |
| Argon2id | RFC 9106 §5.3 | canonical single/multi-lane tag | `kdf/argon2id.rs` |
| ChaCha20-Poly1305 | RFC 8439 §2.8.2, Appendix A | AEAD vector, block fn, Poly1305 mac + Wycheproof corpus | `aead/chacha20_poly1305.rs` |
| ML-KEM-768 | FIPS 203 §6; NIST ACVP (ACVP-Server commit `65370b8`) | KeyGen / Encaps / Decaps ACVP test cases | `kem/ml_kem_768.rs` |

Cross-referenced boot-smoke protocol: `tools/hw-smoke-v0.33.md` (#1394). Cross-referenced
KAT test harness landed in XRW-03-02: `tests/hw-smoke-crypto/` (commit `ead392a`).

## Known Gap

- **#1348** (satellite-linkable runtime shim tracking issue) and **#1349** (real test-runner
  tracking issue) remain **OPEN on GitHub** despite their work landing at `v0.33.1` — the
  closing commit wrote `Closes #1391, #1348` and `Closes #1393, #1349` (comma-joined per
  bullet), and GitHub's closing-keyword parser only honors the first issue reference after
  each `Closes` keyword, so only #1391 and #1393 auto-closed. Not closed by this document;
  recommended as a small housekeeping commit closing #1348 and #1349 directly.
- Tags `v0.33.0` / `v0.33.1` are **not** crypto-substrate-exclusive releases — Wave 0's
  parallel dispatch also landed the v0.26-aml-substrate bundle, the v0.30-vulkan-spirv
  bundle, a BLAKE3 hash intrinsic, and `design/paideia-as-debt-catalog.md` under the same two
  tags (see RELEASE-NOTES-v0.26.md and RELEASE-NOTES-v0.30.md for those write-ups).
- Boot-side smoke-ELF wiring — chaining the six `HWSMOKE_KAT_OK_*` probes into an actual QEMU
  serial-log assertion in `tools/run-smoke.sh` — is explicitly deferred to Wave 1 per the
  `### Hardware smoke` CHANGELOG subsection.

## Issue Reference

Closes #1353 (this v0.33 integration + release-notes + CHANGELOG issue).

## Tag / Commit

- Argon2id + ChaCha20-Poly1305 trait/FFI/tests: untagged `0.29.x`, commits `b7c56bd` (#1302),
  `5647d21` (#1305), `e1a05de` (#1306), all 2026-08-22.
- ML-KEM-768: tag `v0.30.0`, commit `05af83f3299b74fc526a0240b38a5c7575a29748` (#1352),
  2026-09-03.
- Bundle close-out companions: tag `v0.33.1`, commit
  `e06452cda24a313498e8465620013c02bda5de0e`, 2026-09-03.
- KAT boot-smoke harness follow-on: commit `ead392a3c9fd4fbfb332cf959454e4cb70e69a41`,
  2026-09-05 (post-tag, still `workspace.version 0.33.1`).
- This retrospective documentation is its own, separate commit (see the CHANGELOG.md
  `v0.33 — post-quantum crypto substrate retrospective` entry).
