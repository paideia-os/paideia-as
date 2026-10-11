# paideia-as-stdlib

**Status:** Shipped — `.pdx` stdlib sources for paideia-as.

Hosts the `.pdx` source files that constitute the paideia-as
standard library. Key types (v0.18):

- Generic `Option<T>` and `Result<T, E>` in `pdx/option.pdx` and
  `pdx/result.pdx`. Currently used via hand-monomorphised aliases
  (`OptionU64`, `ResultU64U64`) pending `#997c`.
- `Str { ptr: *u8, len: u64 }` in `pdx/str.pdx` — borrowed
  byte-slice type with field accessors.

A `tests/parse_pdx.rs` harness verifies parse-cleanliness of every
`.pdx` under `pdx/`.

## Example

```paideia
import paideia.stdlib.option
let x : OptionU64 = Some(42)
```

## See also

- [`../../design/toolchain/stdlib-phase4.md`](../../design/toolchain/stdlib-phase4.md)
  — Phase 4 m11 closure appendix.
