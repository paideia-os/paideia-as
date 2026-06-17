## Summary
<1–3 sentences>

## Issue
Closes #<n> (or "tracking only" if exploratory).

## Pillar / decision impact
<pillars 1–11; Q1–Q15; Q-A1–Q-A10; "none" is valid>

## Design-doc reference
<URL into `paideia-os/paideia-os/design/` that this change is consistent with;
 if the design doc itself needs an update, link the open PR in that repo and add the `needs-design` label here>

## CI status
- [ ] `cargo fmt --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo build --workspace --all-targets` clean
- [ ] `cargo test --workspace` clean
- [ ] `cargo deny check` clean (only required when `Cargo.toml` or `Cargo.lock` changed)
- [ ] Linearity-regression corpus passes (when `area:types` / `area:effects` / `area:elaborator` touched)

## Linearity-check impact
<list any `tests/linearity-regression/accept/` or `tests/linearity-regression/reject/` files added or modified; "none" otherwise>

## Risk & rollback
<how this is reverted if it breaks `main`; usually `git revert <sha>` is enough, but state it>

## Self-review checklist
- [ ] I read the diff after pushing (not before)
- [ ] I waited ≥ 30 min between final push and self-merge for non-XS PRs
- [ ] No outstanding comments on the PR (`gh pr view --json reviews`)
- [ ] The merge note format below is included

## Merge note
```
Closes #<issue-n>
Milestone: m<N>-<slug>
Size: <XS|S|M|L>
Design-doc: <URL or "Design-Doc-Waiver: <reason>">
```
