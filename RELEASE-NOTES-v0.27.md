# paideia-as v0.27 Release Notes — v0.27-dma-timeline bundle

**Status:** Retrospective — bundle code already shipped; this document plus the matching
CHANGELOG.md entry closes the tracking issue, #1369.
**Bundle / Round:** v0.27 (Milestone: `v0.27-dma-timeline`)
**Documented:** 2026-09-05

## Versioning note

This bundle's number does **not** match the `workspace.version` tag of the same number.
Tag `v0.27.0` shipped unrelated crypto work (X25519 keygen + scalarmult, RFC 7748 §5,
commit `a825f842bcdc8b62b53d4a4a30ea6e8d8e277fd6`, 2026-09-01), and `v0.27.1`–`v0.27.4`
were unrelated point-fix releases the same day. The bundle's four rows landed later,
split across two `workspace.version` tags by Wave 0's dependency-resolution dispatch
order rather than bundle order:

- `@timeline_wait` / `@timeline_signal` landed under **`workspace.version 0.31.0`**
  (tag `v0.31.0`, commit `af6f91bbf0ac171303ab23096c26f7522dacf0cb`, 2026-09-03) —
  Wave 0 Batch 2.
- `@dma_buffer`, the 128-bit atomic CAS intrinsic, and `@include_bytes_signed` landed
  under **`workspace.version 0.32.0`** (tag `v0.32.0`, commit
  `5a27a99532dd1551d9b2a64b28981ec829a55bc5`, 2026-09-03) — Wave 0 Batch 3.

## What Shipped

- **`@dma_buffer(size, alignment, coherency)` intrinsic** —
  `crates/paideia-as-intrinsic/src/dma_buffer.rs` (565 lines).
  (v0.27-M1-001, issue #1365, CLOSED)
- **`@timeline_wait` / `@timeline_signal` syntax** —
  `crates/paideia-as-parser/src/timeline.rs` (549 lines).
  (v0.27-M1-002, issue #1366, CLOSED)
- **128-bit atomic CAS intrinsic** — `crates/paideia-as-intrinsic/src/atomic128.rs`
  (557 lines). (v0.27-M1-003, issue #1367, CLOSED)
- **`@include_bytes_signed(path, keyring)` for firmware blobs** —
  `crates/paideia-as-intrinsic/src/include_signed.rs` (556 lines).
  (v0.27-M1-004, issue #1368, CLOSED)

All four M1 rows are closed on GitHub already; this document closes only the
bundle-level integration issue.

## Issue Reference

Closes #1369 (this v0.27 integration + release-notes + CHANGELOG issue).

## Tag / Commit

- Row #1366 at tag `v0.31.0`, commit `af6f91bbf0ac171303ab23096c26f7522dacf0cb`.
- Rows #1365, #1367, #1368 at tag `v0.32.0`, commit `5a27a99532dd1551d9b2a64b28981ec829a55bc5`.
