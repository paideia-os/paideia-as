# FMT-13 (paideia-as#1577): corpus idempotence harness

## Summary

Opt-in sweep that asserts `format(format(src)) == format(src)` on every
`.pdx` file in the paideia-as workspace (~1,233 files). The invariant is
the single most important formatter contract: a non-idempotent formatter
causes churn forever on "format on save". The harness lands with the
FMT-3 fallback contract (byte-identical output on every input), so the
sweep is trivially green today; it becomes a real gate as FMT-4..8
structural walkers start emitting non-verbatim output.

## Files changed

- `crates/paideia-fmt/src/corpus.rs` (new, ~215 LOC incl. unit tests) —
  `walk_pdx(dir) -> Vec<PathBuf>` and `check_idempotence(path, opts) ->
  Result<IdemReport, io::Error>` plus the shared `IdemReport` enum
  (`Clean`/`Dirty`/`FormatError`). Deliberately mirrors
  `cmd_fmt::walk_pdx_files`' hidden-dir + `target/` skip. 4 unit tests
  under `#[cfg(test)]` cover the walker (sorting, hidden-dir skip),
  the clean path, the reject-fixture branch (parse-error returns
  `Clean` under the FMT-3 verbatim fallback), and I/O error
  propagation.
- `crates/paideia-fmt/src/lib.rs` — add `pub mod corpus;` and the
  FMT-13 row in the pipeline table. Keep `pub` (not
  `#[cfg(test)]`) so FMT-14 (parse-stability, paideia-as#1578) and
  FMT-15 (round-trip, paideia-as#1579) can reuse the walker + report
  types without duplication.
- `crates/paideia-fmt/tests/corpus_idempotence.rs` (new, ~245 LOC) —
  `#[test] #[ignore]` `fn fmt_corpus_is_idempotent`. Prints a
  `scanned / clean / dirty / format-error` summary; on failure prints
  a 20-line line-diff per file (capped at 10 files) and panics. Four
  always-on sanity tests cover the dir resolver (default + env
  override via `resolve_corpus_dirs_with(Option<String>)`), the
  workspace-root locator, the single-file checker on good + malformed
  input, and the walker.

## Corpus discovery

1. `PAIDEIA_FMT_CORPUS=<path>` env var — walk that directory only
   (useful for bisecting). The resolver is factored into
   `resolve_corpus_dirs_with(Option<String>)` so the sanity tests
   exercise both branches without racing on process-global env.
2. Default: `tests/` + `examples/` under the paideia-as workspace
   root, located by climbing from `CARGO_MANIFEST_DIR` until a
   `Cargo.toml` + `crates/` pair is found (bounded 6-step climb).

## Running

```sh
cargo test --package paideia-fmt --test corpus_idempotence \
    -- --include-ignored --nocapture
```

`--nocapture` is required to see the sweep summary (printed via
`eprintln!`). The test is `#[ignore]` by default — ~2,400 format
calls (two passes × ~1,200 files) is single-digit seconds but still
too heavy for the default `cargo test` loop.

## Reject fixtures (not excluded)

~86 intentionally-malformed fixtures exist in the workspace:

- `tests/parser-reject-corpus/corpus/reject/*.pdx`
- `tests/reflection-corpus/corpus/reject/*.pdx`
- `tests/modules-corpus/corpus/reject/*.pdx`
- `tests/impl-corpus/corpus/reject/*.pdx`
- `tests/effects-corpus/corpus/reject/*.pdx`
- `tests/linearity-regression/reject/*.pdx`
- `tests/borrow-corpus/corpus/r_s0908_nested_invalid.pdx`
- `tests/end-to-end/codes/r_generic_malformed_bound.pdx`
- `tests/build-emit/parse_error_p0154_exit_nonzero.pdx`
- `crates/paideia-as/tests/data/lex_error.pdx`
- `tools/klog-migrate/tests/fixtures/level_error_err*.pdx`
- `tools/klog-migrate/tests/fixtures/level_error_fail*.pdx`

They are **not** excluded: the FMT-3 fallback returns malformed input
verbatim, so idempotence holds trivially (both passes return the
original bytes). If FMT-9/10 ever starts propagating lex/parse errors
out of `format`, those files will migrate to the `format-error`
bucket (non-fatal) without breaking the `dirty == 0` gate.

## Complexity

- **Walker**: `O(N)` filesystem walk, hidden-dir and `target/` skipped.
- **Check**: 2 × full lex+parse+walk+layout passes per file + a string
  equality on the output. Dominated by the parser.
- **Sweep (current corpus, ~1,233 files)**: ~2,466 format calls, empirically
  single-digit seconds on a modern laptop. Not appropriate for the default
  `cargo test` loop, hence `#[ignore]`.
- **Peak memory**: bounded by the largest file in the corpus (`format`
  holds the AST arena + source text for one file at a time; strings are
  dropped between files). No cumulative state.

## Follow-ups wired by this ticket

- FMT-14 (paideia-as#1578, parse-stability) reuses `walk_pdx` and the
  `IdemReport::Dirty` diff plumbing.
- FMT-15 (paideia-as#1579, round-trip) reuses the same two.
- Keep the `pub mod corpus;` export — don't `#[cfg(test)]`-gate it.

## Build note

Build NOT run from this sub-agent (main-only policy). Main should
invoke `bash tools/build.sh` and re-invoke with the error tail on
failure.
