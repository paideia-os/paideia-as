# paideia-as-shell-repl

**Status:** Shipped — R229.M1 REPL turn pipeline skeleton.

One REPL "turn" is a single string of source the user typed (a
pipeline, a `datalog { … }` block, a lambda, …). This crate owns
the four-stage pipeline that a turn walks — parse, typecheck,
elaborate, execute — plus the `ReplState` (turn counter + persistent
`SessionEdb` from `paideia-as-shell-datalog`) that outlives the
individual turn.

## Example

```rust
use paideia_as_shell_repl::{ReplState, run_turn};

let mut state = ReplState::new();
let out = run_turn(&mut state, "ls | wc -l").unwrap();
```

## See also

- `paideia-as-shell-lex` → `paideia-as-shell-ast` →
  `paideia-as-shell-hm` → `paideia-as-shell-cmd` — the per-stage
  substrates this crate orchestrates.
- `paideia-as-shell-datalog::SessionEdb` — the persistent facts
  owned across turns.
