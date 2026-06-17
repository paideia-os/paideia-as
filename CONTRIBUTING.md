# Contributing to paideia-as

Thanks for your interest. This is a small, disciplined project. Reading this file before opening a PR will save us both time.

## Repo overview

`paideia-as` is the Rust implementation of the PaideiaOS custom assembler. The design specification lives in the sibling repo [`paideia-os/paideia-os`](https://github.com/paideia-os/paideia-os) under `design/toolchain/`. **Implementation decisions in this repo must be consistent with that design.** If a design needs to change, open a PR there first; this repo can land code that depends on a pending design change as long as the relationship is explicit.

## Quick start

```bash
git clone https://github.com/paideia-os/paideia-as
cd paideia-as
git config core.hooksPath .githooks   # activates pre-push protection
cargo build --workspace
cargo test --workspace
```

## Before opening a PR

1. **Find or open an issue.** Every PR closes exactly one issue. If you're working on something not yet tracked, open an issue first with the `Task` template.
2. **Read the linked design doc.** The doc is the source of truth; the code is the implementation.
3. **Size your PR.** See [PR sizing discipline](#pr-sizing-discipline) below. PRs larger than ~1000 LOC are split before review.
4. **Run local checks.**
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo build --workspace --all-targets
   cargo test --workspace --all-targets
   ```
   The pre-push hook will reject force-pushes to `main` but does not run these — that's on you.
5. **Reference the design doc** in your PR body or a commit message. Use a URL like `https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/<doc>.md`. The `design-doc-lint` CI gate enforces this; the only escape is an explicit `Design-Doc-Waiver: <reason>` line.

## PR sizing discipline

| Band | Net diff (LOC) | Files | Tests | Review target |
|---|---|---|---|---|
| **XS** | ≤ 50 | ≤ 3 | 0–1 | ≤ 10 min |
| **S** | 51–200 | ≤ 6 | ≥ 1 | ≤ 25 min |
| **M** | 201–500 | ≤ 12 | ≥ 1 | ≤ 45 min |
| **L** | 501–1000 | ≤ 20 | ≥ 2 | ≤ 60 min |
| **XL** | > 1000 | — | — | **forbidden — must be split** |

Generated code, test corpora, `insta` snapshots, and `Cargo.lock` are excluded from the LOC count. The canonical split pattern is: (a) refactor/scaffolding PR (semantically no-op); (b) feature PR; (c) test/corpus PR.

## Test-coverage minimum per PR type

| Label | Requirement |
|---|---|
| `type:feature` | At least one unit test per new public function; one integration test per externally observable behavior. |
| `type:bug` | A regression test that fails on `main` and passes on the PR. |
| `type:refactor` | No new tests; existing suite passes with zero changes to test code. |
| `type:perf` | A criterion bench recording before/after with ≥ 5 runs. |
| `type:test` | Test-only; no production code changes. |
| `type:doc` | `cargo doc` builds clean. |
| `type:infra` | CI changes must be exercised in the PR's own CI run. |

## Branch / merge policy

- Trunk-based on `main`. Topic branches `topic/<slug>` are short-lived.
- **Squash-merge only.** Merge commits and rebase-merges are forbidden.
- Linear history is enforced via branch protection.
- Force-push to `main` is impossible (local pre-push hook + future GitHub branch protection when on Pro).
- Tags follow `v<MAJOR>.<MINOR>.<PATCH>`; phase-1 stays in `v0.*.*`.

## Self-review discipline

Solo development means CI is the only mechanical reviewer; structural discipline replaces a second pair of eyes.

- **Push first, then review.** Open the PR, then read your own diff in the GitHub UI as if it came from someone else.
- **Cooling-off period.** Non-XS PRs wait ≥ 30 min between final push and self-merge. M and L PRs wait ≥ 2 h.
- **The "one objection" rule.** While self-reviewing, raise at least one substantive comment on each non-XS PR. If you can't find one, look harder.
- **No same-minute merges.** A PR opened and merged within the same minute is recorded as a process violation.

Before clicking "Squash and merge":

```bash
gh pr view <n> --json reviews,statusCheckRollup,mergeable,mergeStateStatus
```

Verify `mergeable == "MERGEABLE"`, all required checks `SUCCESS`, no outstanding comments.

## Merge note format

The squash commit message must include:

```
Closes #<issue-n>
Milestone: m<N>-<slug>
Size: <XS|S|M|L>
Design-doc: <URL or "Design-Doc-Waiver: <reason>">

<body>

Co-Authored-By: <as appropriate>
```

`Closes #<n>` auto-closes the issue. `Milestone:`, `Size:`, `Design-doc:` are grep-able for monthly self-audit.

## Higher-stakes areas

When a PR touches `area:types`, `area:effects`, `area:elaborator`, `area:emitter-pax`, or `area:pq-sign`:

1. Write a ≤ 200-word design note in the PR description explaining the change *in your own words* (forces re-derivation).
2. Let the PR sit ≥ 24 h before merging.
3. Re-read the design doc cold the next morning before merging.

This is the minimum substitute for an absent second reviewer.

## Label invariants

Every issue (and by inheritance every PR) carries at least:
- One `area:*` label (the affected crate).
- One `type:*` label (feature/bug/test/doc/infra/refactor/perf).
- One `phase:*` label (typically `phase:1` during phase-1 work).
- One `priority:*` label (p0/p1/p2/p3).

## Reporting bugs

File an issue with the `type:bug` label. Include:
- A minimal `.pdx` (or equivalent) input that reproduces.
- Expected vs. actual diagnostic codes (or other output).
- The output of `paideia-as --version`.

## License

By contributing, you agree your contributions are licensed under the project's [LICENSE](LICENSE).
