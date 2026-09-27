# parser-reject-corpus harness

A self-contained workspace member whose only job is to walk corpora of
`.pdx` files and assert paideia-as's parser (`crates/paideia-as-parser`)
emits the expected **P-category** diagnostic codes on stderr — parse
errors and parser-visible malformations (macro fragment kinds, quote /
antiquote structure, handler-block ordering, etc.).

Sibling of `tests/reflection-corpus/`, which does the same for
**M-category** codes (typed-elaborator reflection: macro matching,
template expansion, hygiene, splice types). The two harnesses share
the same shell-out shape and `.pdx` + `.expect` sidecar convention;
the only structural difference is the regex the stderr scraper
applies (`/P\d{4}/` vs `/M\d{4}/`).

## Layout

```
tests/parser-reject-corpus/
├── Cargo.toml
├── README.md
├── src/lib.rs               # the `p_codes_for(path)` harness
├── tests/runner.rs          # integration test entry point
└── corpus/
    ├── accept/
    │   ├── basic_module.pdx
    │   ├── basic_module.expect
    │   ├── valid_macro_decl.pdx
    │   ├── valid_macro_decl.expect
    │   ├── valid_quote_with_antiquote.pdx
    │   └── valid_quote_with_antiquote.expect
    └── reject/
        ├── r_antiquote_outside_quote.pdx  → P0170
        ├── r_antiquote_outside_quote.expect
        ├── r_finally_not_last.pdx         → P0162
        ├── r_finally_not_last.expect
        ├── r_malformed_quote.pdx          → P0171
        ├── r_malformed_quote.expect
        ├── r_unknown_fragment_kind.pdx    → P0110
        └── r_unknown_fragment_kind.expect
```

## Running

```sh
cargo test -p paideia-parser-reject-corpus                                # accept runner only
cargo test -p paideia-parser-reject-corpus -- --include-ignored           # + reject runner
```

The reject runner is `#[ignore]`'d because it shells out to `cargo run
-p paideia-as` per fixture and expects a warmed workspace. See the
`#[ignore = ...]` reason on `reject_corpus_emits_expected_p_codes` in
`tests/runner.rs` for details.

The harness provides two tests:

1. **`accept_corpus_emits_no_parser_codes`** — walks every `.pdx` in
   `corpus/accept/`, invokes `paideia-as build --emit placeholder`,
   and asserts zero P-codes are emitted on stderr. Fixtures are
   deliberately minimal so no coincidental parser diagnostic slips
   through.

2. **`reject_corpus_emits_expected_p_codes`** — walks every `.pdx` in
   `corpus/reject/`, compares the emitted P-code set to the companion
   `.expect` sidecar. Mismatches are collected and reported together
   at the end so a single test run surfaces all fixture drift.

## Adding a fixture

Drop a `.pdx` file into `corpus/accept/` or `corpus/reject/` with a
sidecar `<stem>.expect`.

For **accept fixtures**, the `.expect` file should contain:
```text
# accept — zero parser-codes expected
```

For **reject fixtures**, the `.expect` file lists expected P-codes,
one per line (`#` starts a comment; blanks skipped):
```text
P0170   # antiquote outside quote
```

For a fixture to be *complete*, it must:

1. Be a plausible `.pdx` file the current lexer accepts.
2. Clearly express the parser-visible violation being tested (in
   comments inside the `.pdx` itself or in the `.expect` sidecar).
3. Have a sidecar `.expect` file that lists the expected P-code(s).
4. Pass the appropriate test (accept or reject).

## Parser codes tracked here (P-category)

The harness captures *every* P-code emitted, not a curated subset;
the four fixtures relocated from `reflection-corpus` in issue #1557
happen to cover these:

| Code   | Emission site                                          | Fixture                        |
|--------|--------------------------------------------------------|--------------------------------|
| P0110  | `parse_macro.rs`:392 — unknown macro fragment kind     | `r_unknown_fragment_kind.pdx`  |
| P0162  | `parse_handler.rs`:167 — `finally` not last in handler | `r_finally_not_last.pdx`       |
| P0170  | `quote.rs`:131 — antiquote outside quote block         | `r_antiquote_outside_quote.pdx`|
| P0171  | `quote.rs`:90 — malformed quote (missing `}`)          | `r_malformed_quote.pdx`        |

Add fixtures for further P-codes as needed; no code-catalog entry
needs to be updated in this harness — the comparator is regex-driven
and picks up any `P\d{4}` seen on stderr.
