# paideia-as-shell-ast

**Status:** Shipped — R221.M5 (unified AST) + R221.M6 (span provenance across NFC) + R221.M7 (parser + Unicode fuzz corpus).

Lifts the context-stamped token stream produced by
`paideia-as-shell-lex` into a single `SyntaxNode` enum, driven by a
top-down recursive-descent parser that dispatches to a per-context
sub-parser without any backtracking. Spans survive the NFC
normalisation step so error diagnostics point at the user-typed
bytes, not the normalised form.

## Example

```rust
use paideia_as_shell_lex::lex;
use paideia_as_shell_ast::parse;

let tokens = lex("ls | wc -l").unwrap();
let ast = parse(&tokens).unwrap();
```

## See also

- `paideia-as-shell-lex` — upstream token source.
- `paideia-as-shell-cmd` — downstream command-module functor
  surface.
- custom-assembler.md §SH-D (parent-repo semantic-shell spec).
