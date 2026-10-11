# paideia-as-ir

**Status:** Shipped — typed core IR for paideia-as.

Parallels the AST arena pattern with a single `IrArena` slab over
`IrNodeData`. Every node carries a substructural lattice class
(`LinClass::{Ordered, Linear, Affine, Unrestricted}`) and an
interned effect-row reference (`EffectRowId`) per
custom-assembler.md §6.1. The per-node instruction payload shape
is set out in
[`design/toolchain/per-node-ir-payload-phase3.md`](../../design/toolchain/per-node-ir-payload-phase3.md).

## Example

```rust
use paideia_as_ir::{IrArena, LinClass};

let mut arena = IrArena::new();
let id = arena.alloc_literal_u64(42, LinClass::Unrestricted);
assert_eq!(arena.get(id).lin_class, LinClass::Unrestricted);
```

## See also

- [`../../design/toolchain/per-node-ir-payload-phase3.md`](../../design/toolchain/per-node-ir-payload-phase3.md).
- `paideia-as-elaborator` — the AST → IR lowering + lattice walkers.
- `paideia-as-encoder` — consumes IR node payloads and emits bytes.
