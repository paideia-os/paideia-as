# paideia-as-macro

**Status:** Shipped (scaffolded 2026-09-03 for Wave 0 Batch 3; substantive content lands in v0.25-M1-004 macro system).

Macro-system infrastructure. Hosts the phase-1 pattern-matcher
surface plus the `derive_refinement` sub-module. Superseded for new
work by the elaborator-reflection substrate
(`paideia-as-reflection`, R220.M1); phase-1 pattern macros continue
to work as a degenerate case.

## Example

```paideia
macro twice {
  ($x:expr) => { $x + $x }
}
```

## See also

- [`../../design/toolchain/macros-phase1.md`](../../design/toolchain/macros-phase1.md)
  — phase-1 pattern + phase-2 reflection outcome (**Superseded**).
- `paideia-as-reflection` — the canonical Phase 2+ form.
