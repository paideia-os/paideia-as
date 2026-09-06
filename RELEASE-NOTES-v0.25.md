# paideia-as v0.25 Release Notes — v0.25-session-functors bundle

**Status:** Retrospective — bundle partially shipped (3 of 4 M1 rows); this document plus
the matching CHANGELOG.md entry closes the tracking issue, #1359.
**Bundle / Round:** v0.25 (Milestone: `v0.25-session-functors`, GitHub milestone #112)
**Documented:** 2026-09-05

## Versioning note

This bundle's number does **not** match the `workspace.version` tag of the same number.
Tag `v0.25.0` shipped unrelated crypto work (SHA-256 typed intrinsic, FIPS 180-4 §6.2, issue
#1338, commit `bec881eaf6d687a4c9638a7038548a65f2f03754`, 2026-09-01). Wave 0 (MASTER_PLAN.md)
later dispatched primitives from many bundles in parallel, bumping `workspace.version` in
file-disjointness / dependency-resolution order rather than bundle order. Three of the four
`v0.25-session-functors` M1 rows landed split across two later tags: `v0.31.0` (M1-001) and
`v0.32.0` (M1-003, M1-004).

## What Shipped

- **v0.25-M1-001 session-typed functor signatures** (#1355, CLOSED). `functor F(In) -> Out
  with S: session { ... }` parser plus a Vasconcelos-style session-type ADT (dual, seq,
  branch, end, well-formedness). Landed tag `v0.31.0`, commit
  `af6f91bbf0ac171303ab23096c26f7522dacf0cb` (2026-09-03).
- **v0.25-M1-003 linear-cap consumption verifier for unsafe blocks** (#1357, CLOSED).
  `verify_unsafe_block` in `paideia-as-linear`; diagnostics L0100-L0102. Landed tag
  `v0.32.0`, commit `5a27a99532dd1551d9b2a64b28981ec829a55bc5` (2026-09-03).
- **v0.25-M1-004 `@derive(base, refinement)` macro expansion** (#1358, CLOSED).
  `expand_derive_refinement` in `paideia-as-macro`; diagnostics M0100-M0110. Landed tag
  `v0.32.0`, same commit as M1-003.

## What Did NOT Ship

- **v0.25-M1-002 effect-row inference at call sites** (#1356, **OPEN**). Row-polymorphic
  inference of effect rows at session-functor call sites was never implemented under this
  bundle — no commit touches `crates/paideia-as-types/src/effect_row.rs` (the file named in
  #1356's own scope). A related-but-distinct primitive, row-polymorphic *effects* with
  unification (`crates/paideia-as-types/src/row_poly.rs`, #1375, landed under the separate
  `v0.29-compositor-substrate` bundle at tag `v0.31.0`), provides adjacent `EffectRow`
  infrastructure but does not implement call-site inference against the M1-001 functor
  signatures. #1356 remains open and undone; recommended as a follow-up dispatch row.

The `v0.25-session-functors` milestone is therefore 3-of-4 M1 rows complete and cannot be
marked fully closed until #1356 lands.

## Issue Reference

Closes #1359 (this v0.25 integration + release-notes + CHANGELOG issue). Note: #1359's own
scope text lists "Depends on v0.25-M1-001,002,003,004" — read literally, integration should
have awaited #1356. This document proceeds with a partial-bundle retrospective per the same
discipline as the other Wave-0 bundle write-ups in this series (document what shipped, flag
what did not, close only the integration-tracking issue), rather than leaving #1359 open
indefinitely. #1356 itself is **not** claimed as shipped and stays open.

## Tag / Commit

- M1-001 (#1355) landed at tag `v0.31.0`, commit `af6f91bbf0ac171303ab23096c26f7522dacf0cb`.
- M1-003 (#1357) / M1-004 (#1358) landed at tag `v0.32.0`, commit
  `5a27a99532dd1551d9b2a64b28981ec829a55bc5`.
- M1-002 (#1356) has not landed as of this writing.
- This retrospective documentation is its own, separate commit (see the CHANGELOG.md
  `v0.25-session-functors retrospective` entry).
