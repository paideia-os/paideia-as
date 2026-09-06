# paideia-as v0.29 Release Notes — v0.29-compositor-substrate bundle

**Status:** Retrospective — bundle code already shipped; this document plus the matching
CHANGELOG.md entry closes the tracking issue, #1378.
**Bundle / Round:** v0.29 (Milestone: `v0.29-compositor-substrate`)
**Documented:** 2026-09-05

## Versioning note

Unlike the other three bundles in this retrospective batch, **no `v0.29.0` git tag exists
at all.** `workspace.version` went from `0.28.1` straight to two untagged `0.29.x` commits —
`0.29.1` ("R91-XREPO.M1 Phase A satellite runtime shim", commit `54dfff3dfe6037124f531635da15bbc6ac309b2c`)
and `0.29.2` ("satellite libc mem-op primitives", commit `f480a8da671abe8a39e86c3991b68b4e5f854c10`),
both 2026-09-02 and both unrelated cross-repo escalation work, not this bundle — before
reaching tag `v0.30.0`. The `v0.29-compositor-substrate` bundle's three rows instead landed
under two later `workspace.version` tags, per Wave 0's dependency-resolution dispatch order:

- Row-polymorphic effects and handler composition landed under **`workspace.version 0.31.0`**
  (tag `v0.31.0`, commit `af6f91bbf0ac171303ab23096c26f7522dacf0cb`, 2026-09-03) —
  Wave 0 Batch 2.
- Session-type recursion with well-founded induction landed under
  **`workspace.version 0.32.0`** (tag `v0.32.0`, commit `5a27a99532dd1551d9b2a64b28981ec829a55bc5`,
  2026-09-03) — Wave 0 Batch 3.

## What Shipped

- **Row-polymorphic effects** — `crates/paideia-as-types/src/row_poly.rs` (433 lines).
  (v0.29-M1-001, issue #1375, CLOSED)
- **Handler composition (`handle E1 then handle E2`)** —
  `crates/paideia-as-types/src/handler_compose.rs` (385 lines).
  (v0.29-M1-002, issue #1376, CLOSED)
- **Session-type recursion with well-founded induction** —
  `crates/paideia-as-types/src/session_rec.rs` (599 lines).
  (v0.29-M1-003, issue #1377, CLOSED)

All three rows are closed on GitHub already; this document closes only the
bundle-level integration issue.

## Issue Reference

Closes #1378 (this v0.29 integration + release-notes + CHANGELOG issue).

## Tag / Commit

- Rows #1375, #1376 at tag `v0.31.0`, commit `af6f91bbf0ac171303ab23096c26f7522dacf0cb`.
- Row #1377 at tag `v0.32.0`, commit `5a27a99532dd1551d9b2a64b28981ec829a55bc5`.
