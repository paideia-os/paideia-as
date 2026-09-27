# CHANGELOG — paideia-as#1535 (PAS-DEBT-B7-007)

## Ticket
Reactivate the LSP-harness latency probe at
`tests/lsp-harness/tests/harness.rs` — debt catalog row 471 records
"Latency probe `#[ignore]`'d for release-build profiling only."

## Site status (pre-change)

* Debt catalog row 471 is **stale**. There is no `#[ignore]` attribute
  on `latency_single_char_change_under_100ms` (line 172); the test is
  already an active `#[test]` and runs on every `cargo test` invocation.
* The only `ignore` reference in the file is a doc-comment on line 4
  that still describes the test as "#[ignore]'d". That comment is out
  of sync with the code.
* The test's own docstring (lines 164–170) documents the honest
  scaffold: with elaborator-side PositionIndex population still not
  wired, `diagnose_document_with_cache` returns near-instantly and the
  `<100 ms` assertion passes trivially in debug builds.
* Consequence once elaboration lands: debug-profile latency will
  exceed 100 ms even on correct code (unoptimized parser + walker
  timings are not representative), and the assertion will start
  failing spuriously in ordinary `cargo test` runs. Only release-build
  timings are meaningful.

## Option chosen: (a) with runtime `debug_assertions` skip

Removed the (already-absent) `#[ignore]` framing entirely and added a
runtime gate: the test now returns early with an `eprintln!` skip
notice under `cfg!(debug_assertions)`, and enforces the `<100 ms`
budget only under release-profile compilation. Rationale:

* The test is visible in `cargo test` output (as a passing test with
  a skip note on stderr) rather than silently suppressed by
  `#[ignore]`.
* Contributors who forget the release-profile requirement do not get a
  false failure from a debug run once elaboration lands.
* `cargo test --release -p lsp-harness latency_single_char` runs the
  full probe with the real budget enforced — no `--ignored` flag
  needed.
* Option (b) (keep `#[ignore]`, sharpen the reason) was rejected
  because there is no `#[ignore]` today; re-adding one would
  regress test visibility for zero benefit.

## Files touched

* `tests/lsp-harness/tests/harness.rs`
  * Module-level doc-comment (lines 1–7): replaced the stale
    "One #[ignore]'d latency probe" line with the new
    debug-skip / release-profile framing, citing B7-007 (#1535).
  * `latency_single_char_change_under_100ms` docstring
    (lines 164–182): appended a "Release-profile only" paragraph
    naming the invocation command and referencing B7-007 (#1535).
  * Test body: added a `cfg!(debug_assertions)` early-return with an
    `eprintln!` skip notice at the top of the function.
  * No changes to the assertion logic, the synthetic-document
    construction, the mutation shape, or the `Instant::now()`
    measurement.

## Not changed

* `workspace.version` — unchanged (per ticket constraint).
* No LSP harness library code (`lsp_harness::…`) touched — only the
  integration test's attributes and body.
* Debt catalog TSV row 471 — parent should update it to reflect the
  new state (runtime skip, not `#[ignore]`) when closing #1535.

## Recommended follow-up

1. Close #1535 as landed: the test is active, cleanly skipped in
   debug, and enforceable under release.
2. Add a lightweight CI/local convention lane that runs
   `cargo test --release -p lsp-harness latency_single_char` after any
   elaborator-side PositionIndex population change (m4 walker work
   tracked separately). Until that lane exists, the probe is
   opportunistic: contributors run it manually on release when
   touching the LSP hot path.
3. Once elaborator-side population lands and the probe starts
   measuring real work, revisit the 100 ms threshold — the current
   value predates elaboration and may need retuning (or splitting into
   parse-only vs. parse+elaborate budgets).
4. Update the debt catalog TSV row 471 to close-out state, or convert
   it into a forward-looking row tracking the release-profile CI lane
   (item 2).

## Build

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails. Changes are confined to one
integration-test file (attribute-adjacent doc updates + one early-
return `if` block); `cargo check --tests -p lsp-harness` should be
clean, and the test suite should continue to pass in debug (probe
skips) and in release (probe enforces the <100 ms budget, trivially
today).
