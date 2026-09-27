# PAS-DEBT-B7-002 (#1530) — reflection-corpus reject runner

## Outcome
Not reactivatable in this wave. Reject runner stays `#[ignore]`'d; the ignore
reason has been sharpened so the blocker is precise and self-documenting.
Recommend closing #1530 as blocked on m3 macro-match/expand driver + fixture
authoring (tracked separately).

## Site inspection

- `tests/reflection-corpus/tests/runner.rs` has two tests:
  - `accept_corpus_emits_no_macro_codes` — already active (not
    `#[ignore]`'d). 15 real `quote {...}` fixtures under `corpus/accept/`
    plus `.placeholder` outputs; test appears healthy today.
  - `reject_corpus_emits_expected_codes` — previously `#[ignore]`'d with
    reason "reject corpus documentation-by-example until m3 driver".
- Comparator `src/lib.rs::m_codes_for` is active. It shells out to
  `paideia-as build --emit placeholder` and scrapes stderr for tokens
  matching `M\d{4}` only.

## Reject corpus audit (8 fixtures)

| Fixture | `.pdx` content | `.expect` codes | Diagnosis |
|---|---|---|---|
| `r_antiquote_outside_quote` | real (`~(v)` outside quote) | `P0170` | P-code — comparator ignores; belongs in parser-reject corpus |
| `r_finally_not_last` | trivial (`let m = ~(1)`) | `P0162` | P-code; misplaced |
| `r_malformed_quote` | real (unterminated `quote{...`) | `P0171` | P-code; misplaced |
| `r_unknown_fragment_kind` | trivial (`let m = 1`) | `P0110` | P-code; misplaced |
| `r_macro_no_matching_rule` | trivial (`let m = 1`) | comment-only | placeholder; needs macro-invoke that fires M0308 |
| `r_pattern_match_failure` | trivial (`let m = 1`) | comment-only | placeholder; needs macro-invoke |
| `r_recursion_depth` | trivial (`let m = 1`) | comment-only | placeholder; needs recursive-macro invoke that fires M0311 |
| `r_unbound_metavariable` | trivial (`let m = 1`) | comment-only | placeholder; needs macro-def with unbound RHS metavar that fires M0309 |

Two independent blockers:
1. **m3 driver.** M0308/M0309/M0311 firing paths require the m3
   macro-match/expand driver, which is not yet landed. M0312 (splice type
   mismatch) is already explicitly deferred per README.
2. **Fixture authoring.** Even once m3 lands, the 4 M-code fixtures need
   real macro definitions and invocations authored — the current stubs
   are `let m = 1` with comment-only expect files.
3. **Corpus scoping.** The 4 P-code fixtures test parse-time diagnostics
   and don't belong in an M-code harness. They should either move to a
   parser-reject corpus, or the comparator should be widened to extract
   P-codes too (out of scope; would change semantics of a healthy accept
   test).

## Change made

`tests/reflection-corpus/tests/runner.rs`:
- Left `#[ignore]` in place per task step 4.
- Expanded the doc comment above `reject_corpus_emits_expected_codes` to
  enumerate the two-part blocker with fixture names.
- Rewrote the `#[ignore = "..."]` reason so `cargo test` output names the
  blockers (m3 driver, fixture authoring, P-code relocation) and the
  ticket (PAS-DEBT-B7-002).

No code paths changed. `src/lib.rs` (comparator) untouched.
No workspace.version bump.

## Files touched

- `tests/reflection-corpus/tests/runner.rs`
- `.plans/scratch/CHANGELOG-1530-b7002-reactivate.md` (this file)

## Recommended follow-up

Close #1530 with a comment along these lines:

> Reject runner cannot be meaningfully reactivated in the debt wave.
> Two independent blockers remain:
> (1) m3 macro-match/expand driver has not landed — M0308/M0309/M0311
>     firing paths don't exist yet.
> (2) 4 reject fixtures are placeholder `let m = 1` modules that need
>     real macro invocations authored once m3 is available.
> (3) 4 other reject fixtures target P-codes (P0170/P0162/P0171/P0110)
>     which the M-code comparator does not extract; they should relocate
>     to a parser-reject corpus.
> Ignore reason has been sharpened so the situation is self-documenting
> in test output. Track fixture authoring under a new m3-scoped issue
> once m3 lands; track P-code corpus split under its own docs issue.

## Build discipline

Build not run — main should invoke `bash tools/build.sh` and re-invoke me
with the error tail if it fails. Change is comment-only, so `cargo check
--tests` should be trivially green; the risk is only that the longer
attribute string tickles a lint I did not anticipate.
