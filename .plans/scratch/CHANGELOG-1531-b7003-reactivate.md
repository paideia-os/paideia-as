# CHANGELOG — paideia-as#1531 (PAS-DEBT-B7-003)

## Ticket
Reactivate the effects-corpus `index_u64_outside_rawmem_row.pdx` reject
fixture at `tests/effects-corpus/tests/runner.rs:57`.

## Site status

* Call-resolution: **partially landed**. `crates/paideia-as-elaborator/
  src/name_resolution_walker.rs` (phase-4-m1-006) records use↔def pairs
  for Var nodes. `populate.rs` wires m1-006 Load/Store + m1-004
  intrinsic calls into the instruction side-table. But **end-to-end
  intrinsic-call → effect-walker dispatch on user-declared function
  bodies is not confirmed** to emit F1100 at the row-check boundary
  (only `effect_walker.rs`'s own unit tests exercise the F1100 path
  today).
* Fixture ignore state: the `#[ignore]` sits on the **entire**
  `reject_corpus_emits_expected_codes` test (runner.rs:59), not
  per-fixture. Reason cites the broader m3 elaborator driver, not just
  call-resolution.
* Fixture readiness: the fixture itself is **doubly aspirational**:
  1. The `index_u64(xs, idx)` call is commented out — no F1100 can fire.
  2. No companion `index_u64_outside_rawmem_row.expect` sidecar exists;
     the reject-corpus harness would fail with "missing .expect
     sidecar" if the parent test were un-ignored.
* Two other reject fixtures share the missing-sidecar shape:
  `r_row_closed_rejects_larger_scope.pdx`,
  `r_row_poly_fixed_not_subset.pdx`.

## Files touched

* `tests/effects-corpus/corpus/reject/index_u64_outside_rawmem_row.pdx`
  — expanded header comment to enumerate the three compound blockers
  (commented-out call, missing `.expect`, m3 driver gap) and cite
  #1531. No code change; comment-only.
* `tests/effects-corpus/tests/runner.rs` — extended the doc comment on
  `reject_corpus_emits_expected_codes` (lines ~54–79) with a
  per-fixture reactivation-debt roll-up naming #1531 and the two peer
  aspirational fixtures. `#[ignore]` attribute unchanged.

## Not changed

* No elaborator / call-resolution code touched (per ticket constraint).
* `workspace.version` not bumped (per ticket constraint).
* No `.expect` sidecar created — creating one without the fixture
  actually emitting F1100 would replace a documentation gap with a
  false-positive test failure once the parent ignore lifts.

## Recommendation

Keep #1531 **open**. Reactivation requires four coordinated steps that
cannot be done under this ticket's scope:

1. Confirm (or land) end-to-end intrinsic-call → effect-walker
   dispatch such that a user function calling an effectful intrinsic
   without the corresponding effect in its declared row emits F1100 at
   the row-check boundary. This is elaborator work.
2. Uncomment the `index_u64(xs, idx)` call in the fixture.
3. Add `corpus/reject/index_u64_outside_rawmem_row.expect` containing
   `F1100`.
4. Add a dedicated per-fixture test in `runner.rs` following the shape
   of `effect_cap_coupling_reject_fixture_emits_c1301` (line 108),
   rather than lifting the blanket `#[ignore]` on
   `reject_corpus_emits_expected_codes` — the two peer aspirational
   fixtures would still block that path.

Consider filing a follow-up ticket for the parallel reactivation of
`r_row_closed_rejects_larger_scope` and `r_row_poly_fixed_not_subset`,
since they share the same missing-sidecar / aspirational-body shape
and will likely gate on the same m3 driver landing.

## Build

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails. The changes are comment-only in
two files (one `.pdx`, one `.rs`); `cargo check --tests` should be
clean.
