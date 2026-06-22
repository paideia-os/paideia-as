# paideia-as PA7-Completion & Anticipated Needs — Process & Governance (softarch)

**Status:** Draft v0.1
**Date:** 2026-06-22
**Owner:** softarch agent (process / governance / cargo-green-gate dimensions)
**Round scope ceiling:** finish the PA7 byte-emit story (G1–G3), close the parser/lexer/grammar gaps (G4–G10) that paideia-os integration uncovered, and land the highest-value anticipated primitives (G11–G15) only if they fit without crossing the round size budget. The bounded stop point is "every quarantined paideia-os file builds clean; `tools/stubs.S` is deleted; the paideia-os submodule pin is bumped".
**Sister deliverable:** `.plans/pa7-completion-osarch-plan.md` (osarch) — owns the *what* (issues G1–G15, decomposition, per-issue acceptance).

---

## 0. Scope + non-overlap with osarch

This doc and the osarch companion partition PA7-completion governance cleanly. The split mirrors Phase 5 and Phase 6; this round carries the discipline forward and adds the cross-repo unquarantine cadence as a new mechanical step.

| Dimension | Owner |
|---|---|
| Issue list, sequencing, per-issue decomposition (G1–G15) | osarch (`.plans/pa7-completion-osarch-plan.md`) |
| Per-issue technical acceptance (encoder bytes, parse-tree shape, etc.) | osarch (companion) |
| PR sizing discipline (XS/S/M bands; no L this round) | softarch (this doc, §2) |
| GitHub label additions for the round | softarch (this doc, §3) |
| Milestone shape, naming, closure criterion | softarch (this doc, §4) |
| Issue body template (PA7-completion additions) | softarch (this doc, §5) |
| Tempo (continuous; pause only at the two named checkpoints) | softarch (this doc, §1) |
| cargo-green gate + workspace baseline | softarch (this doc, §6) |
| SARIF regen discipline (G4–G10 diagnostics-touching list) | softarch (this doc, §7) |
| Unquarantine protocol (the new cross-repo mechanical step) | softarch (this doc, §8) |
| Closure rituals (round close + version bump + submodule bump) | softarch (this doc, §9) |
| Documentation discipline (`design/toolchain/` per-issue + roll-up) | softarch (this doc, §10) |

Where the two docs overlap by necessity (issue identifiers, milestone names), the osarch plan is the source of truth for the *list* and the *technical content*; this doc is the source of truth for the *shape* (label set, closure criteria, retrospective discipline, tempo, unquarantine cadence).

The round is *surgical*. Per the directive: "complete PA7, add unary support, anticipate other needs; add as issues; solve one by one; when it works, unquarantine paideia-os files and continue paideia-os development." There is no L-band work this round; if any single issue projects above 400 LOC net diff, osarch must split it before this softarch plan accepts it.

---

## 1. Tempo + cross-repo handoff

PA7-completion runs continuously across all issues with **exactly two pause-and-verify checkpoints** plus a round-close ritual. This is a refinement of the Phase 6 "continuous until the cross-repo unblock event" rule: PA7-completion has two intermediate unblock events embedded inside the round, because the 13 quarantined paideia-os files split cleanly into two batches keyed to the byte-emit gap closures.

Per memories `feedback_cross_repo_escalation`, `feedback_phase6_to_paideia_os_resume`, and `feedback_paideia_as_version_discipline`, the checkpoint pauses are existing protocol. This round *formalises* the unquarantine-verify-resume cadence so the two intermediate checkpoints are first-class steps, not ad-hoc breaks.

### 1.1 Per-issue cadence (between checkpoints)

```
softarch  → produces the issue body (PA7-completion template, §5)
workerbee → implements + cargo test --workspace
debugger  → triaged if cargo red, OR if test count drops,
            OR if SARIF golden mismatch after intentional catalog change
if cargo green AND test count strictly grew:
  commit + push to main         (PaideiaOS-mode no-PR workflow, per Phase 4 §3)
  gh issue close <n>
  → immediately pick up next issue; no pause
```

### 1.2 Checkpoint 1 — G1 + G2 close

**Trigger:** the last of {G1 symbol export rework, G2 byte-position unification} closes.

**Verification ritual:**

```bash
# In paideia-as (no-op; already green):
cargo test --workspace

# In paideia-os (the cross-repo verify):
cd ../paideia-os
git submodule update --remote paideia-as
# Unquarantine the 4 G2-blocked files:
git mv .quarantine/src/kernel/kernel_main.pdx src/kernel/kernel_main.pdx
git mv .quarantine/src/kernel/exceptions.pdx  src/kernel/exceptions.pdx
git mv .quarantine/src/kernel/idt.pdx         src/kernel/idt.pdx
git mv .quarantine/src/kernel/pt_walk.pdx     src/kernel/pt_walk.pdx
./tools/build.sh
# If clean → commit the unquarantine; otherwise file a paideia-as
# follow-up issue and resume the paideia-as loop on it.
```

If `./tools/build.sh` returns 0 and the resulting `kernel.elf` is non-empty, **the unquarantine commit lands on the paideia-os side** with the message form in §8.3. paideia-as then resumes the autonomous loop on G3.

If `./tools/build.sh` is red, the failure mode is one of three:
1. **A G1/G2 acceptance gap** — file a new paideia-as issue under the round's milestone, label `gap:byte-emit`, and resume paideia-as with it as the next issue. Do not unquarantine yet.
2. **A new gap not anticipated in G1–G15** — file a new issue with label `gap:byte-emit` or `gap:parser-surface` as appropriate; add to the round's milestone; resume.
3. **A paideia-os-side stale artifact** — clean the paideia-os build tree, retry. If still red, file under (1) or (2).

### 1.3 Checkpoint 2 — G3 + G9 + G10 close

**Trigger:** the last of {G3 unsafe-body lowering, G9 block-tail relaxation, G10 array-index l-value} closes.

**Verification ritual:**

```bash
# In paideia-os:
cd ../paideia-os
git submodule update --remote paideia-as

# Unquarantine the remaining 9 files (osarch plan §G3/G9/G10 names the exact set):
for f in $(ls .quarantine/src/kernel/); do
  git mv ".quarantine/src/kernel/$f" "src/kernel/$f"
done
./tools/build.sh

# If clean: delete the stubs as a SEPARATE commit + re-verify.
git rm tools/stubs.S
# Remove the linker glue that references stubs.o (if any):
#   tools/build.sh and/or src/kernel/linker.ld — osarch plan G1 names exact paths.
./tools/build.sh                                # MUST still be green
```

If `./tools/build.sh` after the stubs.S deletion is red, the most likely cause is that G1 symbol export is producing the wrong symbol binding (LOCAL vs GLOBAL) or the wrong section. File a P0 issue against paideia-as labelled `gap:byte-emit` + `priority:P0`; halt the loop until resolved; do not leave paideia-os in a half-unquarantined state on `main`. The fallback is `git revert` of the stubs.S deletion commit so paideia-os main stays bootable while the paideia-as fix is in flight.

### 1.4 Round close

**Trigger:** all selected G-issues closed (G1–G10 mandatory; G11–G15 to the extent included by osarch).

The closure ritual (§9) bumps paideia-as workspace.version to v0.7.0, tags, bumps the paideia-os submodule pin, and resumes the paideia-os autonomous loop on the R6.5+ / D7+ reactivation backlog. The substrate now actually emits real bytes; the reactivation loop can proceed past the points where it previously stalled.

### 1.5 Stop conditions summary

| Stop kind | When | Action |
|---|---|---|
| Per-issue | After commit + push + close | Pick up next issue immediately. No pause. |
| Checkpoint 1 | G1 + G2 closed | Run §1.2; commit unquarantine batch on paideia-os; resume paideia-as on G3. |
| Checkpoint 2 | G3 + G9 + G10 closed | Run §1.3; commit second unquarantine batch + stubs.S deletion; resume on remaining. |
| Round close | All selected G-issues closed | Run §9 ritual; resume paideia-os on R6.5+/D7+. This is the user-visible terminal stop. |

The two intermediate checkpoints are the new shape this round introduces — *forced cross-repo verification points* keyed to the two byte-emit batches. The standard continuous tempo (no pause between issues) is otherwise unchanged; cf. Phase 5 / Phase 6 which paused only at phase close.

---

## 2. PR sizing discipline

PA7-completion has tighter size bands than Phase 5/6. The gaps are surgical; there is no L-band work this round. If osarch authors an issue that projects above 400 LOC net diff, this softarch plan rejects it — osarch must split before workerbee picks up.

### 2.1 Size bands (this round)

| Size | Net diff (LOC, ex. generated, ex. corpus, ex. snapshots) | Examples | Files touched | Review target |
|---|---|---|---|---|
| **XS** | < 50 | G4 unary `~`, G5 keyword removal, G6 optional `->`, G8 cast operator (single-token grammar additions) | ≤ 3 | ≤ 10 min |
| **S** | 50–200 | G1 symbol export rework, G2 byte-position unification, G7 sized-int routing, G9 block-tail relaxation, G10 array-index l-value | ≤ 6 | ≤ 25 min |
| **M** | 200–400 | G3 unsafe-body lowering (large because it wires many encoders), G11 long-mode primitives group (split if > 400) | ≤ 12 | ≤ 45 min |
| **L** | — | **forbidden this round — must be split** | — | — |

Generated code, test corpora, and `Cargo.lock` lines are excluded from the LOC count. SARIF snapshot regen output is excluded.

### 2.2 Hard-split triggers (this round)

A single issue must split if any of: (1) net diff projects above 400 LOC; (2) it touches both an operand parser AND an encoder bridge (split parser-refactor PR first, encoder-bridge PR second — same as Phase 6 §2.4 trigger 7); (3) it activates more than one Phase-4 surface construct; (4) it mixes byte-emit fix (G1–G3) with parser-surface work (G4–G10) — different `gap:*` labels, different test surfaces; (5) it mixes any G1–G10 work with G11–G15 anticipated primitives (stretch items live behind §4.3's gate and must not be co-mingled with mandatory work).

### 2.3 No-PR mode reminder

Per Phase 4 retrospective §3 (carried through Phase 5 and Phase 6): under PaideiaOS-mode, work lands as direct pushes to `main` after `cargo test --workspace` green. The size bands still apply — the "PR" is the squash-merge commit. A single commit that exceeds 400 LOC net is forbidden the same way an XL PR would be in PR mode.

---

## 3. GitHub label additions for the round

PA7-completion adds **five new labels**. The Phase 5 and Phase 6 cross-cutting labels (`gated:downstream-paideia-os`, `area:walker-activation`, `area:bug-fix-from-paideia-os`, `area:emit-activation`, `area:boot-intrinsics`) remain in active use where applicable. The `phase:N` purple convention is retained for round labelling; the `gap:*` labels are new this round and provide the orthogonal classification by gap-family.

| Label | Color | Description |
|---|---|---|
| `pa7-completion` | `#5319E7` (purple) | Every issue in this round. Closes when §4.3 round-close criterion is met. |
| `unblocks-paideia-os` | `#FBC02D` (yellow) | Closure of this issue unquarantines one or more specific paideia-os files. The issue body MUST list the files under `## Unblocks paideia-os file(s)`. |
| `gap:byte-emit` | `#D32F2F` (red) | The G1/G2/G3 family — the original PA7 incompleteness (symbol export, byte-position unification, unsafe-body lowering). |
| `gap:parser-surface` | `#1976D2` (blue) | The G4–G10 family — parser / lexer / grammar additions surfaced by paideia-os integration (unary `~`, keyword removal, optional `->`, sized-int routing, `as` cast, block-tail relaxation, array-index l-value). |
| `gap:anticipated` | `#388E3C` (green) | The G11–G15 family — stretch primitives anticipated by osarch but not surfaced by the current integration. May defer to v0.8. Gated behind §4.3 budget check. |

### 3.1 Label combinations

Every PA7-completion issue carries:

- `pa7-completion`.
- Exactly one of `gap:byte-emit`, `gap:parser-surface`, `gap:anticipated`.
- `unblocks-paideia-os` if and only if closing it unquarantines one or more paideia-os files. (G11–G15 typically do *not* carry this label.)
- One `area:*` (crate-level, from the existing 18 `area:*` labels).
- One `type:*`.
- One `priority:*`.

### 3.2 Carryover labels

`gated:downstream-paideia-os` (every `unblocks-paideia-os` issue also carries this), `area:emit-activation` (G1–G3 glue), `area:walker-activation` (G3 + G9 + G10 walker chain), `area:boot-intrinsics` (G11–G15 long-mode primitives).

### 3.3 Creation

The five labels are created via `gh label create` against `paideia-os/paideia-as` as the first commit of the round (before G1 lands). The osarch plan's G1 issue body MUST enumerate them verbatim with the colors and descriptions above.

```bash
gh label create pa7-completion         --color 5319E7 --description "PA7-completion round — see .plans/pa7-completion-osarch-plan.md"
gh label create unblocks-paideia-os    --color FBC02D --description "Closure unquarantines specific paideia-os files; body lists them under '## Unblocks paideia-os file(s)'"
gh label create gap:byte-emit          --color D32F2F --description "G1/G2/G3 family — original PA7 byte-emit incompleteness"
gh label create gap:parser-surface     --color 1976D2 --description "G4–G10 family — parser/lexer/grammar additions surfaced by paideia-os integration"
gh label create gap:anticipated        --color 388E3C --description "G11–G15 family — anticipated stretch primitives; may defer to v0.8"
```

---

## 4. Milestone shape

### 4.1 Naming

| Convention | Value |
|---|---|
| Single milestone slug | `pa7-completion` |
| GitHub milestone title | `PA7 Completion (byte-emit + parser-surface + anticipated)` |
| Description | "See `.plans/pa7-completion-osarch-plan.md`. Closes when §4.3 criterion is met." |
| Close trigger | All selected issues closed AND round-close ritual (§9) completed. |

One milestone for the whole round. The G1–G15 numbering provides the within-milestone sequencing; the `gap:*` labels provide the orthogonal classification. There is no per-family submilestone because the cross-repo verification cadence is keyed to the two checkpoints (§1.2, §1.3), not to label groupings.

### 4.2 Within-milestone sequencing

osarch owns the order. The softarch invariants the order must respect:

1. G1 lands before G2 lands before checkpoint-1 verification (§1.2).
2. G3, G9, G10 all land before checkpoint-2 verification (§1.3). Order among the three is osarch's call.
3. G4–G8 may land at any point before round close; they are independent of the checkpoints.
4. G11–G15 land only after G1–G10 are all closed (per §4.3 budget rule).

### 4.3 Round-close criterion (the bounded stop point)

The autonomous loop stops *only* when all of:

| # | Criterion | How verified |
|---|---|---|
| 1 | All G1–G10 issues closed. | `gh issue list --label pa7-completion --label gap:byte-emit --state open` AND `gh issue list --label pa7-completion --label gap:parser-surface --state open` both empty. |
| 2 | All 13 paideia-os files unquarantined and on `main`. | `cd ../paideia-os && ls .quarantine/src/kernel/ 2>/dev/null | wc -l` returns 0 (or `.quarantine/` is absent entirely). |
| 3 | `tools/stubs.S` deleted from paideia-os main. | `cd ../paideia-os && test ! -f tools/stubs.S`. |
| 4 | paideia-os `./tools/build.sh` produces a non-empty `kernel.elf` against the bumped submodule pin. | `cd ../paideia-os && ./tools/build.sh && test -s build/kernel.elf`. |
| 5 | `paideia-as --version` reports `0.7.0`. | `paideia-as --version \| grep -q '^paideia-as 0.7.0$'`. |
| 6 | Tag `v0.7.0` pushed. | `git tag --list v0.7.0 && git ls-remote --tags origin v0.7.0`. |
| 7 | `CHANGELOG.md` `## v0.7.0 — PA7 completion (byte-emit + parser-surface + anticipated)` section landed. | `grep -q '^## v0.7.0 — PA7 completion' CHANGELOG.md`. |
| 8 | `design/toolchain/phase-transition-7.md` retrospective written. | File exists; references each per-issue design note from §10. |

G11–G15 (the `gap:anticipated` stretch) are *not* gating: the round can close with some or all of them open and re-classified as `phase:8` for the next round. The bounded stop is criteria 1–8 above. If all G1–G10 are closed AND criteria 2–8 are met, the round closes regardless of G11–G15 state.

### 4.4 The stretch budget rule

G11–G15 land *only* if all of:

- G1–G10 are closed.
- The round's running net-LOC budget is below 2000 lines total (rough heuristic to avoid scope creep).
- Each G11–G15 issue still satisfies §2.1 size bands and §2.2 split triggers.

If the budget is exceeded, the remaining G11–G15 issues are re-labelled `phase:8` (label to be created at Phase 8 open) and the round closes on G1–G10 alone. This is the explicit scope-creep guard.

---

## 5. Issue body template

PA7-completion inherits the Phase 6 template (per `.plans/phase-6-softarch.md` §5) and adds one required field. The Phase 6 fields (`## Surfaced by paideia-os`, `## Unblocks paideia-os`, `## Cross-repo escalation source`) all stay. The new field `## Unblocks paideia-os file(s)` is mandatory for any issue carrying the `unblocks-paideia-os` label.

```markdown
---
name: PA7 Completion Task
about: A unit of PA7-completion work that fits a single XS/S/M PR (per §2). No L this round.
title: '[area] short imperative summary'
labels: 'pa7-completion'
assignees: snunezcr
---

## Summary
<one paragraph, ≤ 3 sentences>

## Gap family
<one of: gap:byte-emit | gap:parser-surface | gap:anticipated>

## Pillar / decision impact
<which of pillars 1–11 and decisions Q1–Q15 / Q-A1–Q-A10 this touches; "none" is valid>

## Acceptance criteria
- [ ] …
- [ ] cargo test --workspace green
- [ ] Test count strictly grew (record old → new)
- [ ] SARIF snapshot regenerated if catalog.toml touched (G4, G7, G8, G9, G10 family — see §7)
- [ ] Definition of done is an OBSERVABLE TEST — not "compiles". Name the test (module::test_name).
- [ ] Linked design-doc note under design/toolchain/

## Files created / modified
<expected paths>

## Dependencies
<links to prerequisite or sibling issues; "none" if standalone>

## Estimated size
<XS / S / M; L is forbidden this round (§2.1)>

## Test plan
<unit / integration / property / snapshot / corpus / golden-byte fixtures>

## Surfaced by                                              ← MANDATORY
<paideia-os integration commit. Default: `paideia-os@d155100`. Override only if a
 later paideia-os commit surfaced a refinement.>

## Unblocks paideia-os file(s)                              ← NEW (this round)
<exact relative paths, one per line (e.g. `.quarantine/src/kernel/kernel_main.pdx`).
 Required for `unblocks-paideia-os`-labelled issues; "none" otherwise (typical for G11–G15).>

## Unblocks paideia-os                                     ← INHERITED (Phase 5/6)
<paideia-os issue numbers this removes a gate from. "none" if internal-only.>

## Cross-repo escalation source                           ← INHERITED (Phase 5/6)
<paideia-os issue + commit SHA. Redundant with `## Surfaced by` this round; keep both
 for grep-continuity with Phase 5/6.>

## Definition of done
<observable test (`crate::module::test_name`) that fails before this change and passes
 after. Not "code compiles". For unquarantine-blocking issues, the de-facto end-to-end
 DoD is "the named paideia-os files build clean under `./tools/build.sh`"; the named
 unit test is the in-paideia-as proxy triggering the same code path.>

## Notes
<free-form>
```

### 5.1 Commit-message template (PaideiaOS-mode direct push)

Per §1.1, PA7-completion work lands as direct pushes. The commit message is the audit trail:

```
[pa7c-<NNN>] <short imperative>

Closes #<n>
Round: pa7-completion
Gap family: <byte-emit|parser-surface|anticipated>
Size: <XS|S|M>
Test count: <old> -> <new>
SARIF regen: <yes|n/a>
Surfaced by: paideia-os@d155100
Unblocks paideia-os file(s): <list or none>
Fix-test: <module::test_name>

<body>

Co-Authored-By: workerbee <noreply@anthropic.com>
```

`Test count`, `SARIF regen`, `Surfaced by`, `Unblocks paideia-os file(s)`, and `Fix-test` lines are mandatory. Their absence is grep-able for the round-close audit.

---

## 6. cargo-green gate

The cargo-green gate is the only mechanical reviewer in PaideiaOS-mode. PA7-completion inherits Phase 6's gate (including §6.6 fix-test transition discipline) and adds the cross-repo canary requirement.

### 6.1 Per-issue requirements

Inherits Phase 6 §6.1 verbatim. Additions / restatements this round:

| Check | Tool | Failure policy |
|---|---|---|
| Test count strictly grows | Workerbee awk pipeline (§6.3) | A flat count is a P0 smell; a drop is a P0 bug. |
| SARIF snapshot regen | `cargo insta review` if `catalog.toml` touched | Mandatory. See §7. |
| Boot orchestration smoke | `cargo test -p paideia-as-build --test boot_orchestration` (the PA7-009 smoke) | Must stay green every commit. |
| Cross-repo canary | `paideia_os_phase1_rebuild` (osarch G1 wires the exact invocation) | Must stay green every commit on either repo. |
| Fix-test transition | Per Phase 6 §6.6 (fail-before / pass-after) | Every PA7-completion issue names a fix-test. |

### 6.2 Baseline at PA7 close

| Metric | Value at PA7 close (entering PA7-completion) |
|---|---|
| Workspace test count | **2651** |
| paideia-as version | 0.6.0 |
| paideia-os quarantined files | 13 (under `.quarantine/src/kernel/`) |
| paideia-os stub workaround | `tools/stubs.S` (hand-written `ret` bodies for `uart_init`/`uart_puts`) |

PA7-completion invariant: the test count starts at 2651 and only grows. Any commit that produces a count below the previous commit's count is reverted on sight. Each G-issue lands its own fixture; the new-fixtures-per-issue cadence is the test-count-growth source.

### 6.3 Test-count counting discipline (carried from Phase 5/6)

The only valid count comes from the explicit pipeline:

```bash
cargo test --workspace 2>&1 \
  | awk '/^test result:/ { ok+=$4; fail+=$6; ignored+=$8 } END { print ok+fail+ignored }'
```

Workerbee prompts that ask for a test count MUST embed this pipeline. Any other counting method reports a *partial* count and is grounds for re-running.

### 6.4 P0 escalations

Inherits Phase 6 §6.5. New this round: `boot_orchestration` smoke red (the PA7-009 smoke; its red state means the byte-emit story regressed) and `paideia_os_phase1_rebuild` canary red on either repo (cross-repo contract broken — halt both loops).

### 6.5 New-fixtures-per-issue requirement

Every issue lands at least one new fixture alongside the source change. `gap:byte-emit`: golden-byte fixture (`.pdx` source + expected byte sequence + emit assertion). `gap:parser-surface`: parse-tree fixture (`.pdx` source + AST shape + snapshot under `crates/paideia-as-syntax/tests/snapshots/`); add a SARIF fixture if diagnostics change (§7). `gap:anticipated`: golden-byte fixture for the primitive in isolation plus a paideia-os-side smoke if applicable. An issue without a named fixture is rejected at the softarch authoring step.

---

## 7. SARIF regen discipline

G4 (unary `~`), G7 (sized ints), G8 (`as` casts), G9 (block-tail), and G10 (array-index l-value) all add or change diagnostics. Per memory `feedback_paideia_as_version_discipline` and the Phase 6 §6.3 SARIF discipline:

### 7.1 Issues that MUST regen SARIF

| Issue | Reason |
|---|---|
| G4 unary `~` | New parser path → new diagnostic for "expected operand after `~`" + the lexer's handling of `~` near comments. |
| G7 sized-int routing | New diagnostic for sized-int overflow + the routing decision tree. |
| G8 cast operator | New `as` keyword + diagnostic for invalid casts. |
| G9 block-tail relaxation | Changed diagnostic for "block-tail must be expression" → softened to allow unit-typed contexts. |
| G10 array-index l-value | New diagnostic surface for array-index l-value (or relaxation of the existing "not assignable" diagnostic). |

### 7.2 Regen ritual (per memory `feedback_paideia_as_version_discipline`)

```bash
# After the catalog.toml edit lands in the working tree:
cargo insta review                                # accept the new snapshots
# Verify no .snap.new files remain:
test -z "$(find crates/paideia-as-diagnostics/tests/snapshots -name '*.snap.new')"
# Commit catalog.toml + .snap files in the SAME commit as the source change.
```

The single-commit rule is load-bearing: a `.snap.new` file left in the tree is a process violation that surfaces as a CI red on the next PR; it is cheaper to catch in-commit.

### 7.3 Workerbee prompt preamble (mandatory phrasing)

Per Phase 6 §6.7, every workerbee prompt that touches `catalog.toml` includes verbatim:

```
SARIF REGEN MANDATORY if catalog touched.
No .snap.new files in crates/paideia-as-diagnostics/tests/snapshots/ after commit.
FIX-TEST MANDATORY:
  - name the test in the AC (module::test_name)
  - verify FAIL on parent commit
  - verify PASS on this commit
```

The softarch agent inserts this preamble when emitting the workerbee handoff.

### 7.4 Round-close final regen

The §9 round-close ritual includes one final `cargo insta review` pass against `main` to catch any drift that slipped through. If the final pass produces diffs, those diffs are committed as `[pa7c-final] SARIF round-close regen` before the v0.7.0 tag.

---

## 8. Unquarantine protocol

The cross-repo mechanical step new to this round. Each of the 13 paideia-os files leaves `.quarantine/` via a deterministic, scriptable sequence.

### 8.1 Per-file protocol (5 steps)

```bash
# Step 1: Move the file back, preserving git history.
cd ../paideia-os
git mv .quarantine/src/kernel/PATH src/kernel/PATH

# Step 2: Single-file paideia-as check (fast path — exits 0 if the unquarantined
# file's surface compiles cleanly against the bumped submodule pin).
./tools/paideia-as/target/release/paideia-as check src/kernel/PATH

# Step 3: Full kernel build — must produce kernel.elf without using tools/stubs.S
# for symbols that the unquarantined file is supposed to provide.
./tools/build.sh

# Step 4: Commit the per-file unquarantine (or per-checkpoint batch — see §8.2).
git add src/kernel/PATH .quarantine/  # the .quarantine/ del is part of the mv
git commit -m "$(cat <<'EOF'
unquarantine: src/kernel/PATH

Re-enabled after paideia-as PA7-completion #<issue> closed.
Build verified: tools/build.sh produces kernel.elf cleanly.

Co-Authored-By: workerbee <noreply@anthropic.com>
EOF
)"

# Step 5: After all 13 files return AND tools/stubs.S is no longer referenced
# by any unquarantined file's symbol resolution, delete it as a SEPARATE commit
# and re-verify.
git rm tools/stubs.S
# Also remove any linker-script glue or build.sh lines that reference stubs.o
# (osarch plan G1 names the exact paths).
./tools/build.sh                                  # MUST still be green
git add tools/build.sh src/kernel/linker.ld 2>/dev/null
git commit -m "$(cat <<'EOF'
remove tools/stubs.S — real symbol exports now come from paideia-as

The hand-written ret bodies for uart_init/uart_puts in tools/stubs.S were
a PA7-era workaround for missing top-level symbol exports from paideia-as.
With pa7-completion G1 closed, real symbols now resolve from the .o files
produced by paideia-as. Build verified: tools/build.sh still produces
kernel.elf cleanly with no unresolved relocations.

Co-Authored-By: workerbee <noreply@anthropic.com>
EOF
)"
```

That is five steps total. Step 1 is the move, step 2 is the per-file check, step 3 is the full build, step 4 is the commit, step 5 is the stubs.S deletion + re-verify. Steps 1–4 run once per file (or per batch — see §8.2); step 5 runs exactly once at checkpoint 2.

### 8.2 Per-batch vs per-file commit choice

Default: per-checkpoint batch (one commit per checkpoint, cleaner history, matches §1.2 / §1.3 verification). Fallback to per-file commits when a batch is red and bisecting is needed (e.g., file B depends on file A's symbol resolution; land A first, verify green, then B).

### 8.3 Batch commit-message template

```
unquarantine: <checkpoint name> batch (<N> files)

Re-enabled after paideia-as PA7-completion #<G1>, #<G2> closed.
Build verified: tools/build.sh produces kernel.elf cleanly.

Files:
  - src/kernel/kernel_main.pdx
  - src/kernel/exceptions.pdx
  - src/kernel/idt.pdx
  - src/kernel/pt_walk.pdx

Co-Authored-By: workerbee <noreply@anthropic.com>
```

### 8.4 Rollback procedure

If step 3 (`./tools/build.sh`) is red: do not commit; `git checkout src/kernel/PATH && git checkout .quarantine/` to restore both sides (the `git mv` is fully reversible because `.quarantine/` is preserved); file a new paideia-as issue under the round milestone with the build error attached (label `gap:byte-emit` or `gap:parser-surface`); resume the paideia-as autonomous loop on the new issue; re-try the unquarantine after it closes.

---

## 9. Closure rituals

Three rituals: per-issue, per-checkpoint, round-close.

### 9.1 Per-issue ritual

```
commit + push to paideia-as main         (PaideiaOS-mode no-PR workflow)
gh issue close <n>
→ immediately pick up next issue
```

No design-doc requirement per-issue beyond the `## Notes` field in the issue body and any inline `design/toolchain/<topic>.md` note the issue's AC names. The roll-up is at round close (§9.3 / §10).

### 9.2 Per-checkpoint ritual (twice in the round)

**Checkpoint 1 (after G1 + G2):**

1. paideia-as: confirm cargo green, test count grew, both issues closed.
2. paideia-os: run §1.2 verification ritual.
3. paideia-os: commit the 4-file unquarantine batch per §8.3.
4. paideia-os: push to main.
5. paideia-as: resume autonomous loop on G3.

**Checkpoint 2 (after G3 + G9 + G10):**

1. paideia-as: confirm cargo green, test count grew, all three issues closed.
2. paideia-os: run §1.3 verification ritual.
3. paideia-os: commit the 9-file unquarantine batch per §8.3.
4. paideia-os: run §8.1 step 5 — delete `tools/stubs.S` as a separate commit + re-verify.
5. paideia-os: push both commits to main.
6. paideia-as: resume autonomous loop on remaining G4–G10 (and G11–G15 if budget allows).

### 9.3 Round-close ritual

When §4.3 criteria 1, 5–8 are met (criteria 2–4 land as a consequence of checkpoint 2):

```bash
# In paideia-as:
# 1. workspace.version: 0.6.0 -> 0.7.0 in Cargo.toml
cargo update --workspace

# 2. CHANGELOG.md: add "## v0.7.0 — PA7 completion (byte-emit + parser-surface + anticipated)"
#    Include: closed G-issues, test-count delta (2651 -> N), SARIF additions,
#    unquarantine summary, stubs.S deletion.

# 3. Final SARIF regen pass.
cargo insta review
test -z "$(find crates/paideia-as-diagnostics/tests/snapshots -name '*.snap.new')"

# 4. design/toolchain/phase-transition-7.md retrospective (shape per §10.2).

# 5. Commit + tag.
git add Cargo.toml Cargo.lock CHANGELOG.md design/toolchain/phase-transition-7.md \
        crates/paideia-as-diagnostics/tests/snapshots/
git commit -m "Release v0.7.0 — PA7 completion"
git tag v0.7.0
git push origin main v0.7.0

# 6. find-paideia-as.sh strict re-verify (memory: feedback_paideia_as_version_discipline).
./scripts/find-paideia-as.sh

# In paideia-os:
cd ../paideia-os
git submodule update --remote paideia-as
git add paideia-as
git commit -m "Bump paideia-as submodule to v0.7.0 (PA7 completion)"
git push origin main
# Resume paideia-os loop on R6.5+/D7+. The substrate now actually emits real bytes.
```

Per memory `feedback_paideia_os_no_cicd`: paideia-os never runs Actions; the round-close paideia-os-side verification is local `./tools/build.sh` + the pre-push hook. The submodule bump commit is the cross-repo handshake.

### 9.4 The handshake — bidirectional

Per memory `feedback_cross_repo_escalation`: `v0.7.0` tag (producer closure) → submodule bump (consumer ack) → paideia-os R6.5+/D7+ resume (consumer proof). Each edge is grep-able forever. This is the third such handshake in the cohort (Phase 5 was #1, Phase 6 was #2); future rounds reuse this shape verbatim.

---

## 10. Documentation discipline

Per standing memory `project_design_directory`: every architectural choice lands in `design/`. PA7-completion has two named architectural decisions worth highlighting; the round close adds a single retrospective.

### 10.1 Per-issue design notes

Each G-issue's AC includes a `design/toolchain/<topic>.md` reference. Two architectural decisions are pre-named (osarch picks the exact filename):

- **G2: "EmitWalker owns byte position; encoder reads from it"** (likely `design/toolchain/emitwalker-byte-position.md`). Records the seam where parsed-operand byte-position metadata flows into the encoder, the previous duplication (encoder + emitter both computing), and the post-fix single-source-of-truth invariant.
- **G9: "Block in unit-typed context auto-synthesises ()"** (likely `design/toolchain/block-tail-unit-synth.md`). Records the type-context check, the synth point, and the diagnostic relaxation.

Smaller notes (G4 unary `~`, G7 sized-int routing, etc.) land inline as appendices or one-page `design/toolchain/<gap-name>.md` notes — osarch's choice per issue.

### 10.2 Round-close retrospective

`design/toolchain/phase-transition-7.md` — same shape as `phase-transition-5.md` and `phase-transition-6.md`: §0 scope, §1 carryover disposition, §2 didn't ship (G11–G15 deferred items + reason), §3 got right (two-checkpoint cadence, gap:* label triad, unquarantine protocol), §4 would change, §5 → v0.8 carryover, §6 closing note. Per-issue design notes linked from §3 and §4. Written in the closing commit (§9.3 step 4), not deferred.

### 10.3 Roll-up note

Per memory `project_design_directory`: design notes are mandatory, not optional. A G-issue commit without its named design-doc reference is rejected at the softarch authoring step.

---

## 11. References

- `.plans/pa7-completion-osarch-plan.md` — WHAT companion (osarch). Owns G1–G15 list + per-issue acceptance.
- `.plans/phase-6-softarch.md` — analogous prior softarch doc; this round mirrors its structure.
- `.plans/phase-6-plan.md`, `.plans/phase-5-build-emit-softarch.md`, `.plans/phase-5-build-emit-plan.md` — prior round companions.
- `design/toolchain/phase-transition-5.md`, `design/toolchain/phase-transition-6.md` — prior retrospectives; baseline shape for `phase-transition-7.md`.
- paideia-os commit `d155100` ("fix: make tools/build.sh ... produce a bootable kernel.elf") — surfaced every gap this round; every issue's `## Surfaced by` points here.
- Memories: `feedback_cross_repo_escalation` (protocol), `feedback_phase6_to_paideia_os_resume` (resumption pattern), `feedback_paideia_as_version_discipline` (workspace.version + tag + CHANGELOG triple), `feedback_paideia_os_no_cicd` (local-only verification on paideia-os side), `project_design_directory` (design/ doc rule).

---

*End of document.*
