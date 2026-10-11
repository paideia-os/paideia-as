# paideia-as-shell-cmd

**Status:** Shipped — R222.M1..M3 command-module functor surface.

Every semantic-shell command is a functor parameterised by the
schemas and services it consumes:

```text
module Find : CommandSig = functor (Schemas : SchemasSig) -> struct
  let name = "find"
  …
```

This crate owns the `CommandSig` signature, the common `functor`
plumbing, and the registry that `paideia-as-shell-repl` dispatches
through.

## Example

Consumed via the shell REPL turn pipeline; see
`paideia-as-shell-repl` for the entry point.

## See also

- `paideia-as-shell-ast` — upstream syntax surface.
- [`../../design/toolchain/command-dispatch-pattern.md`](../../design/toolchain/command-dispatch-pattern.md)
  — command-dispatch strategies.
- `paideia-as-shell-repl` — downstream turn-driver.
