# paideia-as-shell-lex

**Status:** Shipped — R221.M4 context-tracking lexer for the semantic shell's three sub-languages.

The shell composes three sub-languages lexically, not semantically:
the parser is in one of `Pipeline`, `Datalog`, or `Lambda` at every
point, and the current context is a stack (`Pipeline` on the bottom
by construction; `datalog { … }` pushes `Datalog`; `{ |args| body }`
pushes `Lambda`; the matching `}` pops). Each token in the output
stream is stamped with the sub-language context that produced it.

## Example

```rust
use paideia_as_shell_lex::{lex, Context};

let tokens = lex("ls | datalog { fact(a). }").unwrap();
// first few tokens are Context::Pipeline; after `datalog {` they are Context::Datalog.
```

## See also

- `paideia-as-shell-ast` — downstream unified-AST parser.
- custom-assembler.md §SH-D1 (parent-repo semantic-shell spec).
