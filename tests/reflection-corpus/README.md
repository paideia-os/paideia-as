# reflection-corpus harness

A self-contained workspace member whose only job is to walk corpora of `.pdx` files
and assert paideia-as's typed elaborator reflection properly handles macros, quote/antiquote,
and related constructs. Validates M-category codes (macro reflection: M0308/M0309/M0311/M0312).

The harness invokes `paideia-as build` via subprocess rather than calling the elaborator
pipeline directly. This ensures the test validates the actual CLI output that end users
see, not a synthetic in-process codepath.

## Layout

```
tests/reflection-corpus/
├── Cargo.toml
├── README.md
├── src/lib.rs               # the `m_codes_for(path)` harness
├── tests/runner.rs          # integration test entry point
└── corpus/
    ├── accept/
    │   ├── simple_quote.pdx
    │   ├── simple_quote.expect
    │   ├── quoted_binop.pdx
    │   ├── quoted_binop.expect
    │   └── ... (15+ accept fixtures)
    └── reject/
        ├── r_antiquote_outside_quote.pdx
        ├── r_antiquote_outside_quote.expect
        ├── r_malformed_quote.pdx
        ├── r_malformed_quote.expect
        └── ... (8+ reject fixtures)
```

## Running

```sh
cargo test -p paideia-reflection-corpus
```

This harness provides two tests:

1. **`accept_corpus_emits_no_macro_codes`** — walks every `.pdx` in `corpus/accept/`,
   invokes `paideia-as build`, and asserts zero M0308/M0309/M0311/M0312 codes are
   emitted. Fixtures are syntactically valid, well-formed Paideia source.

2. **`reject_corpus_emits_expected_codes`** — walks every `.pdx` in `corpus/reject/`,
   compares emitted M-codes to the companion `.expect` sidecar. Mark `#[ignore]`'d
   fixtures with an explicit reason if they cannot yet fire their target code
   (e.g., awaiting m3 driver plumbing).

## Adding a fixture

For a **new code**: drop the `.pdx` file into `corpus/accept/` or `corpus/reject/`
plus a sidecar `<stem>.expect` file.

For **accept fixtures**: `.expect` file should contain:
```text
# accept — zero macro-codes expected
```

For **reject fixtures**: `.expect` file lists expected M-codes (one code per line):
```text
M0308   # no matching rule
```

For a fixture to be *complete*, it must:

1. Be syntactically valid `.pdx` source the current parser accepts.
2. Clearly express (in comments) the semantic violation or behavior being tested.
3. Have a sidecar `.expect` file that lists the expected code(s).
4. Pass the appropriate test (accept or reject).

## Reflection codes (M-category, 0300-0499)

| Code   | Source                                | Test status    |
|--------|---------------------------------------|----------------|
| M0308  | macro_match: no matching rule         | active         |
| M0309  | macro_expand: unbound metavariable    | active         |
| M0311  | macro_expand: recursion depth limit   | active         |
| M0312  | splice: type mismatch in result Term  | deferred (m3)  |

Code M0310 is reserved for future reflection-layer diagnostics.

## Implementation status

- m2 (current): Fixtures present, both tests active. Corpus validates quote/antiquote,
  macro matching, recursion guards, and hygiene behavior on real source.

- m3: Splice type-checking (M0312) fires once splice validates elaborated Terms
  against the elaboration context type.
