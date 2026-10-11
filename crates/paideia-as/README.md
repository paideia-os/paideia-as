# paideia-as

**Status:** Shipped — the CLI front end for the paideia-as toolchain. In every paideia-as release since Phase 2.

The command-line entry point. Dispatches to one of six subcommands
(`check`, `build`, `test`, `doc`, `fmt`, `lsp`, `dump-ast`), each
implemented under `src/cmd_*.rs`. The actual compilation substance
lives in the sibling crates (`paideia-as-lexer`,
`paideia-as-parser`, `paideia-as-elaborator`, `paideia-as-ir`,
`paideia-as-encoder`, `paideia-as-emitter-*`); this crate owns
only the CLI schema (`src/cli.rs`, clap) and the subcommand wiring.

## Example

Parse-check a tutorial example:

```sh
cargo run --release -p paideia-as -- check examples/01_hello_module.pdx
```

Build to ELF64:

```sh
cargo run --release -p paideia-as -- build examples/15_sum_array.pdx --emit elf64 -o /tmp/sum.o
```

## See also

- [`../../design/toolchain/syntax-reference.md`](../../design/toolchain/syntax-reference.md)
  — grammar reference.
- [`../../examples/README.md`](../../examples/README.md) — tutorial
  `.pdx` sequence.
- [`../../README.md`](../../README.md) — workspace entry point and
  crate inventory.
