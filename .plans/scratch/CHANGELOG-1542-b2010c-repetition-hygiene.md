# CHANGELOG scratch — #1542 B2-010c Slice C (repetition + hygiene)

**Issue:** PAS-DEBT-B2-010c, GitHub #1542.
**Version:** workspace 0.36.65 → 0.36.66.
**Wave:** 37 (Slice C caps the macro grammar work started in Slice A / Slice B).

## Summary

Slice C ships the two remaining macro-grammar primitives: `$( ... )*`
/ `$( ... )+` repetition on both pattern and template sides, and a
soft per-invocation hygiene rename over template-literal identifier
tokens. Both `MacroPatternElem` and `MacroTemplateElem` grow
`Repetition` variants; the matcher folds per-iteration captures into
`MatchBinding::reps`; the expander walks the template recursively,
emitting one copy of a group per iteration.

Hygiene: `expand_macro` grows `hygiene_scope: Option<MacroScopeId>`.
When `Some`, template-literal identifiers pick up a `_h<scope_id>`
suffix; fragment substitutions stay verbatim. This is a **soft**
hygiene stopgap — full name-resolver-aware hygiene per Ullrich 2020 §3
(the machinery `expand_reflective_hygienic` already wires for typed
macros via R220.M2, #1416) requires a structured Syntax view over the
string-substitution path, which phase-1 composition does not yet
produce. Slice D (follow-up) delivers that bridge.

## Diagnostics

- `M0310` — repetition count mismatch across two fragments bound by
  the same `$( )*` / `+` group.
- `M0314` — template repetition misuse (rep-bound fragment
  referenced outside a template group, or a template group with no
  rep-bound fragment reference). The task text named M0313 for this;
  M0313 is already allocated to `file_module: no top-level module`,
  so Slice C picks M0314 as the next free slot in the macro range.

## New tests (13 total)

Parser (`parse_macro.rs`):
1. `slice_c_pattern_star_repetition_parses`
2. `slice_c_pattern_plus_repetition_parses`
3. `slice_c_pattern_repetition_without_separator`
4. `slice_c_template_repetition_parses`

Matcher (`macro_match.rs`):
5. `slice_c_structured_star_matches_three_args`
6. `slice_c_structured_star_accepts_zero_matches`
7. `slice_c_structured_plus_rejects_zero_matches`
8. `slice_c_structured_count_mismatch_emits_m0310`

Expander (`macro_expand.rs`):
9. `expand_macro_repetition_star_zero_iterations_emits_empty`
10. `expand_macro_repetition_star_three_iterations_emits_all`
11. `expand_macro_rep_bound_ref_outside_group_emits_m0314`
12. `expand_macro_rep_group_without_rep_fragment_emits_m0314`
13. `expand_macro_hygiene_scope_renames_template_idents`

## New fixtures (3)

- `tests/end-to-end/codes/m2_macro_star_repetition.{pdx,expect}`
- `tests/end-to-end/codes/m2_macro_plus_repetition.{pdx,expect}`
- `tests/end-to-end/codes/m2_macro_repetition_no_sep.{pdx,expect}`

## Files changed

- `crates/paideia-as-ast/src/macros.rs` — `RepMin`; `Repetition`
  variants; `#[non_exhaustive]` on `MacroPatternElem` (already on
  `MacroTemplateElem` since Slice B).
- `crates/paideia-as-ast/src/lib.rs` — re-export `RepMin`.
- `crates/paideia-as-parser/src/parse_macro.rs` — recursive
  byte-cursor scanners; four new tests.
- `crates/paideia-as-parser/tests/macro_fragment_kinds.rs` — added
  `_` wildcard arm on the newly-non-exhaustive `MacroPatternElem`
  match.
- `crates/paideia-as-elaborator/src/macro_match.rs` — `reps` +
  constructors; `#[non_exhaustive]` on `MatchBinding`;
  `match_structured`; `M_REP_COUNT_MISMATCH`; four Slice C tests.
- `crates/paideia-as-elaborator/src/macro_expand.rs` —
  `hygiene_scope` param on `expand_macro`; recursive
  `expand_template_elems` walker;
  `collect_template_fragment_names`; `rewrite_ident_tokens_hygienic`;
  `M_TEMPLATE_REP_MISUSE`; six existing calls updated with `None`;
  five Slice C tests.
- `crates/paideia-as-elaborator/src/lib.rs` — re-exports for the
  new public items.
- `crates/paideia-as-diagnostics/catalog.toml` — M0310 + M0314.
- `STATUS.md` — M-code table rows for M0310 / M0314.

## Constraints honoured

- No encoder changes.
- Slice A / Slice B match sites untouched (all cross-crate matches
  on `MacroPatternElem` / `MacroTemplateElem` already carried `_`
  or `matches!(...)`; the one integration-test exhaustive match on
  `MacroPatternElem` grew a `_` arm as required by
  `#[non_exhaustive]`).
- `MatchBinding` extended additively (`reps: Option<Vec<String>>`);
  no destructive shape change. Struct became `#[non_exhaustive]` so
  future capping is safe.
- Encoder pitfall reminder (`loop`, `if`, `let` reserved labels)
  respected — no new identifiers of those spellings introduced.

## Parser ambiguity noted

The `$` character is now overloaded: `$name:kind` (fragment site),
`$(` (repetition group opener), and a lone `$` (literal). The
scanner disambiguates by lookahead: `$` + `(` opens a group; `$` +
ident-start opens a fragment; anything else stays a literal char.
No ambiguity with the multiplication `*` — `*` outside a
`$( ... )SEP?` position never triggers the group terminator scan.
