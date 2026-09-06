# paideia-as v0.30 Release Notes — v0.30-vulkan-spirv bundle

**Status:** Retrospective — bundle code already shipped; this document plus the matching
CHANGELOG.md entry close the tracking issue, #1382.
**Bundle / Round:** v0.30 (Milestone: `v0.30-vulkan-spirv`)
**Documented:** 2026-09-05

## Versioning note

This bundle's number does **not** match the `workspace.version` tag of the same number.
Wave 0 (MASTER_PLAN.md) dispatched primitives from many bundles in parallel; `workspace.version`
was bumped in file-disjointness / dependency-resolution order, not bundle order. Tags `v0.30.0`
and `v0.30.1` shipped unrelated crypto work (ML-KEM-768 KEM, then a crypto-FFI file-split
refactor). The `v0.30-vulkan-spirv` bundle's three rows actually landed under
**`workspace.version 0.33.0`** (tag `v0.33.0`, commit `6596f208014e23deb1df281ecdd102fc51a6cd12`,
2026-09-03).

## What Shipped

- **`@spirv_module(path)`** — compile-time SPIR-V import as a `KIND_MEMORY` symbol;
  little-endian magic-word validation (`0x07230203`); emits to a `.rodata.spirv` section.
  `crates/paideia-as-intrinsic/src/spirv_module.rs`. (v0.30-M1-001, issue #1379)
- **`@wgsl_module(path)`** — Vello compute-shader WGSL import; UTF-8 + BOM + NUL +
  1-MiB size gates; emits to a `.rodata.wgsl` section.
  `crates/paideia-as-intrinsic/src/wgsl_module.rs`. (v0.30-M1-002, issue #1380)
- **`f16` type intrinsic** — hand-rolled IEEE 754 binary16 encoding; round-to-nearest-ties-even;
  subnormal handling; unblocks scRGB-linear color-pipeline work.
  `crates/paideia-as-intrinsic/src/f16.rs`. (v0.30-M1-003, issue #1381)

All three rows are documented in the `## 0.33.0` CHANGELOG.md entry (Wave 0 Batch 4).

## Known Gap

Issues **#1379, #1380, #1381 remain OPEN on GitHub** as of this writing. The commit that landed
them (`6596f208`) did not carry `Closes #NNNN` trailers, so GitHub's auto-close never fired even
though the code merged, was tested, and shipped. This document does not close them — closing the
individual M1 rows is out of scope for the #1382 bundle-integration issue. Recommended follow-up:
a small housekeeping commit closing #1379, #1380, and #1381 directly.

## Issue Reference

Closes #1382 (this v0.30 integration + release-notes + CHANGELOG issue).

## Tag / Commit

- Bundle code landed at tag `v0.33.0`, commit `6596f208014e23deb1df281ecdd102fc51a6cd12`.
- This retrospective documentation is its own, separate commit (see the CHANGELOG.md
  `v0.30-vulkan-spirv retrospective` entry).
