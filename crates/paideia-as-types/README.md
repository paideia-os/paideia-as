# paideia-as-types

**Status:** Shipped — monomorphic type interner + lattice-class kinds.

Interns types and tags them with a substructural lattice class
(`Ordered`, `Linear`, `Affine`, `Unrestricted`). The elaborator
uses this to assign types to IR nodes per custom-assembler.md §5.

## Example

```rust
use paideia_as_types::{TypeInterner, Primitive};

let mut interner = TypeInterner::new();
let u64_id = interner.intern_primitive(Primitive::U64);
```

## See also

- [`../../design/toolchain/syntax-reference.md`](../../design/toolchain/syntax-reference.md).
- `paideia-as-effects` — the effect-row side of the discipline.
- `paideia-as-elaborator` — the primary consumer.
