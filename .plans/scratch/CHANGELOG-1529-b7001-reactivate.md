# PAS-DEBT-B7-001 (#1529) — examples_compile.rs + codes/m2_macro_*.pdx

## Outcome
Not reactivatable in this wave. Both `#[ignore]`'s stay in place; both
have been sharpened so `cargo test` output names the real blockers, and
the debt-catalog row has been corrected — it previously cited **#217 as
"macro driver"**, but #217 is a *closed* handler-typing ticket
("elaborator: handler well-typedness with row-polymorphic effects",
closed 2026-06-19). The actual macro-driver dependencies are **#1541**
(template expansion + substitution) and **#1542** (repetition + hygiene),
both OPEN.

Recommend #1529 stays OPEN, blocked on **three** independent items —
one for each `#[ignore]` half plus the shared macro-driver chain.

## Site inspection

The debt-catalog row `PAS-DEBT-B7-001` cites two sites in a single row:
`tests/end-to-end/tests/examples_compile.rs` **and** `codes/m2_macro_*.pdx`.
These are in fact **two independent tests with different blockers** —
the catalog row conflated them.

### Site A — `tests/end-to-end/tests/examples_compile.rs:79`

Test: `every_compiles_end_to_end_example_builds_to_elf64`.
Consumes: `examples/*.pdx` (NOT `codes/m2_macro_*.pdx`).
Previous ignore reason: `"phase-3-m1-013+: elaborator intrinsic-call
chokepoint is the last hop before examples flip to compiles-end-to-end"`.

Independent status check (2026-09-27):
- `grep 'status: compiles end-to-end' examples/*.pdx` → **0 matches**.
  So even if the m1-013+ chokepoint were resolved, removing the
  `#[ignore]` today would trip the `examples.len() >= 3` assertion.
- Blocker is NOT the macro driver at all — it's the elaborator's
  intrinsic-call lowering chain. No open issue in `gh issue list --search
  "elaborator intrinsic"` matches the m1-013+ chokepoint under a distinct
  ticket number; the reference is only self-cited in the file's own doc
  header (see lines 6–14).

### Site B — `tests/end-to-end/tests/runner.rs:30`

Test: `codes_corpus_matches_expect_files`.
Consumes: **all** `codes/*.pdx` — this is the actual test that would
exercise the `codes/m2_macro_*.pdx` fixtures the catalog row names.
Previous ignore reason: `"codes corpus awaits m2/m5 structured IR
payloads (linearity, effect, capability classes)"`.

Corpus present today under `tests/end-to-end/codes/` (with matching
`.expect` sidecars):
- `m2_macro_deep_recursion.pdx` (recursion guard — needs #209 which is CLOSED)
- `m2_macro_infinite_recursion.pdx` (recursion guard — #209 CLOSED)
- `m2_macro_terminating_at_limit.pdx` (recursion guard — #209 CLOSED)
- 8× `m2_hygiene_*.pdx` (hygiene — #1416 CLOSED via R220.M2)

Two **independent** blockers stack here:

1. **Structured-IR payload emission (m2/m5).** The runner shells out
   to `paideia-as build` and expects the S/F/C-code walkers to fire on
   structured IR. Those walkers require linearity / effect / capability
   payload types that are still m2/m5 work. This blocker is not
   macro-specific.
2. **Macro driver.** For the `m2_macro_*.pdx` fixtures specifically to
   exercise anything meaningful, the macro template-expansion driver
   must land. That is **#1541** (template expansion + substitution) plus
   **#1542** (repetition + hygiene). Both are OPEN.

## Issue-citation correction

The catalog cited `#217` as the macro driver. Actual state of `#217`
(retrieved 2026-09-27):

> **paideia-as#217** — "elaborator: handler well-typedness with
> row-polymorphic effects" — **CLOSED 2026-06-19**. Body: "Phase 1's
> `check_handler` extended to verify that the handler's operations match
> the declared effect's signature under row polymorphism."

This is an m3 handler-typing ticket, unrelated to macros. Cause of the
mis-citation is not investigated here — the catalog now names #1541 /
#1542 with a self-documenting NB on the corrected line, and the
cross-reference appendix has been updated in kind.

## Changes made

1. `tests/end-to-end/tests/examples_compile.rs` — rewrote the
   `#[ignore = "..."]` reason to name the real blocker and cite ticket
   #1529; added a block comment above the test that (a) disambiguates
   this half of the catalog row from the macro-driven half, and
   (b) notes the `examples.len() >= 3` guard would independently fail
   today. Test body unchanged.
2. `tests/end-to-end/tests/runner.rs` — rewrote the
   `#[ignore = "..."]` reason on `codes_corpus_matches_expect_files` to
   name both blockers (structured-IR payloads + macro driver #1541 /
   #1542) and cite ticket #1529; expanded the doc comment above the
   test to record the same, plus the #217 mis-citation. Test body
   unchanged. The other test in this file
   (`expect_files_cover_every_listed_code`) is NOT `#[ignore]`'d and is
   untouched.
3. `design/paideia-as-debt-catalog.md` §8.2 (row 489) — rewrote the
   `PAS-DEBT-B7-001` row to split the site column into the two real
   files with their independent blockers, correct the macro-driver
   citation from #217 → #1541 / #1542, and change "Wave 3" to
   "Deferred (blockers open)" so the tracking column matches reality.
4. `design/paideia-as-debt-catalog.md` §9.2 (row 529) — replaced the
   single stale `paideia-as#217 | Macro driver (B7-001)` row with two
   rows for #1541 and #1542, each with a self-documenting NB pointing
   at the corrected catalog entry.

## Untouched by design

- No macro-driver code (per task constraint 3).
- No `workspace.version` bump (B7 wave batches release-side; per task
  constraint 1).
- No `Cargo.toml` / `.plans/issue-map.tsv` / release-notes churn.

## Files touched

- `tests/end-to-end/tests/examples_compile.rs`
- `tests/end-to-end/tests/runner.rs`
- `design/paideia-as-debt-catalog.md`
- `.plans/scratch/CHANGELOG-1529-b7001-reactivate.md` (this file)

## Recommended follow-up

Leave #1529 **OPEN**. Post a comment along these lines:

> B7-001 cannot be reactivated in the debt wave. Site inspection
> revealed the catalog row conflates two independent tests with
> different blockers, and mis-cites #217 as the macro driver (#217 is
> a closed row-poly handler-typing ticket).
>
> Corrected mapping:
> - `tests/end-to-end/tests/examples_compile.rs::every_compiles_end_to_end_example_builds_to_elf64`
>   — blocked on the elaborator intrinsic-call lowering chokepoint
>   (m1-013+). Independent: no `examples/*.pdx` currently declares
>   `status: compiles end-to-end`, so a `.len() >= 3` guard would
>   also fail today.
> - `tests/end-to-end/tests/runner.rs::codes_corpus_matches_expect_files`
>   — the actual home of the `codes/m2_macro_*.pdx` fixtures. Blocked
>   on (a) structured-IR payload emission for S/F/C-code walkers
>   (m2/m5), and (b) macro driver #1541 (template expansion) plus
>   #1542 (repetition + hygiene). #217 does not gate this.
>
> Ticket stays open pending all three blockers. Both `#[ignore]`
> reasons have been sharpened so the situation is self-documenting in
> `cargo test` output, and the debt-catalog row + cross-reference
> appendix have been corrected. Consider splitting #1529 into
> `#1529a` (examples_compile chokepoint) and `#1529b` (codes-corpus
> + macro driver) once one blocker moves — they will land at very
> different times.

Optionally, file a docs-only follow-up to hunt for other places in the
codebase or design docs that still cite closed #217 as a "macro driver"
dependency; only the catalog was corrected here.

## Build discipline

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails. Changes are (a) comment-only inside
two test files (attribute string + doc comment) and (b) two edits to a
Markdown design doc, so `cargo check --tests` risk is limited to the
longer `#[ignore = "..."]` strings tickling a lint I did not anticipate.
