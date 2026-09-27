# PAS-DEBT-B7-001-b — `@macro_expand` compile-time driver

**Issue:** paideia-as#1556
**Version:** v0.36.74
**Landed:** 2026-09-27
**Unblocks:** paideia-as#1529 (B7-001 codes-corpus), paideia-as#1530 (B7-002 reflection-corpus M-code half)

## What landed

Option (b') from the task brief: a top-level compile-time directive
`@macro_expand([<macro>, "<input>"])` that lets cmd_build drive
`expand_macro` + `match_structured` against a declared macro without
needing R221.M4's user-facing `foo!(...)` invocation syntax. On a
successful expansion the pass produces no downstream side-effect; on
any failure, diagnostics
(M0308 / M0309 / M0310 / M0311 / M0314 / M0315) flow through the
normal DiagnosticSink → SARIF route.

## Why option (b') over option (b)

The task brief left the choice open. b' is smaller and lands the
whole pipeline (parser → arena table → elaborator pass → cmd_build
integration → diagnostic emission) without also having to synthesise
a fresh token stream that has to survive downstream lowering and
walker passes. The corpus tests care about the diagnostic behaviour,
not the emitted machine code, so the simpler variant covers the
unblock cleanly.

Full option (b) — piping expanded tokens through re-lex → elab →
downstream compilation — is a larger surface change that would
duplicate a lot of parse_source_file's job at the seam. Deferred to
Slice D (post-R221.M4) when user-facing invocation syntax makes the
extra plumbing pay for itself.

## Files touched

### paideia-as-ast
- `src/macro_expand_directive.rs` — new. `MacroExpandDirective` struct
  and `MacroExpandDirectiveTable` (flat `Vec<MacroExpandDirective>`).
- `src/lib.rs` — declare + re-export.
- `src/arena.rs` — add `macro_expand_directives` field on `AstArena`;
  initialise in `with_capacity`; accessors
  `macro_expand_directives()` / `_mut()`.

### paideia-as-parser
- `src/parse_macro_expand_directive.rs` — new. Adds
  `Parser::parse_top_level_at_directive` that recognises
  `@macro_expand([<Ident>, "<StringLit>"])` (optionally `;`), pushes
  a `MacroExpandDirective` into the arena, and returns a
  `NodeKind::Placeholder` NodeId.
- `src/lib.rs` — declare the new module.
- `src/parse_item.rs` — new arm on `parse_item`'s dispatch for
  `TokenKind::At`; recovery lists in `parse_source_file` include
  `TokenKind::At` as an item start-point.

### paideia-as-elaborator
- `src/macro_expand_pass.rs` — new. `run_macro_expand_directives`
  entry point. Walks the arena's directive table, resolves each
  entry's macro name against MacroDecls reachable from the given
  root (recurses through Module/Structure/Functor bodies), runs
  `match_structured` → `expand_macro`, and forwards diagnostics.
  New constant `M_MACRO_NOT_FOUND: u16 = 315`.
- `src/lib.rs` — declare the new module + re-export.

### paideia-as (cmd_build)
- `src/cmd_build/mod.rs` — one `if let Some(root) = root_id …` block
  right after `validate_file_module_mapping` and before the
  effect-cap coupling check.

### Cargo.toml
- `workspace.version` 0.36.73 → 0.36.74.

### CHANGELOG.md
- New v0.36.74 entry.

## Parser ambiguities discovered

None. The `TokenKind::At` dispatch is unambiguous at item scope —
neither `parse_item` nor `parse_source_file` previously consumed it
at top level. The `@` prefix on functor-level (`@retain` /
`@immediate`) is disambiguated by `parse_functor_with_attrs`'s own
standalone entry point, not `parse_item`.

## Corpus test unblock status

- paideia-as#1529 (B7-001 codes-corpus) — Now unblocked: fixtures
  can drive expand_macro through top-level `@macro_expand(...)`
  directives and assert the emitted M-codes. Existing
  `m2_macro_identity.pdx` / `m2_macro_swap_args.pdx` /
  `m2_macro_multi_fragment.pdx` fixtures under `tests/end-to-end/codes/`
  can be un-ignored once companion `@macro_expand(...)` directives
  are added.
- paideia-as#1530 (B7-002 reflection-corpus M-code half) — Same
  story; the reflection-corpus runner in
  `tests/reflection-corpus/tests/runner.rs` observes M-codes on
  stderr and its `accept`/`reject` fixture pairs can now exercise the
  expander end-to-end.

Both issues can proceed to fixture-authoring in the next slice.

## Diagnostics

- **M0308** (`macro_match::M_NO_MATCH` — re-emitted by
  `macro_expand_pass::M_NO_MATCH`) — no rule in the referenced
  macro's pattern list accepted the supplied input.
- **M0309** (`macro_expand::M_UNBOUND_META`) — template referenced
  an unbound `$name` metavariable.
- **M0310** (`macro_match::M_REP_COUNT_MISMATCH`) —
  repetition-count mismatch inside a matched rule.
- **M0311** (`macro_expand::M_RECURSION_LIMIT`) — expansion depth
  exceeded `MAX_EXPANSION_DEPTH`.
- **M0314** (`macro_expand::M_TEMPLATE_REP_MISUSE`) — template
  `$( )*` misuse.
- **M0315** (new — `macro_expand_pass::M_MACRO_NOT_FOUND`) — the
  referenced macro name is not declared anywhere reachable from the
  compilation root.

## Tests

- Elaborator: `macro_expand_pass::tests` covers identity happy path,
  M0315 on missing name, M0308 on unmatched arity, empty-table
  no-op, and the Module → Structure recursive resolver path.
- Parser: `parse_macro_expand_directive::tests` covers happy-path
  recording, multi-directive ordering, and P0100 on both unknown
  attribute name and missing bracket.
- AST: `macro_expand_directive::tests` covers empty/non-empty table
  invariants and insertion-order preservation.

## Non-goals for this wave

- No parser support for the R221.M4 macro invocation form
  `foo!(...)` — that stays deferred.
- No modification to `expand_macro`, `match_structured`,
  `macro_expand`, or Slices A/B/C parser code (per the task's
  "Do NOT modify" constraints).
- No new corpus fixtures under `tests/end-to-end/codes/` or
  `tests/reflection-corpus/corpus/` — the follow-up slice (Wave 39 or
  its successor) adds those and un-ignores existing ones.

## Build discipline

Per `feedback_no_background_builds.md`, this softarch turn does not
run `cargo build` or `cargo test`. Main runs `bash tools/build.sh`
and, on success, `bash tools/run-qemu.sh`. Debugger runs `cargo
check --tests -p paideia-as-ast -p paideia-as-parser -p
paideia-as-elaborator -p paideia-as` next.
