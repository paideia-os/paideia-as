---
name: Task
about: A unit of work that fits a single PR (XS / S / M / L per the size bands).
title: '[area] short imperative summary'
labels: ''
assignees: snunezcr
---

## Summary
<one paragraph, ≤ 3 sentences>

## Pillar / decision impact
<which of pillars 1–11 and decisions Q1–Q15 (PaideiaOS) and Q-A1–Q-A10 (custom-assembler.md) this touches; "none" is valid>

## Acceptance criteria
<checklist; each item independently verifiable>
- [ ] …
- [ ] …
- [ ] CI green
- [ ] Linked design doc in PaideiaOS repo reviewed against this change

## Files created / modified
<expected paths in `crates/<crate>/...` and `tests/...`; rough is fine>

## Dependencies
<links to prerequisite or sibling issues; "none" if standalone>

## Estimated size
<XS / S / M / L; if L, justify why not split>

## Test plan
<unit / integration / property / snapshot / corpus; expected new files under `tests/`; whether `insta` snapshots are added>

## Notes
<free-form: edge cases, references, anything reviewers should know>
