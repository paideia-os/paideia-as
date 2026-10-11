# paideia-as-linear

**Status:** Shipped (scaffolded 2026-09-03 for Wave 0 Batch 3; substantive content lands in v0.25-M1-003 linearity types).

Linearity-type infrastructure. Hosts the `unsafe_verify` surface
and sibling helpers that formalise the substructural lattice
checks (`Ordered` / `Linear` / `Affine` / `Unrestricted` per
Walker 2005) at the walker level.

## Example

Driven by `LinearityWalker` inside `paideia-as-elaborator`; this
crate hosts the shared discipline surface rather than a direct
user-facing API.

## See also

- [`../../design/toolchain/borrow-checker-phase4.md`](../../design/toolchain/borrow-checker-phase4.md)
  — Phase 4 m6 closure.
- `paideia-as-elaborator::borrow_walker` — primary consumer.
