# PAS-DEBT-B7-002 (#1530) — reflection-corpus reject runner retest (Wave 38)

## Outcome

**Not reactivatable.** Reject runner stays `#[ignore]`'d. R220.M2 Slice B
(#1541, v0.36.65) and Slice C (#1542, v0.36.66) landed the library-side
macro expander (`paideia_as_elaborator::expand_macro` emitting M0308,
M0309, M0310, M0311, M0314) — but the `paideia-as build` PIPELINE still
never calls that expander, so no `.pdx` source can trigger any M-code
end-to-end. The comparator (`m_codes_for` → `paideia-as build --emit
placeholder <file>` → scrape stderr for `/M\d{4}/`) sees nothing to
match.

Ignore reason has been sharpened once more to name the precise remaining
gap (R221.M4 invocation lexer + `cmd_build` expander wiring), and each
of the 8 `.expect` sidecars now documents its specific blocker or
relocation instruction.

## What Slices B/C actually delivered vs. what remains

Landed (library level, verified by grep on `crates/paideia-as-elaborator/`):
- `macro_match.rs` — `try_match_rules` emits **M0308** (no matching rule)
  and **M0310** (ragged repetition counts across a group).
- `macro_expand.rs` — `expand_macro` / `expand_reflective_hygienic` emit
  **M0309** (unbound `$name` in template, once per unique name),
  **M0311** (depth-guard exceeded via `check_depth`), and **M0314**
  (template repetition misuse: `Rep`-bound ref outside a template
  `Repetition`, or template `Repetition` with no `Rep`-bound fragment).
- `dsl_parser_registry.rs` — R220.M3 attachment point for
  `@dsl_parser("<name>")` with M0320..M0323 for registration errors
  (out of scope for this reject corpus).
- `term_eval/tests_limits.rs` — M0311 also emitted for evaluator fuel/
  depth exhaustion (separate emission site, same code).

NOT landed (verified by grep on `crates/paideia-as/src/cmd_build/`):
- **Macro invocation-site parsing.** The parser has `macro foo(...)
  => { ... }` DECL support (`parse_macro_decl` in `parse_macro.rs`) but
  no `foo!(...)` call syntax and no DSL context-lexer for
  `dsl_name { <body> }`. The `dsl_parser_registry.rs` module doc
  explicitly says: "The context lexer that recognises `dsl_name
  { <body> }` at the invocation site (and thus decides *when* to call
  `DslParserRegistry::dispatch`) lands in **R221.M4** — see
  `design/terminal/semantic-shell-language-plan.md` §4."
- **`cmd_build` pipeline wiring.** No file under
  `crates/paideia-as/src/cmd_build/` (13 files: `addr_of_pass.rs`,
  `addr_of.rs`, `data_pass.rs`, `diagnostics.rs`, `elf.rs`, `fixup.rs`,
  `identifier.rs`, `layout.rs`, `mod.rs`, `pax.rs`, `pe.rs`,
  `placeholder.rs`, `populate.rs`, `resolve_names.rs`, `root_attrs.rs`,
  `tests.rs`, `validate.rs`, `walker_pipeline.rs`) references
  `expand_macro`, `expand_reflective_hygienic`, `MacroCall`, or any
  invocation-node walker. The expander is entirely un-called by
  `paideia-as build`.

Either gap alone would prevent the reject corpus from firing an M-code
via `paideia-as build --emit placeholder`. Both are open.

## Per-fixture status (8 fixtures)

### M-code targets (4 — cannot be authored yet)

| Fixture | Target | Landed emitter? | Body needed | Status |
|---|---|---|---|---|
| `r_macro_no_matching_rule` | M0308 | yes (macro_match.rs) | macro decl + arity-wrong invocation | placeholder — blocked on R221.M4 + cmd_build |
| `r_pattern_match_failure` | M0308 | yes (same site as above) | macro decl + kind-wrong invocation | placeholder — same blocker; also duplicates M0308 with prior fixture (consolidation question flagged in .expect) |
| `r_recursion_depth` | M0311 | yes (macro_expand.rs `check_depth`) | mutually recursive macro pair + driving invocation | placeholder — same blocker |
| `r_unbound_metavariable` | M0309 | yes (macro_expand.rs) | macro decl with unbound `$y` in template + invocation | placeholder — same blocker; NOTE: parser does not eagerly check template-metavar coverage at decl time, so a decl-only fixture would NOT emit M0309 |

Each `.expect` sidecar now names: the target M-code, the exact emission
site (file:line), which Slice landed it, why the current `.pdx` body
is still placeholder, and what body to write once the blockers land.

### P-code targets (4 — belong in a parser-reject corpus)

| Fixture | Target | Emitter landed? | Body correct today? | Status |
|---|---|---|---|---|
| `r_antiquote_outside_quote` | P0170 | yes (quote.rs:131) | yes (`~(v)`) | misfiled — relocate to parser-reject corpus |
| `r_finally_not_last` | P0162 | yes (parse_handler.rs:167) | NO (body is `~(1)`, triggers P0170 not P0162) | misfiled + body-wrong |
| `r_malformed_quote` | P0171 | yes (quote.rs:90) | yes (unterminated `quote { 1 + 2`) | misfiled — relocate |
| `r_unknown_fragment_kind` | P0110 | yes (parse_macro.rs:392) | NO (body is `let m = 1`, no macro decl at all) | misfiled + body-placeholder |

Each `.expect` sidecar now names: the target P-code, the exact emission
site (file:line), whether the current `.pdx` body would actually trigger
it (two of four would not), and the relocation plan.

## Change made

- `tests/reflection-corpus/tests/runner.rs`:
  - Rewrote the doc comment above `reject_corpus_emits_expected_codes`
    to name the two remaining blockers precisely (R221.M4 invocation
    lexer + `cmd_build` expander wiring), enumerate landed M-code
    emission sites, and prescribe a follow-up disposition for #1530.
  - Rewrote the `#[ignore = "..."]` reason so `cargo test` output names
    those blockers and cites Slices B/C.
- `tests/reflection-corpus/corpus/reject/*.expect` (8 files rewritten):
  - Each names its target diagnostic code, the exact emission site,
    what body it needs (M-code fixtures) or its relocation destination
    (P-code fixtures), and why it cannot be activated in this runner
    today. Bare M-code lines are intentionally NOT present in the
    placeholder sidecars — un-ignoring the runner should produce clear
    "expected {} vs got {}" comparisons only after the corpus is
    genuinely fillable, not spurious green from an empty expected set.

Code untouched: `tests/reflection-corpus/src/lib.rs` (comparator),
elaborator, matcher, parser, cmd_build.

## Files touched

- `tests/reflection-corpus/tests/runner.rs`
- `tests/reflection-corpus/corpus/reject/r_macro_no_matching_rule.expect`
- `tests/reflection-corpus/corpus/reject/r_pattern_match_failure.expect`
- `tests/reflection-corpus/corpus/reject/r_recursion_depth.expect`
- `tests/reflection-corpus/corpus/reject/r_unbound_metavariable.expect`
- `tests/reflection-corpus/corpus/reject/r_antiquote_outside_quote.expect`
- `tests/reflection-corpus/corpus/reject/r_finally_not_last.expect`
- `tests/reflection-corpus/corpus/reject/r_malformed_quote.expect`
- `tests/reflection-corpus/corpus/reject/r_unknown_fragment_kind.expect`
- `.plans/scratch/CHANGELOG-1530-b7002-reflection-corpus-retest.md` (this file)

## Version discipline

No `workspace.version` bump in this wave. If main consolidates a
standalone wave, target is 0.36.66 → 0.36.67 with subject
"docs: sharpen reject-corpus blocker notes (#1530)"; if this rides in
a broader batch, main handles the bump.

## Recommended disposition

Keep #1530 OPEN with a comment along these lines:

> Wave 38 retest after Slices B/C landed (v0.36.66): reject runner still
> not reactivatable. The library-side expander now emits
> M0308/M0309/M0310/M0311/M0314 (verified by grep on
> `crates/paideia-as-elaborator/`), but `paideia-as build`'s pipeline
> does NOT call the expander on user source, and the parser has no
> macro-call syntax yet (per `dsl_parser_registry.rs` module doc,
> invocation-site lexing lands in R221.M4).
>
> Two follow-up issues recommended:
>
> 1. **cmd_build expander wiring** (`crates/paideia-as/src/cmd_build/`
>    calls `expand_macro`/`expand_reflective_hygienic` on macro-invocation
>    nodes; scope also covers deciding one M0308 vs distinct codes for
>    arity- vs kind-mismatch, per `r_pattern_match_failure.expect`'s
>    consolidation question).
> 2. **Parser-reject corpus** (`tests/parser-reject-corpus/` sibling of
>    reflection-corpus; P-code comparator `/P\d{4}/`; relocate the four
>    P-code fixtures; repair `r_finally_not_last.pdx` and
>    `r_unknown_fragment_kind.pdx` bodies during relocation).
>
> Close #1530 only after both land AND the four M-code `.pdx`
> placeholders are rewritten around real invocations. (Or split #1530
> into 1530a/1530b tracking each of the two follow-ups.)

## Build discipline

Build not run — main should invoke `bash tools/build.sh` and re-invoke
me with the error tail if it fails. Change is comment/docstring-only
(no Rust code paths altered); risk is limited to a lint on the longer
`#[ignore = "..."]` attribute string or a `.expect` file the comparator
now parses to a stricter shape (unlikely — `parse_expect_file` already
strips `#` comments and skips blank lines).
