# Self-Hosting Fixture

This workspace demonstrates that `.pdx` has sufficient surface expressibility to write a real lexer, proving it is suitable for Phase 5's self-hosting target.

## What's Here

- `pdx/mini_lexer.pdx`: A minimal lexer for a fragment of `.pdx` syntax, written in `.pdx` itself.
  - Recognizes identifiers, integer literals, operators, and whitespace.
  - Not a port of `paideia-as-lexer`, but a parallel implementation.
  - Demonstrates the language's ability to express lexical analysis.

## Gating Note

**Full execution** of `.pdx` programs gates on Phase 5 (the runtime evaluator). This fixture's role is to:

1. **Prove the surface is expressive**: The mini-lexer source compiles via `paideia-as check` without gating on runtime semantics.
2. **Enable Phase 5 bootstrap**: When the Phase 5 runtime evaluator lands, real execution of mini-lexer becomes immediate; no additional language work needed.
3. **De-risk self-hosting**: By fixing the API surface now, Phase 5 can focus purely on evaluation, not design iteration.

## Building & Testing

Run the test suite to verify the fixture parses cleanly:

```bash
cargo test --test parse_mini
cargo test --test parse_mini --ignored -- --nocapture  # requires paideia-as binary
```

See `tests/parse_mini.rs` for details.
