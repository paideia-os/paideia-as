# paideia-as v0.26 Release Notes — v0.26-aml-substrate bundle

**Status:** Retrospective — bundle code already shipped; this document plus the matching
CHANGELOG.md entry closes the tracking issue, #1364.
**Bundle / Round:** v0.26 (Milestone: `v0.26-aml-substrate`)
**Documented:** 2026-09-05

## Versioning note

This bundle's number does **not** match the `workspace.version` tag of the same number.
Tag `v0.26.0` shipped unrelated crypto work (HMAC-SHA256 + HKDF-Extract/Expand, RFC 5869,
commit `f8c739f6d89b91ab2e2dfbf9c78db4eba73dfdeb`, 2026-09-01). Wave 0 (MASTER_PLAN.md)
later dispatched primitives from many bundles in parallel, bumping `workspace.version` in
file-disjointness / dependency-resolution order rather than bundle order. All four
`v0.26-aml-substrate` rows landed together under **`workspace.version 0.33.0`**
(tag `v0.33.0`, commit `6596f208014e23deb1df281ecdd102fc51a6cd12`, 2026-09-03).

## What Shipped

- **`v0.26-M1-001` recursive-descent parser combinators** — `crates/paideia-as-stdlib/src/parsers.rs`
  (804 lines). (issue #1360)
- **`v0.26-M1-002` arbitrary-precision integer intrinsics `@mulu64` / `@divu64`** —
  `crates/paideia-as-intrinsic/src/wide_int.rs` (798 lines). (issue #1361)
- **`v0.26-M1-003` string interning** — `crates/paideia-as-stdlib/src/intern.rs` (391 lines).
  (issue #1362)
- **`v0.26-M1-004` stable `Result<T,E>` idiom** — `crates/paideia-as-stdlib/src/result.rs`
  (268 lines). (issue #1363)

All four rows are documented in the `## 0.33.0` CHANGELOG.md entry (Wave 0 Batch 4).

## Known Gap

Issues **#1360, #1361, #1362, #1363 remain OPEN on GitHub** as of this writing. The commit
that landed them (`6596f208`) referenced each row by number in its message body but did not
carry `Closes #NNNN` trailers, so GitHub's auto-close never fired even though the code
merged, was tested, and shipped — the same gap already documented for the `v0.30-vulkan-spirv`
bundle's #1379–#1381 in this same landing commit. This document does not close them; closing
the individual M1 rows is out of scope for the #1364 bundle-integration issue. Recommended
follow-up: a small housekeeping commit closing #1360–#1363 directly (and #1379–#1381 with
them, if not already handled).

## Issue Reference

Closes #1364 (this v0.26 integration + release-notes + CHANGELOG issue).

## Tag / Commit

- Bundle code landed at tag `v0.33.0`, commit `6596f208014e23deb1df281ecdd102fc51a6cd12`.
- This retrospective documentation is its own, separate commit (see the CHANGELOG.md
  `v0.26-aml-substrate retrospective` entry).
