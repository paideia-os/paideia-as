# PAS-DEBT-B7-001 (#1529) — codes-corpus re-test after Slices B + C

## Wave / batch context

Wave 38 (parallel with #1530 re-test). This is the **second** attempt at
reactivating `tests/end-to-end/tests/runner.rs::codes_corpus_matches_expect_files`
now that the two macro-driver dependencies previously cited in the ignore
reason have landed:

- Slice B — paideia-as#1541 (template expansion + `expand_macro`) landed
  as v0.36.65 (workspace root commit `b2be7b3a`).
- Slice C — paideia-as#1542 (macro repetition + soft hygiene) landed as
  v0.36.66 (workspace root commit `3c68c3c8`).

Workspace root is at v0.36.66 today. Per task constraint, **no version
bump in this wave** — main will consolidate with #1530 at batch close.

## Outcome

Still not reactivatable. `#[ignore]` remains on
`codes_corpus_matches_expect_files`, with a **sharpened three-blocker
reason**. The `examples_compile.rs:79` half of the debt row is
untouched (unrelated m1-013+ elaborator chokepoint — outside this
wave's scope, per task constraint 4).

## Why still ignored

The previous ignore reason named two blockers (macro-driver #1541 +
#1542, and structured-IR walker payloads). Slices B + C landed the
elaborator-side macro grammar and `expand_macro`, but the test still
cannot pass because **three** distinct blockers remain — the wave
uncovered a driver-wiring gap that the prior ignore reason did not
call out:

### Blocker (1) — structured-IR payload emission (unchanged)

Shared with `PAS-DEBT-B7-004`. The S / F / C-code diagnostic walkers
require linearity / effect / capability payload types on structured IR
nodes; today the lowering path in `paideia-as-elaborator` never mints
those payloads on real source. Walkers never fire against corpus
fixtures. No dedicated tracking issue yet — logged as a m2/m5 gap in
`design/paideia-as-debt-catalog.md` §8.2.

### Blocker (2) — `expand_macro` not wired into `cmd_build` (NEW, uncovered by this wave)

`expand_macro` exists in `crates/paideia-as-elaborator/src/macro_expand.rs`
(landed via #1541 + #1542, exhaustively unit-tested in the same file).
But:

```
$ grep -rn 'expand_macro' crates/paideia-as/src/
(no output)
```

The `paideia-as` binary's `cmd_build` pass pipeline never calls
`expand_macro`. Consequently, when the end-to-end harness (`codes_for`
in `tests/end-to-end/src/lib.rs`) invokes `paideia-as build --emit
placeholder <fixture>`, it:

- Successfully parses the `macro …` declaration in each
  `m2_macro_*.pdx` fixture (thanks to the grammar landed in Slices
  B/C).
- Never expands any macro invocation, because the pass pipeline
  contains no elaborator-driven expansion step.
- Emits zero diagnostics on the pathological fixtures
  (`m2_macro_deep_recursion.pdx`, `m2_macro_infinite_recursion.pdx`)
  — `M0311` (recursion limit) is guarded by `check_depth` inside
  `expand_macro`, which is never called from `cmd_build`.

This is a real, filable blocker on the Phase-2 "wire elaborator passes
into `cmd_build`" driver work. It has no dedicated tracking issue yet
and belongs alongside the paideia-as#1532-style umbrella issues.

### Blocker (3) — `parse_expect_file` `ok`-sentinel gap (NEW, uncovered by this wave)

`tests/end-to-end/src/lib.rs::parse_expect_file` is a naive
one-token-per-line reader:

```rust
if !trimmed.is_empty() {
    out.insert(trimmed.to_string());
}
```

It has no notion of an "expect empty diagnostic set" sentinel. 24 of
the current fixtures under `tests/end-to-end/codes/` use the `ok`
sentinel to document exactly that (`grep -l '^ok$' *.expect | wc -l`
== 24, of which 8 are `m2_*` and 8 are the older `generic_*` /
`match_*` fixtures). For every one of them, the runner will compare
`expected == {"ok"}` against `actual == {}` and fail with the
misleading message `expected {"ok"}, got {}`.

Fix belongs in the end-to-end lib crate — either interpret `ok` as
the empty set, or replace it with an explicit empty file. Either way,
outside this test-file-only wave. **This blocker would have to be
addressed even if blockers (1) and (2) were closed today**, because
half the corpus is already `ok`-sentinel-only.

## Fixtures inspected

All landed correctly per Waves 36/37 (see prior wave changelogs):

| Fixture                             | Wave | `.expect` shape | Runnable today? |
|-------------------------------------|------|-----------------|-----------------|
| `m2_macro_identity.pdx`             | 36   | `ok`            | No — blockers (2) + (3) |
| `m2_macro_swap_args.pdx`            | 36   | `ok`            | No — blockers (2) + (3) |
| `m2_macro_multi_fragment.pdx`       | 36   | `ok`            | No — blockers (2) + (3) |
| `m2_macro_star_repetition.pdx`      | 37   | `ok`            | No — blockers (2) + (3) |
| `m2_macro_plus_repetition.pdx`      | 37   | `ok`            | No — blockers (2) + (3) |
| `m2_macro_repetition_no_sep.pdx`    | 37   | `ok`            | No — blockers (2) + (3) |
| `m2_macro_deep_recursion.pdx`       | pre  | `M0311`         | No — blocker (2) |
| `m2_macro_infinite_recursion.pdx`   | pre  | `M0311`         | No — blocker (2) |
| `m2_macro_terminating_at_limit.pdx` | pre  | `ok`            | No — blockers (2) + (3) |
| 8 × `m2_hygiene_*.pdx`              | pre  | `ok`            | No — blockers (2) + (3) |

All fixtures round-trip **at the parser level** — Slices B + C
landed the grammar cleanly. What blocks re-activation is that the
harness shells out to `paideia-as build`, not to `expand_macro`
directly, and `paideia-as build` does not call `expand_macro`.

## Changes made

`tests/end-to-end/tests/runner.rs` — rewrote the doc comment above
`codes_corpus_matches_expect_files` and the `#[ignore = "..."]`
string to record:

- Slices B (#1541) + C (#1542) have landed (v0.36.65 + v0.36.66).
- Three (not two) blockers remain, each named with its precise site
  and the reason it still gates re-activation.
- Explicit instruction not to touch the attribute again until each
  dependency has a landed commit.

Test body unchanged. The other test in this file
(`expect_files_cover_every_listed_code`) is NOT `#[ignore]`'d and is
untouched.

## Untouched by design

- No fixture edits (per task constraint 4).
- No elaborator / lexer / parser code (per task constraint 4).
- No `examples_compile.rs` edits (per task constraint 4 — that
  half of the debt row has its own separate blocker).
- No harness / lib.rs edits (per task constraint 3 — only the test
  attribute).
- No `workspace.version` bump (per task constraint 6 — parallel
  wave with #1530).
- No debt-catalog row edits — the row was already sharpened in
  Wave 31 (see `.plans/scratch/CHANGELOG-1529-b7001-reactivate.md`)
  and its content is still accurate at the row level. Post-wave,
  main may want to append a "landed but not yet reactivating"
  breadcrumb to §8.2 mentioning Slices B + C.

## Non-exhaustive-match compliance

Not applicable — this change is a single attribute-string + doc-comment
rewrite in a test file. No `match` on `MacroPatternElem`,
`MacroTemplateElem`, or `MatchBinding` was added or altered.

## Files touched

- `tests/end-to-end/tests/runner.rs`
- `.plans/scratch/CHANGELOG-1529-b7001-codes-corpus-retest.md` (this file)

## Recommended follow-up

Leave paideia-as#1529 **OPEN**. Post a comment along these lines:

> Wave 38 re-test after Slices B (#1541) + C (#1542) landed:
> `codes_corpus_matches_expect_files` still not reactivatable. The
> `#[ignore]` reason has been sharpened to record three (not two)
> blockers:
>
> 1. Structured-IR payload emission for S / F / C-code walkers
>    (m2/m5, shared with PAS-DEBT-B7-004, no tracking issue yet).
> 2. `expand_macro` (elaborator crate, landed v0.36.66) is NOT
>    wired into `paideia-as` binary's `cmd_build` pass pipeline —
>    `grep -rn expand_macro crates/paideia-as/src/` returns empty.
>    Belongs on the Phase-2 "wire elaborator passes into cmd_build"
>    driver work; no tracking issue yet.
> 3. Harness `parse_expect_file` in
>    `tests/end-to-end/src/lib.rs` does not interpret the `ok`
>    sentinel (24 zero-diagnostic fixtures affected). Fix belongs
>    in the end-to-end lib crate, out of scope for the test-file-
>    only wave.
>
> Slices B + C have unblocked the elaborator-side macro grammar,
> but this test's re-activation now waits on (2) + (3) in addition
> to the walker gap. Recommend splitting #1529 into a "wire
> `expand_macro` into cmd_build" issue and an "interpret `ok`
> sentinel" harness issue once one blocker moves.

Consider filing the "wire `expand_macro` into `cmd_build`" driver
gap as its own issue on the Phase-2 track — it now blocks not only
#1529 but any downstream corpus that lands macro-heavy fixtures.

## Build discipline

Build not run — main should invoke `bash tools/build.sh` and
re-invoke me with the error tail if it fails. The change is a
single attribute-string + doc-comment rewrite inside a test file;
`cargo check --tests` risk is limited to the longer
`#[ignore = "..."]` string tripping a lint I did not anticipate.
