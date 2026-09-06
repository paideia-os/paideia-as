# paideia-as v0.31 Release Notes — v0.31-color-hdr bundle

**Status:** Retrospective — bundle code already shipped; this document plus the matching
CHANGELOG.md entry close the tracking issue, #1386.
**Bundle / Round:** v0.31 (Milestone: `v0.31-color-hdr`)
**Documented:** 2026-09-05

## Versioning note

The bundle's three rows landed across two `workspace.version` tags, due to Wave 0's
dependency-resolution dispatch order rather than bundle order:

- `@fixed_point` landed under **`workspace.version 0.31.0`** (tag `v0.31.0`,
  commit `af6f91bbf0ac171303ab23096c26f7522dacf0cb`, 2026-09-03) — Wave 0 Batch 2.
- `Matrix<T, R, C>` and the CICP helpers landed under **`workspace.version 0.32.0`**
  (tag `v0.32.0`, commit `5a27a99532dd1551d9b2a64b28981ec829a55bc5`, 2026-09-03) —
  Wave 0 Batch 3.

## What Shipped

- **`@fixed_point(bits_int, bits_frac)` type modifier** — trap-on-overflow fixed-point
  arithmetic (add/sub/mul/div); unblocks G6 color-space matrix arithmetic.
  (v0.31-M1-001, issue #1383, CLOSED)
- **`Matrix<T, R, C>` stdlib type + intrinsic hook** — `pdx/matrix.pdx` plus a
  parse-cleanliness smoke test; M2 responsibilities enumerated for follow-on work.
  (v0.31-M1-002, issue #1384, CLOSED)
- **CICP-tagged image-encoding helpers** — `pdx/cicp.pdx`; 5 named tuples covering
  BT.709, sRGB, Display-P3, BT.2020 PQ, and BT.2020 HLG.
  (v0.31-M1-003, issue #1385, CLOSED)

All three M1 rows are closed on GitHub already; this document closes only the
bundle-level integration issue.

## Issue Reference

Closes #1386 (this v0.31 integration + release-notes + CHANGELOG issue).

## Tag / Commit

- Row #1383 at tag `v0.31.0`, commit `af6f91bbf0ac171303ab23096c26f7522dacf0cb`.
- Rows #1384, #1385 at tag `v0.32.0`, commit `5a27a99532dd1551d9b2a64b28981ec829a55bc5`.
