# paideia-as-effects

**Status:** Shipped — effect-row representation and row-polymorphism unifier.

Shares the `EffectRowId` identifier space with `paideia-as-ir` so
that types, effects, and IR all speak the same ids. Row-polymorphism
is Rémy-style disjoint-row unification; the empty row `!{}` is a
checkable purity claim.

## Example

```paideia
let read_mem : forall e. (*u64) -> u64 !{RawMem | e} @{paideia.raw_mem} = …
```

## See also

- custom-assembler.md §4.2 (parent-repo cross-OS specification).
- [`../../design/effects/driver-effects.md`](../../design/effects/driver-effects.md)
  — the five driver-side effects.
- `paideia-as-ir` — co-owned identifier space.
