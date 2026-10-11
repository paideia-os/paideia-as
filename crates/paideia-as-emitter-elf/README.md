# paideia-as-emitter-elf

**Status:** Shipped — ELF64 writer for paideia-as object files.

Emits relocatable `.o` artifacts for the paideia-os kernel-image
target per custom-assembler.md §12.1. Selected by `--emit elf64` or
by one of the kernel-oriented `--target` triplets.

## Example

```sh
paideia-as build examples/15_sum_array.pdx --emit elf64 -o /tmp/sum.o
```

## See also

- `paideia-as-encoder` — upstream byte source.
- `paideia-as-dwarf` — DWARF 5 + paideia vendor sections written
  into the ELF.
- `paideia-as-linker` — downstream consumer (via PAX, not ELF
  directly).
