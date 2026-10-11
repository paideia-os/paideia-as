# paideia-as-lexer

**Status:** Shipped — UTF-8 + BOM-aware source-text validation and token-stream lexer for paideia-as.

Produces the token stream `paideia-as-parser` consumes. BOM-aware
and strict about UTF-8 well-formedness at the file-read boundary.
The 71 reserved words are declared once in `src/token.rs` and
re-exported via `paideia_as_lexer::reserved::{RESERVED_WORDS,
is_reserved()}`.

## Example

```rust
use paideia_as_lexer::lex;

let tokens = lex("let x : u64 = 1").unwrap();
```

## See also

- [`../../design/toolchain/syntax-reference.md`](../../design/toolchain/syntax-reference.md).
- [`../../design/toolchain/reserved-word-policy.md`](../../design/toolchain/reserved-word-policy.md).
