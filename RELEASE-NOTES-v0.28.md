# paideia-as v0.28 Release Notes — v0.28-gpu-submit bundle

**Status:** Retrospective — bundle code already shipped; this document plus the matching
CHANGELOG.md entry closes the tracking issue, #1374.
**Bundle / Round:** v0.28 (Milestone: `v0.28-gpu-submit`)
**Documented:** 2026-09-05

## Versioning note

This bundle's number does **not** match the `workspace.version` tag of the same number.
Tag `v0.28.0` shipped unrelated crypto work (Ed25519 sign+verify, RFC 8032 §5.1, plus
SHA-512, commit `c9c82d8fbf43b1686ef4180fceac605e53d6c8f1`, 2026-09-01); `v0.28.1` was an
unrelated ML-DSA-65 verify intrinsic the next day. All four `v0.28-gpu-submit` rows
instead landed together under **`workspace.version 0.31.0`** (tag `v0.31.0`, commit
`af6f91bbf0ac171303ab23096c26f7522dacf0cb`, 2026-09-03) — Wave 0 Batch 2 of
MASTER_PLAN.md's parallel dispatch.

## What Shipped

- **`@gpu_context(engine) { stmts }` block scope** —
  `crates/paideia-as-parser/src/gpu_context.rs` (285 lines).
  (v0.28-M1-001, issue #1370, CLOSED)
- **`vec<T, N>` type parameterization** — `crates/paideia-as-types/src/vec_typaram.rs`
  (325 lines). (v0.28-M1-002, issue #1371, CLOSED)
- **`@endian(be|le)` on struct fields** — `crates/paideia-as-parser/src/endian_attr.rs`
  (427 lines). (v0.28-M1-003, issue #1372, CLOSED)
- **`@packed_struct` full support** — `crates/paideia-as-parser/src/packed_struct.rs`
  (272 lines). (v0.28-M1-004, issue #1373, CLOSED)

All four rows are closed on GitHub already; this document closes only the
bundle-level integration issue.

## Issue Reference

Closes #1374 (this v0.28 integration + release-notes + CHANGELOG issue).

## Tag / Commit

- All four rows at tag `v0.31.0`, commit `af6f91bbf0ac171303ab23096c26f7522dacf0cb`.
