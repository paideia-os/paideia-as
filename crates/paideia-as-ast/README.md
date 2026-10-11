# paideia-as-ast

**Status:** Shipped — arena-backed surface AST for paideia-as source files.

Every node is interned in an `AstArena` and referred to by
`NodeId`. The `paideia-as-parser` crate is the producer;
`paideia-as-elaborator` is the consumer that lowers AST → IR.

## Example

```rust
use paideia_as_ast::{AstArena, NodeId};

let mut arena = AstArena::new();
// paideia-as-parser does the real work; AstArena is the slab it writes into.
```

## See also

- [`../../design/toolchain/syntax-reference.md`](../../design/toolchain/syntax-reference.md).
- `paideia-as-parser` — the writer.
- `paideia-as-elaborator` — the reader.
