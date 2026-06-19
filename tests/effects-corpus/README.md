# effects-corpus harness

A self-contained workspace member whose only job is to walk corpora of `.pdx` files
and assert paideia-as's effect system properly handles row-polymorphic functions, handlers,
multi-shot resumption, and nested handler composition. Validates F-category codes
(F1100/F1101/F1102/F1105/F1106) and T-category code (T0510).

The harness invokes `paideia-as build` via subprocess rather than calling the elaborator
pipeline directly. This ensures the test validates the actual CLI output that end users
see, not a synthetic in-process codepath.

## Layout

```
tests/effects-corpus/
├── Cargo.toml
├── README.md
├── src/lib.rs               # the `codes_for(path)` harness
├── tests/runner.rs          # integration test entry point
└── corpus/
    ├── accept/              # 12+ fixtures: syntactically valid, emit zero F/T-codes
    │   ├── single_handler_for_io.pdx
    │   ├── row_polymorphic_function.pdx
    │   └── ... (12+ accept fixtures)
    └── reject/              # 8+ fixtures: emit expected F/T-codes via .expect sidecars
        ├── r_perform_outside_handler.pdx
        ├── r_perform_outside_handler.expect
        └── ... (8+ reject fixtures)
```

## Running

```sh
cargo test -p paideia-effects-corpus
```

This harness provides two tests:

1. **`accept_corpus_emits_no_effect_codes`** — walks every `.pdx` in `corpus/accept/`,
   invokes `paideia-as build`, and asserts zero F-category and T-category codes are
   emitted. Fixtures are syntactically valid, well-formed Paideia source.

2. **`reject_corpus_emits_expected_codes`** — walks every `.pdx` in `corpus/reject/`,
   compares emitted codes to the companion `.expect` sidecar. Currently `#[ignore]`'d
   pending m3 elaborator driver implementation (most fixture codes don't yet fire
   through the CLI).

## Adding a fixture

For a **new code**: drop the `.pdx` file into `corpus/accept/` or `corpus/reject/`
plus a sidecar `<stem>.expect` file.

For **accept fixtures**: the `.expect` file is optional (for clarity); if present,
may contain `# accept — zero effect-codes expected` or similar.

For **reject fixtures**: `.expect` file lists expected F/T-codes (one code per line):
```text
F1100   # perform outside handler
```

For a fixture to be *complete*, it must:

1. Be syntactically valid `.pdx` source the current parser accepts.
2. Clearly express (in comments) the semantic violation or behavior being tested.
3. Have a sidecar `.expect` file (required for reject fixtures).
4. Pass the appropriate test (accept or reject).

## Effect-system codes (F/T-category)

| Code   | Meaning                              | Test status    |
|--------|--------------------------------------|----------------|
| F1100  | perform outside handler              | pending m3     |
| F1101  | handler missing operation            | pending m3     |
| F1102  | handler installation order violation | pending m3     |
| F1105  | call row mismatch (effect constraint)| pending m3     |
| F1106  | pure function performs effect        | pending m3     |
| T0510  | row variable out of scope            | pending m3     |

## Coverage

- **12+ accept fixtures**: row-polymorphic functions, basic handlers, nested handlers,
  multi-shot resume, pure callers instantiating row-poly to `!{}`.

- **Multi-shot resume**: ≥5 fixtures exercise `resume` from within a handler, including
  finally clauses and nested compositions.

- **Nested handlers**: ≥3 fixtures exercise 2-3 levels of nested handlers and
  combined effect sets.

## Implementation status

- m2 (current): Fixtures present, accept test active. Accept fixtures document
  parser-accepted row-polymorphic shapes; rejection tests await elaborator codegen.

- m3: Effect walkers wired end-to-end; F/T-codes fire; reject tests activate.
  Fixtures provide regression coverage for effect constraint validation,
  row mismatch detection, and handler correctness.
