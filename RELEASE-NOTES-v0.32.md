# paideia-as v0.32 Release Notes — v0.32-a11y-toolkit bundle

**Status:** Retrospective — bundle code already shipped; this document plus the matching
CHANGELOG.md entry close the tracking issue, #1390.
**Bundle / Round:** v0.32 (Milestone: `v0.32-a11y-toolkit`)
**Documented:** 2026-09-05

## Versioning note

The bundle's three rows landed across two `workspace.version` tags, again due to Wave 0's
dependency-resolution dispatch order rather than bundle order — one row (#1388) shipped a
batch *before* its own sibling row (#1387):

- Row-based subtyping (#1388) landed under **`workspace.version 0.31.0`** (tag `v0.31.0`,
  commit `af6f91bbf0ac171303ab23096c26f7522dacf0cb`) — Wave 0 Batch 2.
- Generational-index trees (#1387) and `@retain`/`@immediate` attributes (#1389) landed
  under **`workspace.version 0.32.0`** (tag `v0.32.0`,
  commit `5a27a99532dd1551d9b2a64b28981ec829a55bc5`) — Wave 0 Batch 3.

## What Shipped

- **Generational-index trees in stdlib** — `pdx/gen_index_tree.pdx`; backing structure
  for `KIND_A11Y_NODE`. (v0.32-M1-001, issue #1387, CLOSED)
- **Row-based subtyping for `KIND_A11Y_NODE`** — width subtyping via
  `RowRecord`/`RecordRowVar` (namespace-distinct from `row_poly`'s effect-row variable).
  (v0.32-M1-002, issue #1388, CLOSED)
- **`@retain` / `@immediate` functor attributes** — AST `FunctorAttr` + `FunctorAttrTable`
  + parser; diagnostics M0330–M0332. (v0.32-M1-003, issue #1389, CLOSED)

All three M1 rows are closed on GitHub already; this document closes only the
bundle-level integration issue.

## Issue Reference

Closes #1390 (this v0.32 integration + release-notes + CHANGELOG issue).

## Tag / Commit

- Row #1388 at tag `v0.31.0`, commit `af6f91bbf0ac171303ab23096c26f7522dacf0cb`.
- Rows #1387, #1389 at tag `v0.32.0`, commit `5a27a99532dd1551d9b2a64b28981ec829a55bc5`.
