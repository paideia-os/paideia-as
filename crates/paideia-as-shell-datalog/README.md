# paideia-as-shell-datalog

**Status:** Shipped — R226.M1 (AST + parser) + R226.M2 (seminaïve fixpoint evaluator) for the semantic shell's embedded Datalog sub-language.

Takes a parsed `datalog { … }` block and evaluates it against a
`SessionEdb` — the persistent extensional database carried across
REPL turns. The evaluator is seminaïve (incremental Δ-based
fixpoint); recursion is permitted but stratified.

## Example

```rust
use paideia_as_shell_datalog::{parse, SessionEdb, eval};

let mut edb = SessionEdb::new();
let program = parse("fact(1). fact(2). double(X) :- fact(X), X > 1.").unwrap();
let idb = eval(&program, &mut edb);
```

## See also

- `paideia-as-shell-ast` — top-level syntax integration (`datalog { … }`).
- `paideia-as-shell-repl` — turn-level `SessionEdb` owner.
