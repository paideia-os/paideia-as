# CHANGELOG — paideia-as#1561 (FMT-0 — paideia-fmt style spec)

**Wave:** 61 (paideia-fmt completion chain, chain head).
**Scope:** design-only; no code touched. Parallel-safe.

## Added

- `design/toolchain/paideia-fmt-style-spec.md` — v1 style specification
  for `paideia-fmt`. Covers width/indent (100 col, 2-space, tabs
  forbidden), trailing-comma policy (always in multi-line), colon
  spacing (split name-colon vs field-colon), container one-line vs
  multi-line threshold, match-arm alignment, unsafe-block field
  alignment, attribute positioning, glyph policy, comment attachment,
  and CLI defaults. 17 numbered sections, ~550 lines, grandfathering
  table included.
- `design/toolchain/syntax-reference.md` — resurrected top-level stub
  previously referenced by 23 lexer + diagnostic + design-doc sites
  (grep: `grep -rn "syntax-reference" .` in paideia-as). Enumerates
  item-level and expression-level productions with one example and one
  parser citation per shape. ~270 lines. Explicitly non-authoritative;
  parser wins on drift. Lexical sections stubbed in §7/§A for a future
  pass.

## Rationale

Wave 61 opens the 21-issue FMT chain (#1561–#1581) that takes the
185-LOC whitespace normalizer to a round-trip, comment-preserving
`.pdx` formatter. Every subsequent FMT-N issue references style
decisions this spec settles; without it, those issues cannot start.

The syntax-reference stub was named a dependency for FMT-0 by the Wave
61 survey (`.plans/scratch/wave-61-fmt-survey.md` §2.7): twenty-plus
lexer and parser sources cite it in their module docs, but the file was
missing from the mainline tree. Resurrecting it unblocks future
documentation drift fixes and lands a parser-function navigation index.

## Non-changes

- No `.rs`, `.pdx`, or `Cargo.toml` touched.
- No `cargo` or `tools/build.sh` run (design-only).
- No `workspace.version` bump.
- No corpus mutation (grandfathering plan documented in spec §15 for
  the subsequent FMT-13 corpus-cleanup PR to execute).

## Follow-ups

- FMT-1 (#1562) consumes `paideia-fmt-style-spec.md` §9 (comment
  attachment) and §11 (unsafe-block field alignment).
- FMT-2 (#1563) consumes `paideia-fmt-style-spec.md` §2 (width/indent),
  §3 (trailing commas), §5.1 (one-line threshold).
- FMT-13 corpus-cleanup PR, when FMT-13 lands, executes the
  grandfathering changes listed in `paideia-fmt-style-spec.md` §15.
