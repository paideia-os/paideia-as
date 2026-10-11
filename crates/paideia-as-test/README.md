# paideia-as-test

**Status:** Shipped — the `paideia-as test` runner.

Discovers `#[test]` functions in a `.pdx` corpus and runs them via
the parser + elaborator path. Since v0.33-M1-007 (`#1393`),
`discover` is a proper AST walker rather than the earlier
plain-text substring scan — `discovered: 0` with exit 0 no longer
masks parser errors.

## Example

```sh
paideia-as test tests/corpus/
```

## See also

- [`../../design/toolchain/tooling-phase4.md`](../../design/toolchain/tooling-phase4.md)
  — Phase 4 m12 closure appendix.
- `paideia-as-doc` — the sibling doc generator.
