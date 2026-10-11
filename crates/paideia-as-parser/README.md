# paideia-as-parser

**Status:** Shipped — hand-written Pratt + recursive-descent parser for paideia-as source.

Consumes the token stream produced by `paideia-as-lexer` and builds
the arena-backed AST defined in `paideia-as-ast`. Parser diagnostics
land in the `P0100`–`P0299` range per the diagnostics catalog.

## Example

```rust
use paideia_as_lexer::lex;
use paideia_as_parser::parse_module;
use paideia_as_ast::AstArena;

let src = "module M = structure { let x : u64 = 1 }";
let tokens = lex(src).unwrap();
let mut arena = AstArena::new();
let module_id = parse_module(&tokens, &mut arena).unwrap();
```

## See also

- [`../../design/toolchain/syntax-reference.md`](../../design/toolchain/syntax-reference.md)
  §7 (operator precedence) and §8 (grammar EBNF).
- `paideia-as-ast` — the arena the parser writes into.
- `paideia-as-lexer` — the token stream the parser reads.
