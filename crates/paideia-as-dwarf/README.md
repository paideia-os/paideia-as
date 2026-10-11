# paideia-as-dwarf

**Status:** Shipped — DWARF 5 debug-info emission with paideia vendor sections.

Writes standard DWARF 5 sections (`.debug_info`, `.debug_line`,
`.debug_abbrev`, …) plus the paideia-vendor sections that carry
substructural lattice class, effect-row and capability-set
annotations alongside the regular debug info.

## Example

Driven implicitly by `paideia-as build` whenever it emits an
object format that supports DWARF (ELF64 today).

## See also

- [`../../design/toolchain/debug-info.md`](../../design/toolchain/debug-info.md)
  — Phase 2 m11-001 vendor-ID registration + design.
