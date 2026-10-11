# paideia-as-elaborator

**Status:** Shipped — AST → IR lowering + lattice/effect/capability walkers for paideia-as.

The thickest crate in the workspace. Owns the lowering from
`paideia-as-ast` → `paideia-as-ir`, every walker that enforces the
substructural / effect / capability discipline (`LinearityWalker`,
`EffectRowWalker`, `CapWalker`, `BorrowWalker`), the stdlib trait
registry (`stdlib_lowering.rs`), the record-return pass, the
tail-call pass, the `@endian(be|le)` lowering, aggregate-return
placement for SysV and MS-x64, and much else.

Several of its sub-modules
(`stdlib_lowering.rs`, `emit_lambda.rs`, `unsafe_walker.rs`) are
flagged for the god-file refactor directive — see
`feedback_god_file_refactor` in the project memory.

## Example

Driven from the `paideia-as build` subcommand under the hood:

```sh
paideia-as build examples/15_sum_array.pdx --emit elf64 -o /tmp/sum.o
```

## See also

- [`../../design/toolchain/walker-hookups-phase4.md`](../../design/toolchain/walker-hookups-phase4.md)
  — Phase 4 m1 closure.
- [`../../design/toolchain/stdlib-trait-lowering.md`](../../design/toolchain/stdlib-trait-lowering.md)
  — PA-r16-007-backtrack trait-method lowering registry.
- [`../../design/toolchain/borrow-checker-phase4.md`](../../design/toolchain/borrow-checker-phase4.md)
  — Phase 4 m6 closure.
