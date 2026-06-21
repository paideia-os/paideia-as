# paideia-as Phase 5 — Build-Emit Activation: Process & Governance (softarch)

**Status:** Draft v0.1
**Date:** 2026-06-20
**Owner:** softarch agent (process / governance / cargo-green-gate dimensions)
**Phase scope ceiling:** build-emit activation only — enough machine code from `.pdx` to unblock paideia-os Phase-1 kernel bring-up. Self-hosting (the originally-planned Phase 5 per `design/toolchain/self-hosting-phase5-plan.md`) and full Phase-4-surface walker activation are deferred to Phase 6+.
**Sister deliverable:** `.plans/phase-5-build-emit-plan.md` (osarch) — owns the *what* (milestones, tasks, decomposition).

---

## 0. Scope + non-overlap

This doc and the osarch plan partition Phase 5 governance cleanly.

| Dimension | Owner |
|---|---|
| Milestone list, sequencing, per-task decomposition | osarch (`.plans/phase-5-build-emit-plan.md`) |
| PR sizing discipline (XS/S/M/L bands, split triggers) | softarch (this doc, §2) |
| GitHub label additions for Phase 5 | softarch (this doc, §3) |
| Milestone shape, naming, closure criterion | softarch (this doc, §4) |
| Issue body template (Phase-5 additions) | softarch (this doc, §5) |
| Autonomous-loop tempo (the explicit Phase-5 override) | softarch (this doc, §1) |
| cargo-green gate + SARIF regen + workerbee preamble | softarch (this doc, §6) |
| Unblock criterion + downstream submodule bump ritual | softarch (this doc, §7) |
| Cross-repo escalation protocol | softarch (this doc, §8) |
| Documentation discipline (design/toolchain/*.md per milestone) | softarch (this doc, §9) |

Where the two docs overlap by necessity (e.g., milestone names appear in both), the osarch plan is the source of truth for the *list*; this doc is the source of truth for the *shape* (label set, closure criteria, retrospective discipline).

---

## 1. Tempo (CRITICAL — Phase-5-specific override)

**The user's decision on 2026-06-20: Phase 5 build-emit-activation runs continuously across all milestones. There is no per-milestone pause-for-review. The autonomous loop runs until the paideia-os Phase-1 unblock-criterion (§7) is met.**

This is an explicit override of the older paideia-as-wide rule (per `feedback_autonomous_tempo.md`) that paused after each milestone for review. The rationale: Phase 5 is itself the result of a cross-repo escalation (paideia-os work stalled on a build-emit placeholder); pausing inside Phase 5 leaves paideia-os blocked. The loop runs to the *cross-repo* unblock event, not to a paideia-as-internal review boundary.

### 1.1 Per-issue cadence

```
softarch  → produces the issue body (Phase-5 template, §5)
workerbee → implements + cargo test --workspace
debugger  → triaged if cargo red, OR if a test count drops
if cargo green:
  commit + push to main         (PaideiaOS-mode no-PR workflow, per Phase 4 §3)
  gh issue close <n>
```

### 1.2 Per-milestone cadence

```
on last-issue-in-milestone close:
  write design/toolchain/<topic>-phase5.md retrospective (§9)
  immediately pick up next milestone's first issue
  no pause; no review checkpoint
```

### 1.3 Stop condition

The loop stops *only* when the paideia-os Phase-1 unblock-criterion (§7) is met. That is the bounded scope ceiling the user has set. Closure ritual: tag `v0.5.0`, push, bump paideia-os submodule pin, resume paideia-os autonomous loop. (Phase 5 closure detail in §4.)

### 1.4 Contrast with prior paideia-as rule

| Aspect | Older rule (paideia-as default) | Phase 5 override |
|---|---|---|
| Pause between milestones | Yes, for user review | **No** |
| Pause between issues | No (within a milestone) | No |
| Pause condition | End-of-milestone | **Only** at Phase-5 closure (§7) |
| Rationale | Solo-dev sanity check | Downstream-paideia-os is blocked; pause cost is cross-repo |

The older rule remains in effect for Phase 6+. This is a one-phase exception.

---

## 2. PR sizing discipline (Phase-5 nuances)

Same XS/S/M/L bands as `.plans/paideia-as-softarch-plan.md` §1.1. Restated here for in-doc reference, with Phase-5-specific L-split triggers appended.

### 2.1 Size bands (unchanged)

| Band | Net diff (LOC, ex. generated, ex. corpus, ex. snapshots) | Files touched | Test files added/modified | Review target |
|---|---|---|---|---|
| **XS** | ≤ 50 | ≤ 3 | 0–1 | ≤ 10 min |
| **S** | 51–200 | ≤ 6 | ≥ 1 | ≤ 25 min |
| **M** | 201–500 | ≤ 12 | ≥ 1 | ≤ 45 min |
| **L** | 501–1000 | ≤ 20 | ≥ 2 | ≤ 60 min |
| **XL** | > 1000 | — | — | **forbidden — must be split** |

Generated code, test corpora, and `Cargo.lock` lines are excluded from the LOC count. SARIF snapshot regen output is excluded.

### 2.2 General L-split triggers (inherited from Phase 1 softarch plan)

1. The PR touches more than one crate's *public* API.
2. The PR introduces a new dependency in `[workspace.dependencies]`.
3. The PR adds a new crate to the workspace.
4. The PR is the first implementation of a Q-A* decision item.
5. The PR mixes feature work + refactor + test scaffolding (split into three PRs).
6. CI wall-clock projected to exceed 25 min.

### 2.3 Phase-5-specific L-split triggers (additional)

These are appended because Phase 5 work concentrates risk along the elaborator → encoder → emitter chain. Bundling these multiplies blast radius.

7. **Elaborator chokepoint + encoder.** An L that modifies the elaborator's lowering chokepoint *and* the encoder's instruction-emission path must split. The elaborator change goes first as a refactor / scaffolding PR (no behavioral change to emitted bytes); the encoder change follows as a feature PR.
8. **New x86_64 encoding + elaborator wiring.** An L that adds new x86_64 instruction encodings (e.g., `lgdt`, `lidt`, `wrmsr`, `cpuid` for boot intrinsics) *and* wires them through the elaborator must split. New encoding PR first (with encoder unit tests + golden-byte fixtures), elaborator-wiring PR second (with .pdx round-trip).
9. **New effect kind + user-code lowering.** An L that adds a new effect kind to the prelude *and* lowers user-code that uses it must split. Effect-row declaration PR first (types/effects only, with linearity-corpus entries); user-code-lowering PR second.

The canonical split pattern (per Phase 1 §1.3): (a) refactor / scaffolding PR (semantically no-op); (b) feature PR consuming the scaffolding; (c) test/corpus PR exercising the feature. Phase 5 adds: (d) golden-byte fixture PR when encoding bytes change.

### 2.4 No-PR mode reminder

Per Phase 4 retrospective §3: under PaideiaOS-mode, work lands as direct pushes to `main` after `cargo test --workspace` green. The size bands still apply — the "PR" is the squash-merge commit. A single commit that exceeds 1000 LOC net is forbidden the same as a PR XL.

---

## 3. GitHub label additions for Phase 5

The existing paideia-as label scheme (per `.plans/paideia-as-softarch-plan.md` §2) covers area / type / phase / priority / special. Phase 5 adds **four** labels. Color choices align with the existing palette: `phase:*` is purple; cross-cutting Phase-5 area labels use a distinguishing teal/orange to avoid collision with the existing green `area:*` crate labels.

| Label | Color | Description |
|---|---|---|
| `phase:5` | `#5319E7` (purple) | Phase-5 deliverable per `.plans/phase-5-build-emit-plan.md`. Closes when the unblock-criterion in this doc §7 is met. |
| `gated:downstream-paideia-os` | `#B60205` (red) | Closure of this issue is part of the paideia-os Phase-1 unblock criterion. Used to filter for the cross-repo critical path: `gh issue list --label gated:downstream-paideia-os --state open`. |
| `area:emit-activation` | `#0E8086` (teal) | Cross-cutting work touching the elaborator → encoder → emitter glue. Distinct from per-crate `area:elaborator` / `area:encoder` / `area:emitter-elf` because the work spans the chokepoint. |
| `area:boot-intrinsics` | `#D77B0E` (orange) | x86_64 instructions added specifically to support paideia-os boot code (`lgdt`, `lidt`, `wrmsr`, `rdmsr`, `cpuid`, `cli`, `sti`, `hlt`, `outb`/`inb` family, segment-register loads). Distinct from `area:encoder` because these are user-visible language intrinsics, not just bytes. |

### 3.1 Label combinations

Every Phase 5 issue carries:

- One `area:*` (crate-level, from the existing 18 `area:*` labels) plus `area:emit-activation` and/or `area:boot-intrinsics` if cross-cutting.
- One `type:*`.
- `phase:5`.
- One `priority:*`.
- `gated:downstream-paideia-os` if and only if closing it removes a gate from a paideia-os Phase-1 issue (#1–#14).

### 3.2 Creation

The labels are created via `gh label create` against `paideia-os/paideia-as` as the first step of the M1 issue in the osarch plan. The osarch plan's M1 issue body must explicitly enumerate the four labels above with the colors and descriptions verbatim.

---

## 4. Milestone shape (Phase 5)

### 4.1 Naming

| Convention | Value |
|---|---|
| Milestone slug | `phase-5-<topic>` (e.g., `phase-5-encoder-boot-intrinsics`, `phase-5-elaborator-chokepoint`) |
| GitHub milestone title | `Phase 5 — <topic>` |
| Description | One sentence + link: "See `.plans/phase-5-build-emit-plan.md` §M<N>." |
| Close trigger | All issues in the milestone closed |

The osarch plan owns the milestone *list* and per-milestone *contents*. This doc owns the *shape*: every milestone in Phase 5 follows the naming convention above and the closure rule below.

### 4.2 Per-milestone closure

A milestone closes when:

1. Every issue under the milestone is closed.
2. The corresponding `design/toolchain/<topic>-phase5.md` appendix (§9) is committed.
3. `cargo test --workspace` is green on the closing commit.
4. The STATUS.md narrative section for that milestone is appended.

No human review pause (per §1). The closing commit immediately precedes the first commit of the next milestone.

### 4.3 Phase 5 closure criterion

This is the user's bounded scope ceiling — the explicit stop point for the autonomous loop. All of:

| # | Criterion | How verified |
|---|---|---|
| 1 | A `.pdx` source built via `paideia-as build --emit elf64 src.pdx -o src.o` AND linked via `ld -T link.ld` produces a binary that when invoked as `qemu-system-x86_64 -kernel src.elf -serial stdio -display none` outputs an observable side effect (writes `x` to COM1) without triple-faulting. | Integration test under `tests/qemu-smoke/`. |
| 2 | The Phase-1 paideia-os kernel boot `.pdx` files build cleanly. | `cd ../paideia-os && cargo xtask build-kernel` exits 0. |
| 3 | `paideia-as --version` reports `0.5.0`. | `paideia-as --version | grep -q '^paideia-as 0.5.0$'`. |
| 4 | Tag `v0.5.0` pushed. | `git tag --list v0.5.0 && git ls-remote --tags origin v0.5.0`. |
| 5 | `CHANGELOG.md` `## v0.5.0 — Phase 5 (build-emit activation)` section landed. | `grep -q '^## v0.5.0 — Phase 5' CHANGELOG.md`. |
| 6 | `STATUS.md` updated with Phase-5-close narrative. | Diff against pre-Phase-5 STATUS.md must include a `## Phase 5 closed` section. |
| 7 | `design/toolchain/phase-transition-5.md` retrospective written. | File exists; references each Phase-5 milestone retrospective from §9. |

When all seven are true, the loop stops, the user is notified, and the cross-repo submodule-bump ritual (§7.2) begins.

---

## 5. Issue body template

Phase 5 inherits the Phase 1 template (per `.plans/paideia-as-softarch-plan.md` §4.1) and adds two required fields. The full template, with Phase 5 additions called out:

```markdown
---
name: Phase 5 Task
about: A unit of Phase-5 build-emit-activation work that fits a single PR (XS/S/M/L per §2).
title: '[area] short imperative summary'
labels: 'phase:5'
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

## Files created / modified
<expected paths>

## Dependencies
<links to prerequisite or sibling issues; "none" if standalone>

## Estimated size
<XS / S / M / L; if L, justify why not split AND show which §2.3 trigger does not apply>

## Test plan
<unit / integration / property / snapshot / corpus / golden-byte fixtures>

## Unblocks paideia-os                                    ← NEW (Phase 5)
<list of paideia-os issue numbers this removes a gate on,
 e.g., `paideia-os/paideia-os#1` for the GDT issue if this task enables `lgdt`.
 "none" if this issue is internal-only (e.g., refactor, doc).>

## Cross-repo escalation source                          ← NEW (Phase 5)
<populated only if this issue was filed in response to a paideia-os symptom.
 Format: link to the paideia-os issue + commit SHA that surfaced the gap.
 "none" if this issue was Phase-5-internal (e.g., follow-up to another Phase-5 issue).>

## Notes
<free-form>
```

The two new fields make the cross-repo dependency graph greppable. `gh issue list --label gated:downstream-paideia-os --state open` returns the critical path; per-issue `## Unblocks paideia-os` tells you *which* paideia-os issues unblock when this one closes.

### 5.1 Commit-message template (PaideiaOS-mode direct push)

Per §1.1, Phase 5 work lands as direct pushes. The commit message is the audit trail:

```
[phase-5-<milestone>-<NNN>] <short imperative>

Closes #<n>
Milestone: phase-5-<topic>
Size: <XS|S|M|L>
Test count: <old> -> <new>
SARIF regen: <yes|n/a>
Unblocks paideia-os: <list or none>

<body>

Co-Authored-By: workerbee <noreply@anthropic.com>
```

`Test count` and `SARIF regen` lines are mandatory; their absence is grep-able for monthly self-audit.

---

## 6. cargo-green gate

The cargo-green gate is the only mechanical reviewer in PaideiaOS-mode. Phase 5 tightens it.

### 6.1 Per-issue requirements

| Check | Tool | Failure policy |
|---|---|---|
| Test suite | `cargo test --workspace` | Blocking — no push without green. |
| Test count growth | Workerbee awk pipeline (per §6.4) | Test count *must strictly grow* per issue. A flat count is a P0 smell. |
| Test count regression | Same | A *drop* in test count is a P0 bug. Halt the loop; spawn debugger. |
| SARIF snapshot regen | `cargo insta review` if `crates/paideia-as-diagnostics/catalog.toml` touched | Mandatory. A missed regen surfaces as a CI red on the next PR; it is cheaper to catch in-PR. |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | Blocking. |
| Format | `cargo fmt --all -- --check` | Blocking (pre-push hook enforces). |

### 6.2 Baseline at Phase-4 close

| Metric | Value at Phase-4 close (2026-06-20, commit `e40bbe7`) |
|---|---|
| Workspace test count | **2172** |
| Crates | 22 (per `Cargo.toml` workspace members) |
| Diagnostic codes | 18 added in Phase 4 (P0196..P0202, T0511..T0514, S0906..S0909, L2001, C1401..C1402, E0010..E0011, M0900) |
| paideia-as version | 0.4.0 |

Phase 5 invariant: the test count starts at 2172 and only grows. Any commit that produces a count below the previous commit's count is reverted on sight.

### 6.3 SARIF regen discipline (lesson from Phase 4)

Per Phase 4 retrospective §4: m7-002 and m7-006 each needed fix-up PRs for missed SARIF regen. The recurring fix was: workerbees forget that adding a diagnostic code to `catalog.toml` requires regenerating the SARIF golden snapshots.

**Phase 5 enforcement (in priority order):**

1. **Workerbee prompt preamble** (mandatory): every workerbee prompt that touches `catalog.toml` includes the literal string `SARIF REGEN MANDATORY if catalog touched` in the prompt body. The softarch agent inserts this when emitting the workerbee handoff.
2. **Pre-commit hook** (Phase 5 m1 deliverable per §9): `.githooks/pre-commit` checks for `git diff --cached --name-only | grep -q catalog.toml` and, if so, runs the SARIF regen + verifies no further `git diff` afterward.
3. **Commit-message check** (manual, monthly audit): grep commits for `SARIF regen: n/a` paired with a `catalog.toml` diff is a process violation.

### 6.4 Test-count counting discipline

Per Phase 4 §4 ("Workerbee test-count reports"): workerbees historically misreported counts by running `cargo test --lib` (which excludes integration tests). **Standing rule, restated for Phase 5:** the only valid count comes from the explicit pipeline:

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

---

## 7. The unblock-criterion focus

This section names the explicit stop point. It is the centerpiece of Phase 5 governance — the autonomous loop runs until this fires.

### 7.1 What "Phase 5 is done" means

**The criterion is NOT "all elaborator constructs lower." That is the bigger Phase 5+ vision (and properly belongs to Phase 6+ when self-hosting is in scope). The criterion IS: paideia-os Phase-1 (issue #1 GDT through issue #14 CI smoke) can build cleanly and the kernel boots in QEMU.**

The above paragraph is the load-bearing scope statement. Every Phase 5 issue should be evaluated against the question "does this remove a gate from paideia-os Phase-1?" If the answer is no, the issue belongs in Phase 6+ — not Phase 5.

### 7.2 Closure ritual

When §4.3's seven criteria all evaluate true:

```bash
# In paideia-as:
git tag v0.5.0
git push origin v0.5.0
# (the v0.5.0 tag is the immutable closure event)

# In paideia-os repo (the consumer):
cd ../paideia-os
git submodule update --remote paideia-as          # bumps pin to the v0.5.0 commit
git add paideia-as
git commit -m "Bump paideia-as submodule to v0.5.0 (Phase 5 build-emit closure)"
git push origin main

# Resume paideia-os autonomous loop:
# - Re-issue the paideia-os Phase-1 issue #1 (GDT) to the workerbee
# - The build no longer hits the placeholder; emit produces real bytes
# - Loop proceeds through paideia-os Phase-1 (#1 → #14)
```

The submodule bump is the *handshake* that proves the cross-repo unblock landed. The paideia-os loop does not resume until the bump commit is on `paideia-os/paideia-os` `main`.

### 7.3 Scope-creep guard

The autonomous loop must not extend Phase 5 beyond §7.1's criterion. If, during the loop, the workerbee or softarch agent proposes work that:

- Targets a paideia-os Phase-2 issue (>= #15), OR
- Targets a self-hosting milestone (any Tier 1/2/3 crate per `self-hosting-phase5-plan.md`), OR
- Targets a Phase-6+ deferred carryover (per Phase 4 retrospective §5),

then that work is filed as a `phase:6` issue (label to be created in Phase 6) and the loop continues with the *next* Phase 5 issue. The softarch agent enforces this gate at issue-body authoring time.

---

## 8. Cross-repo escalation protocol (Phase 5 as the worked example)

Phase 5 itself is the result of a cross-repo escalation. The paideia-os work surfaced a gap in paideia-as: the build emit was a placeholder. This section documents the escalation pattern so any future cross-repo gap follows the same trail.

### 8.1 The Phase-5 escalation trail (verbatim record)

| Step | Event | Artifact |
|---|---|---|
| 1 | Consumer (paideia-os) workerbee attempts to build a `.pdx` file as part of Phase-1 kernel bring-up. | paideia-os commit history (2026-06-20). |
| 2 | Build succeeds but the emitted object file is empty / placeholder bytes. Symptom confirmed empirically. | paideia-os issue (TBD, opened by the user 2026-06-20). |
| 3 | Consumer loop halts. User files cross-repo escalation against paideia-as. | This Phase-5 plan + the resulting `.plans/phase-5-build-emit-plan.md` (osarch). |
| 4 | Producer (paideia-as) executes Phase 5 against the unblock criterion (§7). | The Phase 5 milestones + issues + commits. |
| 5 | Producer reaches §4.3 closure; tags `v0.5.0`. | `v0.5.0` tag in paideia-as. |
| 6 | Consumer bumps submodule pin to `v0.5.0`; commits + pushes. | paideia-os commit "Bump paideia-as submodule to v0.5.0". |
| 7 | Consumer resumes autonomous loop from the previously-blocked issue. | paideia-os Phase-1 #1 onward. |

### 8.2 Generalised protocol

For any future cross-repo escalation:

```
1. Consumer halts; opens escalation issue in consumer repo (links symptom + commit).
2. Producer opens phase or sub-phase against the symptom; uses softarch+osarch
   plan pair (this doc is the template for softarch side).
3. Producer's autonomous loop runs to closure criterion = consumer-unblock event
   (NOT to producer-internal review boundary).
4. Producer tags release.
5. Consumer bumps submodule pin; commits with message
   "Bump <producer> submodule to <tag> (<reason>)".
6. Consumer resumes loop.
```

The pattern's invariants:

- The producer's closure criterion is *consumer-defined*, not producer-internal. This is why §7.1's wording matters: it explicitly names paideia-os Phase-1 as the criterion.
- The submodule bump is the cryptographic handshake — it is grep-able forever in git history.
- The escalation issue in the consumer repo stays open until the bump lands. (It is the audit trail of the cross-repo dependency.)

### 8.3 What this enables

Every future cross-repo issue follows the same shape. The Phase-5 escalation is also a process artifact — it tests the protocol. The protocol works iff Phase 5 closes cleanly per §4.3 and the paideia-os loop resumes per §7.2.

---

## 9. Documentation discipline within Phase 5

Each milestone closes with a `design/toolchain/<topic>-phase5.md` appendix. The retrospective is written during the closing commit of the milestone (per §4.2 step 2), not deferred.

### 9.1 Mandatory Phase-5 design docs

| Doc | Owner milestone | Purpose |
|---|---|---|
| `design/toolchain/build-emit-activation.md` | Master (created in M1, updated through closure) | The master Phase-5 record. Cross-references every milestone retrospective. The single entry point for future-us asking "what did Phase 5 do?". |
| `design/toolchain/boot-intrinsics.md` | The milestone introducing `area:boot-intrinsics` work | Catalog of x86_64 instructions added for paideia-os boot code: opcode encoding, operand forms, prelude binding, .pdx surface syntax, golden-byte fixtures. |
| `design/toolchain/unsafe-block-payload.md` | The milestone introducing `unsafe { ... }` lowering | The unsafe-block lowering walker design. Why unsafe is needed (raw MMIO, port I/O, segment-register loads), how it suppresses borrow / region checks, how it interacts with the effect system. |
| `design/toolchain/phase-transition-5.md` | Closing milestone | Phase 5 retrospective. Same shape as `design/toolchain/phase-transition-4.md`: §0 scope, §1 carryover disposition, §2 didn't ship, §3 got right, §4 would change, §5 Phase-5 → Phase-6 carryover, §6 closing note. |

### 9.2 Per-milestone retrospective shape

Every `<topic>-phase5.md` includes:

1. **Status / scope** — 2-3 lines.
2. **What this milestone delivered** — bulleted list of closed issues, one line each.
3. **What this milestone did not deliver** — explicit deferral list, with the deferral target (Phase 6 / Phase 7 / external).
4. **Test count delta** — before / after / count of new tests, ratio of test-LOC to source-LOC.
5. **Diagnostic codes added** — new codes registered in `catalog.toml` + reserved ranges used.
6. **Cross-repo unblocks** — which paideia-os issues this milestone removed gates from (from per-issue `## Unblocks paideia-os` aggregation).
7. **Open questions** — anything the milestone surfaced that Phase 6+ must address.

### 9.3 Where the docs live

All Phase 5 design docs live under `paideia-as/design/toolchain/` (consistent with the existing Phase 1-4 docs). The doc tree is not relocated. The Phase-1 softarch plan's `design-doc-precedes-code` lint (per `.plans/paideia-as-softarch-plan.md` §5.3) is suspended for Phase 5 — Phase 5 PRs reference paideia-as-internal design docs, not paideia-os/design URLs. The lint can be re-enabled in Phase 6.

---

## 10. References

- `.plans/paideia-as-softarch-plan.md` — the analogous Phase 1 doc; this borrows discipline (size bands, label invariants, self-review structure) and adjusts for Phase-5 specifics (tempo override, downstream-gating, no design-doc-precedes-code lint).
- `.plans/phase-5-build-emit-plan.md` — the WHAT companion (osarch). Owns milestones and per-task decomposition.
- `design/toolchain/phase-transition-4.md` — Phase 4 retrospective. §5 documents the substrate the build-emit gap rests on; §3 documents the no-PR direct-push workflow continued into Phase 5; §4 names the SARIF-regen + test-count lessons folded into §6 above.
- `design/toolchain/self-hosting-phase5-plan.md` — the *originally-planned* Phase 5 (self-hosting Tier 1/2/3 inventory). That work shifts to Phase 6+; this doc explicitly takes "Phase 5" for build-emit activation instead.
- `paideia-os/design/infrastructure/build-system.md` — documents the LCD-surface gate that Phase 5 closes from the consumer side. (Path is in the paideia-os repo, not paideia-as.)
- `feedback_autonomous_tempo.md` (memory) — the older paideia-as-wide "pause after each milestone" rule that Phase 5 §1 overrides.

---

*End of document.*
