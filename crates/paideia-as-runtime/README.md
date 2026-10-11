# paideia-as-runtime

**Status:** Shipped — `no_std` runtime-compatible core types for instruction encoding.

Provides `Instruction`, `Mnemonic`, `IrNodeId`, and the surrounding
`no_std` + `alloc`-only type substrate the paideia-as-encoder
dispatches over. Deliberately has no dependence on elaboration,
parsing, or type-checking — only on x86_64 encoding concerns — so
it can be linked into runtime environments (JIT, satellite tooling,
future self-hosting bootstrap) that cannot pull in the full
compiler.

## Example

```rust
#![no_std]
extern crate alloc;
use paideia_as_runtime::{Instruction, Mnemonic};
```

## See also

- `paideia-as-encoder` — the main consumer.
- `paideia-as-emit` — stable JIT-ready wrapper around the encoder.
