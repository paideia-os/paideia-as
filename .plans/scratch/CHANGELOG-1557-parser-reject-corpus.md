# Issue #1557 — parser-reject corpus harness

## Outcome

New sibling test crate `tests/parser-reject-corpus/`
(`paideia-parser-reject-corpus`) mirrors the reflection-corpus shape
but validates P-category parser diagnostics instead of M-category
reflection diagnostics. Four misfiled fixtures relocated out of
`tests/reflection-corpus/corpus/reject/` via `git mv`; two of them had
`.pdx` bodies that did not actually trigger the claimed P-code and
have been repaired around the shapes documented in the parser's own
unit tests. Three minimal accept fixtures added. Workspace bumps
0.36.66 → 0.36.67; CHANGELOG entry landed.

Reject runner ships `#[ignore]`'d for the same env-plumbing reason
the reflection-corpus reject runner does today (shells out to `cargo
run -p paideia-as` per fixture; needs a warmed workspace). Reason
sharpened to name the CI shape needed to un-ignore.

## What changed

### New crate wiring

- `tests/parser-reject-corpus/Cargo.toml` — name
  `paideia-parser-reject-corpus`, workspace-inherited version, no
  runtime deps (harness only).
- `tests/parser-reject-corpus/src/lib.rs`:
  - `p_codes_for(path) -> Result<BTreeSet<String>, String>` —
    subprocess-out to `paideia-as build --emit placeholder`, scrapes
    stderr for `/P\d{4}/`.
  - `parse_expect_file(content)` — parses `.expect` sidecars
    (one P-code per line, `#` comments, blanks skipped).
  - `parse_p_codes_from_stderr(stderr)` — internal regex-free
    5-byte-window scanner mirroring the M-code path.
  - `CARGO_INIT` warm-up build so per-fixture `cargo run` invocations
    do not recompile.
- `tests/parser-reject-corpus/tests/runner.rs`:
  - `accept_corpus_emits_no_parser_codes` — active. Walks
    `corpus/accept/**/*.pdx`, asserts zero P-codes.
  - `reject_corpus_emits_expected_p_codes` — `#[ignore]`'d, reason
    names the CI setup needed (`cargo test --include-ignored` locally).
    Walks `corpus/reject/**/*.pdx`, reads `.expect` sidecar, asserts
    the emitted P-code set equals the expected set exactly.
- `tests/parser-reject-corpus/README.md` — layout, run commands,
  sidecar convention, per-fixture P-code table.
- Root `Cargo.toml`:
  - Workspace member added between `tests/borrow-corpus` and
    `tests/reflection-corpus`.
  - `workspace.package.version` bumped `0.36.66` → `0.36.67`.

### Fixture relocations (git mv, history preserved)

All 4 pairs moved from
`tests/reflection-corpus/corpus/reject/` to
`tests/parser-reject-corpus/corpus/reject/` (git status shows all 8
as `R`-renames):

| Fixture                          | P-code | Body status              |
|----------------------------------|--------|--------------------------|
| `r_antiquote_outside_quote.{pdx,expect}` | P0170  | body kept (correct)      |
| `r_finally_not_last.{pdx,expect}`        | P0162  | body **repaired**        |
| `r_malformed_quote.{pdx,expect}`         | P0171  | body kept (correct)      |
| `r_unknown_fragment_kind.{pdx,expect}`   | P0110  | body **repaired**        |

All 4 `.expect` sidecars rewritten from the verbose "misfiled"
diagnostic narrative to the clean one-code-per-line sidecar
convention documented in the README.

### Repairs

- `r_finally_not_last.pdx` — was `module M = structure { let m = ~(1) }`
  which triggers P0170, not P0162. New body:
  ```
  module FinallyNotLast = structure {
    let m = with h handle e { finally => i; x }
  }
  ```
  Mirrors the shape of the parser unit test at
  `crates/paideia-as-parser/src/parse_handler.rs` line ~443
  (`finally_must_be_last_emits_p0162`).

- `r_unknown_fragment_kind.pdx` — was `module M = structure { let m = 1 }`
  with no macro decl at all. New body:
  ```
  macro foo($x:wat) => { simple_form($x) }
  ```
  Mirrors the shape of the parser unit test at
  `crates/paideia-as-parser/src/parse_macro.rs` line ~959
  (`unknown_fragment_kind_emits_p0110`).

### New accept fixtures (3)

Deliberately minimal to keep the accept runner from tripping on
coincidental P-codes:

- `basic_module.pdx` — `module Basic = structure { let x = 1 }`.
- `valid_macro_decl.pdx` — `macro foo($x:expr) => { simple_form($x) }`
  (mirrors `single_rule_macro_parses`).
- `valid_quote_with_antiquote.pdx` — proves `~(v)` inside
  `quote { ... }` emits no P0170/P0171.

## Reflection-corpus untouched

Beyond the 8 file renames (4 `.pdx` + 4 `.expect`), the
reflection-corpus crate is untouched: its `Cargo.toml`, `src/lib.rs`,
`tests/runner.rs`, and the 16 remaining fixtures (11 accept + 5
reject targeting M-codes) are unchanged. Its reject runner remains
`#[ignore]`'d for its own reason (M-code pipeline glue missing —
PAS-DEBT-B7-002 / #1530), which the wave 38 changelog documents.

## Version + CHANGELOG

- `Cargo.toml` — `version = "0.36.67"`.
- `CHANGELOG.md` — new `## v0.36.67 — 2026-09-27 — Issue #1557 …`
  section immediately above the v0.36.66 heading.

## Not run

Per softarch protocol, no `cargo check` / `cargo build` /
`cargo test` invocation from this agent. Main should validate with:

```sh
cargo check --tests -p paideia-parser-reject-corpus
```

and then `bash tools/build.sh` for the full workspace warm-through.
On green, `cargo test -p paideia-parser-reject-corpus` runs the
accept fixture; `cargo test -p paideia-parser-reject-corpus -- \
--include-ignored` runs both accept + reject fixtures.
