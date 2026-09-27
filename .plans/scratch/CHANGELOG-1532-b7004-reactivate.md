# PAS-DEBT-B7-004 (#1532) — linearity-regression `reject_corpus_emits_expected_s_codes`

## Outcome
**Not reactivatable in this wave.** The `#[ignore]` stays in place; its
reason string has been sharpened so `cargo test` output names the real
blockers, the surrounding doc comment now enumerates the two independent
gaps and inventories all 24 reject fixtures, and the debt-catalog row
468 has been corrected — it previously said only "awaiting borrow-checker
phase-4 driver hookup", which understated the blocker set by half.

Recommend #1532 stays OPEN, blocked on two independent items (see below);
both need their own tracking issues (neither exists today).

## Site inspection

`tests/linearity-regression/tests/harness.rs:66-67` — the test iterates
`tests/linearity-regression/reject/*.pdx` (24 fixtures) and asserts that
each fixture's emitted S-codes match its `.expect` sidecar. Removing the
`#[ignore]` today would fail on every one of the 24 fixtures because no
walker fires on any of them end-to-end.

### Gap 1 — LinearityWalker payload starvation (lowering side)

`crates/paideia-as-elaborator/src/check_linearity.rs::LinearityWalker`
is registered in `crates/paideia-as/src/cmd_build/walker_pipeline.rs`
(step 1, line 94-108) and its unit tests fire S0900/S0901/S0902/S0903/
S0904 correctly via direct arena manipulation. But end-to-end from
`.pdx` source, every IR node is minted with `LinClass::Unrestricted`
(see `crates/paideia-as-elaborator/src/lower.rs:137` doc comment).

`crates/paideia-as-elaborator/src/kind.rs::type_kind` only assigns
`Linear` / `Affine` when the type is `Type::Ref { mutable }`. No path
today lowers a bare `let unused_linear = 1` (see
`reject/s0900_never_used_01.pdx`) into a Linear binding, and the
surface syntax has no `linear` / `affine` keyword or class annotation
hook. The walker therefore observes only Unrestricted symbols on the
reject corpus and stays silent.

This gap belongs to phase-3 m2/m5 (structured payload injection). No
tracking issue exists yet — recommend filing one that cites
`crates/paideia-as-elaborator/src/kind.rs::type_kind` as the extension
point.

### Gap 2 — Borrow-checker walker not wired into the pipeline

`crates/paideia-as-elaborator/src/borrow_walker.rs::BorrowWalker` ships
with unit tests for its two-code lattice (its own S0906 = immut+mut
conflict, S0907 = double mut) but has **zero** references from
`crates/paideia-as/src/`:

```
$ grep -rn BorrowWalker crates/paideia-as/src/
(empty)
```

It is not part of the walker_pipeline.rs sequence
(LinearityWalker → EffectRowWalker → CapWalker → EmitWalker), so no
user source ever exercises it. This is exactly the "borrow-checker
phase-4 driver hookup" the catalog cites; it also has no tracking
issue yet.

### Complication — S0906/S0907 spec collision

`crates/paideia-as-elaborator/src/check_linearity.rs:167` documents:
- S0906 = branch mismatch
- S0907 = illegal lambda capture

`crates/paideia-as-elaborator/src/borrow_walker.rs:147-159` implements:
- S0906 = "Cannot borrow X as mutable because it is also borrowed as immutable"
- S0907 = "Cannot borrow X as mutable more than once"

The same two codes carry two disjoint meanings depending on which
walker fires them. Reactivation must first pick a canonical numbering.
Fixture filenames (`s0906_branch_mismatch_*`, `s0907_illegal_capture_*`)
follow the linearity-side numbering; if the borrow-side wins, either
the fixtures rename or two disjoint S-code bands split (e.g.
S0910/S0911 for borrow).

## Fixture inventory (24 files, 24 expects)

Expected `.expect` distribution (`ls reject/*.pdx | awk -F_ '{print $1}' | uniq -c`):

| S-code | Count | Fixtures                                              |
|--------|-------|-------------------------------------------------------|
| S0900  | 3     | s0900_never_used_0[1-3].pdx                           |
| S0901  | 3     | s0901_overused_0[1-3].pdx                             |
| S0902  | 2     | s0902_reserved_0[1-2].pdx                             |
| S0903  | 3     | s0903_out_of_order_0[1-3].pdx                         |
| S0904  | 4     | s0904_match_arms_*, s0904_reserved_0[1-2]             |
| S0905  | 4     | s0905_*.pdx (linearity-side spec: handler reorders)   |
| S0906  | 3     | s0906_branch_mismatch_0[1-3] (spec collision — above) |
| S0907  | 2     | s0907_illegal_capture_0[1-2] (spec collision — above) |

All fixtures inspected sample as `let x = 1` placeholders (see
`reject/s0906_branch_mismatch_01.pdx`), i.e. they compile clean today
under Unrestricted defaults. Even if the walker fired, the fixtures
themselves would need to be rewritten to exhibit the condition their
`.expect` names.

## Recommended reactivation sequence

1. File a tracking issue for **Gap 1**: `type_kind` extension so
   phase-3 m2/m5 payload injection lowers real bindings into
   `LinClass::Linear` / `Affine` for the reject corpus.
2. File a tracking issue for **Gap 2**: wire `BorrowWalker` into
   `walker_pipeline.rs` after LinearityWalker, with region entry/exit
   hooks driven by IR scope structure.
3. Resolve S0906/S0907 spec collision (one-line design note under
   `design/toolchain/`).
4. Rewrite the 24 reject fixtures so their `.pdx` bodies actually
   exhibit the condition each `.expect` names (today most are stubs
   kept green so the harness compiles).
5. Remove the `#[ignore]` from `tests/linearity-regression/tests/harness.rs:66`.

Steps 1 and 2 are independent; steps 3-5 depend on 1+2 landing.

## Files touched (comment/attribute-string + catalog only)

- `tests/linearity-regression/tests/harness.rs`
  - Sharpened `#[ignore]` reason to cite paideia-as#1532 and name both gaps.
  - Expanded doc comment above `#[test]` to enumerate expected S-codes,
    fixture inventory, spec collision, and reactivation sequence.
- `design/paideia-as-debt-catalog.md` (row 468 / `PAS-DEBT-B7-004`)
  - Rewrote symptom column: cites both gaps, file/line, and the
    S0906/S0907 spec collision; changed status column to "Deferred
    (blockers open)" per Wave 31 precedent for compound-blocked items.

## Not touched
- `crates/paideia-as-elaborator/src/` — no walker code changed (out of scope).
- 24 reject fixtures — kept as-is per wave instructions.
- Workspace version — batched wave, no bump.

## Verification hook
Main should verify with `cargo check --tests -p paideia-linearity-regression`
(comment-only changes; cannot cause compile failure). Not building myself
per softarch charter.
