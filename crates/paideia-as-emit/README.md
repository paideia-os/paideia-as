# paideia-as-emit

**Status:** Shipped — stable public API for emitting x86_64 instructions at runtime.

Primary entry point for JIT compilers and runtime environments that
need to emit individual x86_64 instructions into a buffer. Wraps the
internal encoder's single-instruction path and enforces two
load-bearing invariants:

1. **No partial emissions**: on error, the buffer is rolled back to
   its pre-call length.
2. The API shape is stable — consumed by paideia-os phase-10 (WASM
   jail) and the paideia-as self-hosting substrate.

## Example

```rust
use paideia_as_emit::{Emit, EmitError};

let mut buf = Vec::<u8>::new();
Emit::new(&mut buf).mov_r64_imm64("rax", 42).unwrap();
```

## See also

- `paideia-as-encoder` — the underlying dispatch surface.
- `paideia-as-runtime` — the `Instruction` / `Mnemonic` types.
