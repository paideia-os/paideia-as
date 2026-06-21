# paideia-as Phase 6 — Walker Activation & paideia-os Bug Fixes: Process & Governance (softarch)

**Status:** Draft v0.1
**Date:** 2026-06-21
**Owner:** softarch agent (process / governance / cargo-green-gate dimensions)
**Phase scope ceiling:** fix the paideia-os-surfaced gaps (#734, #735, #736 area) and activate the m1-005/006 walker chain for the Phase-4 surface constructs that paideia-os Phase-2 (capability system) requires — enough that the paideia-os Phase-2 unblock-criterion in §7 is met. Full Phase-4-surface walker activation beyond what paideia-os Phase-2 requires is deferred to Phase 7+.
**Sister deliverable:** `.plans/phase-6-plan.md` (osarch) — owns the *what* (milestones, fix-issues, decomposition).

---

## 0. Scope + non-overlap

This doc and the osarch plan partition Phase 6 governance cleanly. The split is the same as Phase 5; this doc carries the discipline forward and adds Phase-6-specific adjustments.

| Dimension | Owner |
|---|---|
| Milestone list, sequencing, per-fix decomposition | osarch (`.plans/phase-6-plan.md`) |
| PR sizing discipline (XS/S/M/L bands, Phase-6 split triggers) | softarch (this doc, §2) |
| GitHub label additions for Phase 6 | softarch (this doc, §3) |
| Milestone shape, naming, closure criterion | softarch (this doc, §4) |
| Issue body template (Phase-6 additions) | softarch (this doc, §5) |
| Autonomous-loop tempo (the explicit Phase-6 continuous-run override) | softarch (this doc, §1) |
| cargo-green gate + SARIF regen + workerbee preamble + fix-test discipline | softarch (this doc, §6) |
| Unblock criterion + downstream submodule bump ritual | softarch (this doc, §7) |
| Cross-repo escalation continuation (Phase 6 as second worked example) | softarch (this doc, §8) |
| Documentation discipline (design/toolchain/*.md per milestone, transition doc) | softarch (this doc, §9) |

Where the two docs overlap by necessity (milestone names, fix-issue references), the osarch plan is the source of truth for the *list* and the *technical content*; this doc is the source of truth for the *shape* (label set, closure criteria, retrospective discipline, tempo).

---

## 1. Tempo (CRITICAL — Phase-6-specific override; same as Phase 5)

**Phase 6 walker-activation + paideia-os-bug-fix work runs continuously across all milestones. There is no per-milestone pause-for-review. The autonomous loop runs until the paideia-os Phase-2 unblock-criterion (§7) is met. This is the same tempo override applied in Phase 5 — both override the older paideia-as-wide "pause after each milestone" rule per memory `feedback_autonomous_tempo.md`. The rationale is identical: Phase 6 is itself the result of a cross-repo escalation (paideia-os Phase-1 closed with stub workarounds for #734/#735/#736 and a missing struct activation; paideia-os Phase-2 cannot start until those are fixed). Pausing inside Phase 6 leaves paideia-os blocked. The loop runs to the cross-repo unblock event, not to a paideia-as-internal review boundary.**

### 1.1 Per-issue cadence

```
softarch  → produces the issue body (Phase-6 template, §5)
workerbee → implements + cargo test --workspace
debugger  → triaged if cargo red, OR if a test count drops,
            OR if the mandatory fix-test (§6.6) doesn't fail-before / pass-after
if cargo green AND fix-test transitions correctly:
  commit + push to main         (PaideiaOS-mode no-PR workflow, per Phase 4 §3)
  gh issue close <n>
```

### 1.2 Per-milestone cadence

```
on last-issue-in-milestone close:
  write design/toolchain/<topic>-phase6.md retrospective (§9)
  immediately pick up next milestone's first issue
  no pause; no review checkpoint
```

### 1.3 Stop condition

The loop stops *only* when the paideia-os Phase-2 unblock-criterion (§7) is met. That is the bounded scope ceiling the user has set. Closure ritual: tag `v0.6.0`, push, bump paideia-os submodule pin, resume paideia-os autonomous loop on the Phase-2 capability-system milestone.

### 1.4 Contrast with prior paideia-as rule and with Phase 5

| Aspect | Older rule (paideia-as default) | Phase 5 override | Phase 6 override |
|---|---|---|---|
| Pause between milestones | Yes, for user review | No | **No** |
| Pause between issues | No (within a milestone) | No | No |
| Pause condition | End-of-milestone | Only at Phase-5 closure | **Only** at Phase-6 closure (§7) |
| Cross-repo driver | n/a | paideia-os Phase-1 blocked | paideia-os Phase-2 blocked |
| Closure event | n/a | `v0.5.0` tag + submodule bump | `v0.6.0` tag + submodule bump |

The older rule remains in effect for Phase 7+ unless a third cross-repo escalation lifts it again. This is the second one-phase exception; the precedent is now established.

---

## 2. PR sizing discipline (Phase-6 nuances)

Same XS/S/M/L bands as `.plans/paideia-as-softarch-plan.md` §1.1 and `.plans/phase-5-build-emit-softarch.md` §2.1. Restated here for in-doc reference, with Phase-6-specific L-split triggers appended.

### 2.1 Size bands (unchanged)

| Band | Net diff (LOC, ex. generated, ex. corpus, ex. snapshots) | Files touched | Test files added/modified | Review target |
|---|---|---|---|---|
| **XS** | ≤ 50 | ≤ 3 | 0–1 | ≤ 10 min |
| **S** | 51–200 | ≤ 6 | ≥ 1 | ≤ 25 min |
| **M** | 201–500 | ≤ 12 | ≥ 1 | ≤ 45 min |
| **L** | 501–1000 | ≤ 20 | ≥ 2 | ≤ 60 min |
| **XL** | > 1000 | — | — | **forbidden — must be split** |

Generated code, test corpora, and `Cargo.lock` lines are excluded from the LOC count. SARIF snapshot regen output is excluded.

### 2.2 General L-split triggers (inherited)

1. The PR touches more than one crate's *public* API.
2. The PR introduces a new dependency in `[workspace.dependencies]`.
3. The PR adds a new crate to the workspace.
4. The PR is the first implementation of a Q-A* decision item.
5. The PR mixes feature work + refactor + test scaffolding (split into three PRs).
6. CI wall-clock projected to exceed 25 min.

### 2.3 Phase-5 inheritances (still in effect)

The three Phase-5 L-split triggers (elaborator chokepoint + encoder, new x86_64 encoding + elaborator wiring, new effect kind + user-code lowering) remain in effect for any Phase-6 work that touches those surfaces.

### 2.4 Phase-6-specific L-split triggers (additional)

These are appended because Phase 6 work concentrates risk along two new axes: the operand-parser / encoder-bridge seam (root cause area of #734/#735/#736), and the walker activation chain for Phase-4 surface constructs that have IR but no end-to-end BUILD path.

7. **Operand parser AND encoder bridge.** An L that touches both the operand parser AND the encoder bridge (the seam where parsed operands are lowered into encoder inputs) must split. The parser change goes first as a refactor PR (no behavioral change to emitted bytes; new parses round-trip identically); the encoder-bridge change follows as a feature PR with golden-byte fixtures.
8. **New effect kind AND user-code lowering.** An L that adds a new effect kind AND uses it in user-code lowering (e.g., for capability-descriptor field access in unsafe blocks) must split. Effect-row declaration PR first (types/effects only, with linearity-corpus entries); user-code-lowering PR second. This restates Phase-5 trigger 9; reasserted here because the Phase-2 capability-system fixes touch this seam.
9. **Phase-4 surface activation for BUILD.** An L that activates a Phase-4 surface construct (records / generics / pattern-match arms / trait impls / enum variants) for the BUILD path must split into per-construct issues. Each Phase-4 construct is a discrete walker-activation unit; bundling them defeats the purpose of incremental walker hookups. The canonical pattern: one issue per surface construct, each landing the walker hookup + a focused fix-test (§6.6).
10. **Struct field-access in unsafe blocks.** Specifically: an L that lands struct definition lowering AND struct field-access lowering AND unsafe-block elision of borrow/region checks must split. This is the precise shape of the missing capability-descriptor activation; the temptation will be to land it as one PR because the constructs co-occur in the paideia-os symptom. Resist: three issues, three commits.

The canonical split pattern (per Phase 1 §1.3): (a) refactor / scaffolding PR (semantically no-op); (b) feature PR consuming the scaffolding; (c) test/corpus PR exercising the feature. Phase 6 adds: (d) fix-test PR (or fix-test embedded in the feature PR) that demonstrably failed before the fix lands (§6.6).

### 2.5 No-PR mode reminder

Per Phase 4 retrospective §3 and Phase 5 §2.4: under PaideiaOS-mode, work lands as direct pushes to `main` after `cargo test --workspace` green. The size bands still apply — the "PR" is the squash-merge commit. A single commit that exceeds 1000 LOC net is forbidden the same as a PR XL.

---

## 3. GitHub label additions for Phase 6

Phase 5 added four labels: `phase:5`, `gated:downstream-paideia-os`, `area:emit-activation`, `area:boot-intrinsics`. The latter three remain in active use whenever Phase-6 work touches their seams (e.g., a Phase-6 fix that also adds a boot intrinsic). Phase 6 adds **three** new labels. Color choices align with the existing palette: `phase:*` is purple; cross-cutting Phase-6 area labels use a distinguishing magenta/yellow to avoid collision with Phase-5 teal/orange.

| Label | Color | Description |
|---|---|---|
| `phase:6` | `#5319E7` (purple) | Phase-6 deliverable per `.plans/phase-6-plan.md`. Closes when the paideia-os Phase-2 unblock-criterion in this doc §7 is met. |
| `area:walker-activation` | `#C2185B` (magenta) | Work activating the m1-005 / m1-006 walker chain for the full Phase-4 surface (records, struct field-access, generics-instantiation, pattern-match arms, trait-impls, enum-variant lowering). Distinct from per-crate `area:elaborator` / `area:lower` because the work spans the walker-hookup chokepoint and tends to span multiple crates per construct. |
| `area:bug-fix-from-paideia-os` | `#FBC02D` (yellow) | Items whose source is a paideia-os-surfaced bug (#734, #735, #736 area, or any subsequent paideia-os escalation against the same root cause). Closure of any issue carrying this label MUST be reflected by a regression test that fails before the fix and passes after (per §6.6). |

The Phase-5 cross-cutting labels stay in scope:

| Label (carried over from Phase 5) | Still in use for Phase 6? |
|---|---|
| `gated:downstream-paideia-os` | Yes — every Phase-6 fix-issue that closes a paideia-os-Phase-2 gate carries this. |
| `area:emit-activation` | Yes when the fix touches the elaborator → encoder → emitter glue. |
| `area:boot-intrinsics` | Yes if a Phase-6 fix incidentally adds a boot-intrinsic (unlikely but possible). |

### 3.1 Label combinations

Every Phase 6 issue carries:

- One `area:*` (crate-level, from the existing 18 `area:*` labels) plus `area:walker-activation` and/or `area:bug-fix-from-paideia-os` if cross-cutting.
- One `type:*`.
- `phase:6`.
- One `priority:*`.
- `gated:downstream-paideia-os` if and only if closing it removes a gate from a paideia-os Phase-2 issue.

### 3.2 Creation

The three labels are created via `gh label create` against `paideia-os/paideia-as` as the first step of the M1 issue in the osarch plan. The osarch plan's M1 issue body must explicitly enumerate the three labels above with the colors and descriptions verbatim, plus reassert that the four Phase-5 labels remain in use.

```bash
gh label create phase:6 --color 5319E7 --description "Phase-6 deliverable per .plans/phase-6-plan.md"
gh label create area:walker-activation --color C2185B --description "Work activating the m1-005/006 walker chain for Phase-4 surface constructs"
gh label create area:bug-fix-from-paideia-os --color FBC02D --description "Source is a paideia-os-surfaced bug; closure requires regression test (fail-before/pass-after)"
```

---

## 4. Milestone shape (Phase 6)

### 4.1 Naming

| Convention | Value |
|---|---|
| Milestone slug | `phase-6-<topic>` (e.g., `phase-6-operand-parser-fixes`, `phase-6-walker-struct-activation`) |
| GitHub milestone title | `Phase 6 — <topic>` |
| Description | One sentence + link: "See `.plans/phase-6-plan.md` §M<N>." |
| Close trigger | All issues in the milestone closed |

The osarch plan owns the milestone *list* and per-milestone *contents*. This doc owns the *shape*: every milestone in Phase 6 follows the naming convention above and the closure rule below.

### 4.2 Per-milestone closure

A milestone closes when:

1. Every issue under the milestone is closed.
2. The corresponding `design/toolchain/<topic>-phase6.md` appendix (§9) is committed.
3. `cargo test --workspace` is green on the closing commit.
4. Every `area:bug-fix-from-paideia-os` issue in the milestone has its named regression test in the suite (§6.6).
5. The STATUS.md narrative section for that milestone is appended.

No human review pause (per §1). The closing commit immediately precedes the first commit of the next milestone.

### 4.3 Phase 6 closure criterion

This is the user's bounded scope ceiling — the explicit stop point for the autonomous loop. All of:

| # | Criterion | How verified |
|---|---|---|
| 1 | All Phase 6 issues closed. | `gh issue list --label phase:6 --state open` returns empty. |
| 2 | The 5 required fixes (per osarch plan: #734, #735, #736, struct-definition-lowering, struct-field-access-in-unsafe) verified by paideia-os-side regression: `tools/build.sh` in paideia-os emits real (non-placeholder) bytes for `entry.pdx` + `long_mode.pdx` + all boot files. | `cd ../paideia-os && tools/build.sh && file build/*.elf | grep -v 'empty'` exits 0. |
| 3 | Boot under qemu shows real instructions disassembling correctly. | `cd ../paideia-os && tools/qemu-smoke.sh` exits 0; serial output contains expected sentinel. |
| 4 | `paideia-as --version` reports `0.6.0`. | `paideia-as --version | grep -q '^paideia-as 0.6.0$'`. |
| 5 | Tag `v0.6.0` pushed. | `git tag --list v0.6.0 && git ls-remote --tags origin v0.6.0`. |
| 6 | `CHANGELOG.md` `## v0.6.0 — Phase 6 (walker activation & paideia-os bug fixes)` section landed. | `grep -q '^## v0.6.0 — Phase 6' CHANGELOG.md`. |
| 7 | `STATUS.md` Phase 6 section appended. | Diff against pre-Phase-6 STATUS.md must include a `## Phase 6 closed` section. |
| 8 | `design/toolchain/phase-transition-6.md` retrospective written. | File exists; references each Phase-6 milestone retrospective from §9. |

When all eight are true, the loop stops, the user is notified, and the cross-repo submodule-bump ritual (§7.2) begins.

---

## 5. Issue body template

Phase 6 inherits the Phase 5 template (per `.plans/phase-5-build-emit-softarch.md` §5) and adds two required fields. The Phase 5 template's two additions (`## Unblocks paideia-os`, `## Cross-repo escalation source`) stay; Phase 6 adds `## Surfaced by paideia-os` (mandatory for `area:bug-fix-from-paideia-os` issues) and reshapes `## Unblocks paideia-os` to point at paideia-os Phase-2 issues specifically.

```markdown
---
name: Phase 6 Task
about: A unit of Phase-6 walker-activation / paideia-os-bug-fix work that fits a single PR (XS/S/M/L per §2).
title: '[area] short imperative summary'
labels: 'phase:6'
assignees: snunezcr
---

## Summary
<one paragraph, ≤ 3 sentences>

## Pillar / decision impact
<which of pillars 1–11 and decisions Q1–Q15 / Q-A1–Q-A10 this touches; "none" is valid>

## Acceptance criteria
- [ ] …
- [ ] cargo test --workspace green
- [ ] Test count strictly grew (record old → new)
- [ ] SARIF snapshot regenerated if catalog.toml touched
- [ ] Linked design-doc reference in PaideiaOS or paideia-as repo
- [ ] Named regression test exists (REQUIRED for area:bug-fix-from-paideia-os): test name = <module::test_name>, fails on parent commit, passes on this commit

## Files created / modified
<expected paths>

## Dependencies
<links to prerequisite or sibling issues; "none" if standalone>

## Estimated size
<XS / S / M / L; if L, justify why not split AND show which §2.3/§2.4 trigger does not apply>

## Test plan
<unit / integration / property / snapshot / corpus / golden-byte fixtures>

## Surfaced by paideia-os                                  ← NEW (Phase 6)
<link to the paideia-os issue OR the paideia-os commit SHA that surfaced this bug.
 Format: `paideia-os/paideia-os#NNN` or `paideia-os@<sha>` (e.g., `paideia-os@b6da03a`).
 Required for `area:bug-fix-from-paideia-os` issues.
 "n/a" if this is a walker-activation issue not tied to a specific paideia-os symptom.>

## Unblocks paideia-os                                     ← INHERITED (Phase 5; Phase 6 narrows to Phase-2)
<list of paideia-os Phase-2 issue numbers this removes a gate on.
 Phase 6 specifically targets paideia-os Phase-2 (capability system); list those issues.
 "none" if this issue is internal-only.>

## Cross-repo escalation source                           ← INHERITED (Phase 5)
<populated only if this issue was filed in response to a paideia-os symptom (i.e., area:bug-fix-from-paideia-os).
 Format: link to the paideia-os issue + commit SHA that surfaced the gap.
 Redundant with `## Surfaced by paideia-os` for Phase 6; keep both for grep-continuity with Phase 5.
 "none" if this issue was Phase-6-internal (e.g., follow-up to another Phase-6 issue).>

## Notes
<free-form>
```

The new field makes the paideia-os → paideia-as escalation graph cleanly greppable. `gh issue list --label area:bug-fix-from-paideia-os --state closed` returns the verified fix set; per-issue `## Surfaced by paideia-os` lets future-us trace each fix back to the paideia-os symptom that motivated it.

### 5.1 Commit-message template (PaideiaOS-mode direct push)

Per §1.1, Phase 6 work lands as direct pushes. The commit message is the audit trail:

```
[phase-6-<milestone>-<NNN>] <short imperative>

Closes #<n>
Milestone: phase-6-<topic>
Size: <XS|S|M|L>
Test count: <old> -> <new>
SARIF regen: <yes|n/a>
Surfaced by paideia-os: <link or n/a>
Unblocks paideia-os: <list or none>
Fix-test: <module::test_name or n/a>

<body>

Co-Authored-By: workerbee <noreply@anthropic.com>
```

`Test count`, `SARIF regen`, `Surfaced by paideia-os`, and `Fix-test` lines are mandatory; their absence is grep-able for monthly self-audit.

---

## 6. cargo-green gate

The cargo-green gate is the only mechanical reviewer in PaideiaOS-mode. Phase 6 inherits Phase 5's gate and tightens it with a new fix-test invariant.

### 6.1 Per-issue requirements

| Check | Tool | Failure policy |
|---|---|---|
| Test suite | `cargo test --workspace` | Blocking — no push without green. |
| Test count growth | Workerbee awk pipeline (per §6.4) | Test count *must strictly grow* per issue. A flat count is a P0 smell. |
| Test count regression | Same | A *drop* in test count is a P0 bug. Halt the loop; spawn debugger. |
| SARIF snapshot regen | `cargo insta review` if `crates/paideia-as-diagnostics/catalog.toml` touched | Mandatory. A missed regen surfaces as a CI red on the next PR; it is cheaper to catch in-PR. |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | Blocking. |
| Format | `cargo fmt --all -- --check` | Blocking (pre-push hook enforces). |
| Fix-test transition (NEW) | Per §6.6 | Blocking for `area:bug-fix-from-paideia-os` issues. |

### 6.2 Baseline at Phase-5 close

| Metric | Value at Phase-5 close (2026-06-20 → 2026-06-21, tag `v0.5.0`) |
|---|---|
| Workspace test count | **2419** |
| Crates | as of Phase-5 close (per `Cargo.toml` workspace members) |
| Diagnostic codes | Phase-5 additions per `design/toolchain/phase-transition-5.md` |
| paideia-as version | 0.5.0 |

Phase 6 invariant: the test count starts at 2419 and only grows. Any commit that produces a count below the previous commit's count is reverted on sight.

### 6.3 SARIF regen discipline (carried from Phase 5)

The Phase 5 enforcement chain remains in force:

1. **Workerbee prompt preamble** (mandatory): every workerbee prompt that touches `catalog.toml` includes the literal string `SARIF REGEN MANDATORY if catalog touched` in the prompt body. The softarch agent inserts this when emitting the workerbee handoff. **Restated for Phase 6: this is non-negotiable. The phrase appears verbatim in every Phase-6 workerbee prompt that authors or modifies a diagnostic code.**
2. **Pre-commit hook**: `.githooks/pre-commit` checks for `git diff --cached --name-only | grep -q catalog.toml` and, if so, runs the SARIF regen + verifies no further `git diff` afterward.
3. **Commit-message check** (manual, monthly audit): grep commits for `SARIF regen: n/a` paired with a `catalog.toml` diff is a process violation.

### 6.4 Test-count counting discipline (carried from Phase 5)

The only valid count comes from the explicit pipeline:

```bash
cargo test --workspace 2>&1 \
  | awk '/^test result:/ { ok+=$4; fail+=$6; ignored+=$8 } END { print ok+fail+ignored }'
```

Workerbee prompts that ask for a test count MUST embed this pipeline. Any other counting method (e.g., `--lib`, `-p <crate>`) reports a *partial* count and is grounds for re-running.

### 6.5 P0 escalations

| Symptom | Class | Response |
|---|---|---|
| Test count drops | P0 bug | Halt loop; spawn debugger; do not push until green AND count restored. |
| `cargo test --workspace` red | P0 bug | Halt; spawn debugger. |
| SARIF golden mismatch after regen (catalog drift) | P0 bug | Halt; the catalog change must be intentional + reflected in `## CHANGELOG`. |
| Clippy red on `main` | P0 bug | Revert the offending commit immediately. |
| Fix-test does not transition (fail-before / pass-after) | P0 bug (NEW) | Halt; the fix is not yet proven. See §6.6. |

### 6.6 Fix-test transition discipline (NEW for Phase 6)

**Every Phase-6 fix-issue under milestones M1, M2, M3 (i.e., every issue carrying `area:bug-fix-from-paideia-os`) MUST have a unit test that fails before the fix lands and passes after. The osarch plan's per-issue Acceptance Criteria MUST name the test (module path + test name). The workerbee prompt MUST verify the transition empirically:**

```bash
# 1. Land the fix-test alone (no production code change) — must FAIL.
#    Verify failure by name:
cargo test --workspace -- --exact <module::test_name> 2>&1 | grep -q 'FAILED'

# 2. Land the production code fix in the same PR/commit.

# 3. Re-run — must PASS:
cargo test --workspace -- --exact <module::test_name> 2>&1 | grep -q 'ok'
```

The transition is a property of the *fix*, not the *test*. A test that passes both before and after the production change is not a regression test; it is a smoke test, and the fix is not proven. Workerbees that submit a fix without a fail-before / pass-after named test are sent back; the issue does not close.

For walker-activation issues (not `area:bug-fix-from-paideia-os`), the fix-test discipline is recommended but not strictly mandatory. The looser bar: walker-activation issues must add a positive end-to-end test that exercises the activated construct (e.g., a `.pdx` source that uses the construct, compiled to bytes, with a golden-byte fixture or a behavioral assertion).

### 6.7 Workerbee prompt preamble (Phase 6 amendment)

Every Phase-6 workerbee prompt MUST contain, verbatim, near the top:

```
SARIF REGEN MANDATORY if catalog touched.
FIX-TEST MANDATORY if area:bug-fix-from-paideia-os:
  - name the test in the AC (module::test_name)
  - verify FAIL on parent commit
  - verify PASS on this commit
```

The softarch agent inserts this preamble when handing off any Phase-6 issue.

---

## 7. The unblock-criterion focus

This section names the explicit stop point. It is the centerpiece of Phase 6 governance — the autonomous loop runs until this fires.

### 7.1 What "Phase 6 is done" means

**The criterion is paideia-os Phase 2 can start cleanly. Specifically: paideia-os struct-based capability descriptor (`struct Cap { kind: u8, rights: u16, flags: u8, obj_ref: u64 }`) builds end-to-end. Field-access in unsafe blocks emits real bytes. AND paideia-os entry.pdx (`cli; hlt; jmp $-1`) builds without U1606 errors. AND paideia-os long_mode.pdx mov-cr instructions emit real bytes (not placeholder).**

The above paragraph is the load-bearing scope statement. Every Phase 6 issue should be evaluated against the question "does this remove a gate from paideia-os Phase-2 (capability system)?" If the answer is no, the issue belongs in Phase 7+ — not Phase 6.

### 7.2 Closure ritual

When §4.3's eight criteria all evaluate true:

```bash
# In paideia-as:
git tag v0.6.0
git push origin v0.6.0
# (the v0.6.0 tag is the immutable closure event)

# In paideia-os repo (the consumer):
cd ../paideia-os
git submodule update --remote paideia-as          # bumps pin to the v0.6.0 commit
git add paideia-as
git commit -m "Bump paideia-as submodule to v0.6.0 (Phase 6 walker activation & bug fixes)"
git push origin main

# Resume paideia-os autonomous loop:
# - Re-issue the previously-stubbed paideia-os Phase-1 follow-ups (#734/#735/#736 area) to the workerbee
#   to remove the stub workarounds.
# - Then begin paideia-os Phase 2 (capability system) from issue #1.
# - The capability descriptor struct + field-access in unsafe now build end-to-end.
```

**Then: bump paideia-os submodule to post-Phase-6 paideia-as commit. paideia-os Phase 2 work resumes.**

The submodule bump is the *handshake* that proves the cross-repo unblock landed. The paideia-os loop does not resume until the bump commit is on `paideia-os/paideia-os` `main`.

### 7.3 Three smoke-tests as the closure trigger

Phase 6 closes when these three paideia-os-side smoke tests pass:

1. `cd ../paideia-os && tools/build.sh entry.pdx` exits 0; emitted bytes are real (not the placeholder `00 00 00 …` of the pre-Phase-5 era, and no U1606 errors).
2. `cd ../paideia-os && tools/build.sh long_mode.pdx` exits 0; disassembly of the `.elf` shows real `mov %eax, %cr0` family encodings.
3. `cd ../paideia-os && tools/build.sh capability.pdx` (where `capability.pdx` is the canonical capability-descriptor smoke source) exits 0; field-access reads/writes resolve to real loads/stores against the struct layout.

All three must pass against a clean checkout of paideia-os `main` with the paideia-as submodule pinned to the `v0.6.0` tag.

### 7.4 Scope-creep guard

The autonomous loop must not extend Phase 6 beyond §7.1's criterion. If, during the loop, the workerbee or softarch agent proposes work that:

- Targets a paideia-os Phase-3 issue (anything beyond capability system), OR
- Targets a self-hosting milestone (per `self-hosting-phase5-plan.md`, deferred from the original Phase 5 plan), OR
- Activates a Phase-4 surface construct *not* required by paideia-os Phase 2 (e.g., trait objects, higher-kinded generics, advanced pattern guards),

then that work is filed as a `phase:7` issue (label to be created in Phase 7) and the loop continues with the *next* Phase 6 issue. The softarch agent enforces this gate at issue-body authoring time.

---

## 8. Cross-repo escalation continuation (Phase 6 as second worked example)

Phase 5 established the cross-repo escalation protocol. Phase 6 is the second invocation of that protocol — proof that the pattern generalises. This section records Phase 6's escalation trail and continues the protocol forward.

### 8.1 The Phase-6 escalation trail (verbatim record)

| Step | Event | Artifact |
|---|---|---|
| 1 | paideia-os Phase 1 (14/14 issues) closed under `v0.5.0` of paideia-as. Three bugs surfaced during Phase 1 were worked around with stubs: #734, #735, #736. A fourth gap — struct definition + field-access lowering activation for the capability descriptor — was not even stub-able and blocks Phase 2 outright. | paideia-os Phase-1 closure commits; paideia-as issues #734/#735/#736. |
| 2 | paideia-os Phase 2 (capability system) cannot start. The consumer loop halts. | This Phase-6 plan + the parallel osarch plan `.plans/phase-6-plan.md`. |
| 3 | Producer (paideia-as) executes Phase 6 against the unblock criterion (§7). | The Phase 6 milestones + fix-issues + commits. |
| 4 | Producer reaches §4.3 closure; tags `v0.6.0`. | `v0.6.0` tag in paideia-as. |
| 5 | Consumer bumps submodule pin to `v0.6.0`; commits + pushes. | paideia-os commit "Bump paideia-as submodule to v0.6.0 (Phase 6 walker activation & bug fixes)". |
| 6 | Consumer removes the Phase-1 stubs (one issue per stub), then resumes Phase 2. | paideia-os Phase-1-followup commits, then Phase-2 issue #1 onward. |

### 8.2 Per-fix-issue and per-closure cross-repo discipline

Phase 6 makes the cross-repo dependency graph denser than Phase 5 (more fix-issues; more downstream consumers per fix). Per §5 issue-body template:

- **Each Phase 6 fix-issue includes a "Surfaced by paideia-os" body field** linking to the paideia-os symptom (issue number or commit SHA). This is mandatory for `area:bug-fix-from-paideia-os` issues.
- **Each Phase 6 fix-issue closure links forward to the paideia-os Phase-2 issues that the fix unblocks** via the `## Unblocks paideia-os` field. This makes the forward-edge of the cross-repo dependency graph greppable in the same way the backward-edge is.

The combination gives a bidirectional escalation graph:

```
paideia-os symptom commit  →  paideia-as fix-issue (Surfaced by)
paideia-as fix-issue       →  paideia-os Phase-2 unblock (Unblocks)
paideia-as v0.6.0 tag      →  paideia-os submodule bump (commit message)
```

Every edge is grep-able forever.

### 8.3 Protocol invariants (carried from Phase 5, reasserted)

- The producer's closure criterion is *consumer-defined*, not producer-internal. Phase 6 §7.1 names paideia-os Phase-2 as the criterion; Phase 5 §7.1 named paideia-os Phase-1.
- The submodule bump is the cryptographic handshake — grep-able forever in git history.
- The escalation record in the consumer repo (the paideia-os Phase-1 retrospective + the Phase-2 plan) stays as the audit trail.

### 8.4 What two invocations establish

After Phase 6 closes, the cross-repo escalation protocol has been exercised twice (paideia-os Phase-1 → paideia-as Phase 5; paideia-os Phase-2 → paideia-as Phase 6). The pattern is now load-bearing infrastructure for the PaideiaOS / paideia-as cohort. Any future Phase 7+ cross-repo gap should follow the same shape without further reinvention. The third invocation will not need a new softarch plan section — it will reuse this one verbatim, swapping phase numbers.

---

## 9. Documentation discipline within Phase 6

Each milestone closes with a `design/toolchain/<topic>-phase6.md` appendix. The retrospective is written during the closing commit of the milestone (per §4.2 step 2), not deferred.

### 9.1 Mandatory Phase-6 design docs

| Doc | Owner milestone | Purpose |
|---|---|---|
| `design/toolchain/walker-activation-phase6.md` | Master (created in M1, updated through closure) | The master Phase-6 record. Cross-references every milestone retrospective. The single entry point for future-us asking "what did Phase 6 do?". |
| `design/toolchain/operand-parser-fixes.md` | The milestone introducing #734/#735/#736-area fixes | The root-cause analysis + fix design for the operand-parser / encoder-bridge bugs. Includes the canonical bug repro for each, the before/after parse-tree shape, and the fix-test names. |
| `design/toolchain/struct-and-field-access-lowering.md` | The milestone introducing struct + field-access activation | The walker-hookup design for struct definitions + field-access (including the unsafe-block elision of borrow/region checks that the capability descriptor requires). |
| `design/toolchain/phase-transition-6.md` | Closing milestone | **Phase 6 retrospective.** Same shape as `design/toolchain/phase-transition-5.md`: §0 scope, §1 carryover disposition, §2 didn't ship, §3 got right, §4 would change, §5 Phase-6 → Phase-7 carryover, §6 closing note. Substantial milestone appendices linked. |

Per-milestone appendices are written if the milestone delivered substantial work (e.g., a multi-issue M for one of the three bug-fix areas, or the struct-activation milestone). A milestone that lands one or two XS/S fix-issues need only have its narrative folded into the master `walker-activation-phase6.md` doc; a dedicated `<topic>-phase6.md` appendix is optional.

### 9.2 Per-milestone retrospective shape

Every `<topic>-phase6.md` includes:

1. **Status / scope** — 2-3 lines.
2. **What this milestone delivered** — bulleted list of closed issues, one line each.
3. **What this milestone did not deliver** — explicit deferral list, with the deferral target (Phase 7 / external).
4. **Test count delta** — before / after / count of new tests, ratio of test-LOC to source-LOC, count of fix-tests added (§6.6).
5. **Diagnostic codes added** — new codes registered in `catalog.toml` + reserved ranges used.
6. **Cross-repo unblocks** — which paideia-os Phase-2 issues this milestone removed gates from (from per-issue `## Unblocks paideia-os` aggregation).
7. **Surfaced-by-paideia-os linkage** — for each `area:bug-fix-from-paideia-os` issue, the paideia-os symptom commit and the fix-test name.
8. **Open questions** — anything the milestone surfaced that Phase 7+ must address.

### 9.3 Where the docs live

All Phase 6 design docs live under `paideia-as/design/toolchain/` (consistent with the existing Phase 1-5 docs). The doc tree is not relocated. The Phase-1 softarch plan's `design-doc-precedes-code` lint (per `.plans/paideia-as-softarch-plan.md` §5.3) remains suspended for Phase 6 — Phase 6 fix-issues reference paideia-as-internal design docs (operand-parser-fixes, struct-and-field-access-lowering) and paideia-os repo issues; that is sufficient. The lint can be re-enabled in Phase 7 alongside the older "pause after each milestone" rule.

---

## 10. References

- `.plans/phase-6-plan.md` — the WHAT-to-fix companion (osarch). Owns milestones, fix-issues, and per-task decomposition.
- `.plans/phase-5-build-emit-softarch.md` — the analogous Phase 5 softarch doc. Phase 6 mirrors its structure; this doc cites it where discipline is carried forward verbatim.
- `.plans/phase-5-build-emit-plan.md` — the WHAT companion of Phase 5; documents the build-emit gate that Phase 5 closed (and that Phase 6 now extends through the walker chain).
- `design/toolchain/phase-transition-5.md` — Phase 5 retrospective. The baseline against which Phase 6 is measured (2419 tests; v0.5.0 closure; cross-repo escalation #1).
- `paideia-os/design/infrastructure/phase-2-entry.md` — the entry conditions that Phase 6 must satisfy. The consumer-side authority for §7.1's load-bearing scope statement.
- `.plans/paideia-as-softarch-plan.md` — Phase 1 softarch plan; source of the size bands (§2) and label scheme (§3) that Phase 6 inherits.
- `feedback_autonomous_tempo.md` (memory) — the older paideia-as-wide "pause after each milestone" rule that Phase 6 §1 overrides (for the second time; same exception shape as Phase 5).

---

*End of document.*
