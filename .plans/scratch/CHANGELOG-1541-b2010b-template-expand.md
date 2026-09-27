# PAS-DEBT-B2-010b Slice B — macro template expansion (structured)

**Issue:** paideia-as#1541
**Wave:** 36 (2026-09-27)
**Version bump:** 0.36.64 → 0.36.65
**Predecessor:** Slice A (B2-010, #1503, v0.36.52) shipped
`MacroPatternElem` + `NodeKind::MacroPattern` + `MatchBinding`
matcher.
**Successor:** Slice C (B2-010c, #1542) will layer repetition
(`$( ... )*`) and hygiene onto the structured shape landed here.

## What Slice B lifts to structured form

Templates: previously a span-only `NodeKind::Placeholder`. Now a real
`NodeKind::MacroTemplate` arena node carrying a
`Vec<MacroTemplateElem>` populated by `parse_macro.rs`, and consumed
by a new `expand_macro` function in the elaborator.

## Files changed

### AST (`crates/paideia-as-ast/`)

- `src/macros.rs` — added `MacroTemplateElem::{Fragment, Literal}`
  enum; extended `MacroRule` with `template_elems`; updated the
  module docstring to describe Slice B alongside Slice A.
- `src/arena.rs` — added `NodeKind::MacroTemplate` alongside
  `MacroPattern`.
- `src/lib.rs` — re-exported `MacroTemplateElem`.

### Parser (`crates/paideia-as-parser/`)

- `src/parse_macro.rs` — allocate template as `NodeKind::MacroTemplate`
  (was `Placeholder`); added `extract_macro_template` that
  char-scans the template bytes for `$name` reference sites and
  returns the interleaved literal / fragment sequence; wired the new
  `template_elems` field into `MacroRule` construction.
- `tests/macro_fragment_kinds.rs` — 4 new tests covering the
  Slice B template arena node kind + structural shape.

### Elaborator (`crates/paideia-as-elaborator/`)

- `src/macro_expand.rs` — added `FragmentName` type alias,
  `MacroExpansion` return struct, `bindings_by_name` helper, and
  `expand_macro` function that walks `template_elems`, composes the
  substituted source, re-lexes it under a caller-supplied `FileId`,
  and returns `{source, tokens, diagnostics}`. Kept `expand_template`
  (phase-1 string-based path) unchanged as fallback. 7 new tests.
- `src/macro_match.rs` — the test-only `MacroRule` literal now
  includes `template_elems: Vec::new()` to keep the matcher tests
  compiling against the extended struct.

### Fixtures (`tests/end-to-end/codes/`)

- `m2_macro_identity.pdx` + `.expect` — trivial `id($x:expr)` identity.
- `m2_macro_swap_args.pdx` + `.expect` — 2-fragment, each ref twice.
- `m2_macro_multi_fragment.pdx` + `.expect` — 3 fragments of 3 kinds
  (ident + literal + expr).

Corpus runner is still `#[ignore]`-gated per B7-001; the fixtures
ship today so the parser + expand_macro round trip has stable
source, and the runner reactivation is one `#[ignore]` deletion once
the macro driver is wired.

## Test coverage summary

- **Elaborator, macro_expand**: 6 new round-trip tests + 1 new
  bindings-helper test. All exercise `expand_macro` via
  hand-constructed `MacroTemplateElem` sequences (structured
  template_elems built by the parser are validated separately in
  the parser tests below).
- **Parser, macro_fragment_kinds**: 4 new tests validating the arena
  node kind + structural shape of `template_elems` produced by
  `parse_macro.rs`.

Total: 10 new unit tests across two crates. The elaborator's existing
Slice A `expand_template` tests are all preserved; the phase-1 path
remains callable.

## Gaps discovered in `MatchBinding`

None. The Slice A `MatchBinding { name: String, kind:
MacroFragmentKind, captured: String }` shape is a natural fit for
Slice B — `expand_macro` consumes it verbatim via the
`bindings_by_name` helper that builds the map keyed on `.name`. The
task description's `BTreeMap<FragmentName, MatchBinding>` signature
is satisfied by `FragmentName = String` since `MatchBinding.name` is
already a `String`; no new type or shape change on the matcher side
was required.

## Slice C compatibility (hygiene + repetition, #1542)

Slice C can layer onto Slice B's contract without breaking it:

1. **Repetition (`$( ... )*`)**: the `MacroTemplateElem` match in
   `expand_macro` carries a wildcard `_` arm that collapses new
   variants to a silent no-op — so Slice C can add
   `MacroTemplateElem::Repetition { inner, sep }` (or similar) and
   `MacroPatternElem::Repetition` in parallel without breaking
   existing Slice B callers. Slice C's own `expand_macro` update
   then adds the missing arm.

2. **Hygiene**: `expand_macro`'s output is a fresh
   `MacroExpansion { source, tokens, diagnostics }`. Slice C's
   hygiene pass can decorate `tokens` with `hygiene::MacroId` tags
   after `expand_macro` returns, or `expand_macro` can grow an
   optional `hygiene_scope: Option<MacroScopeId>` parameter that
   defaults to the current behaviour when `None`. Either shape is
   additive.

3. **Pattern-side validation**: `_pattern_elems` is already
   plumbed through the signature (reserved parameter, unused
   today). Slice C's consistency check (`$name` template refs must
   have a corresponding pattern fragment) drops in with no signature
   break.

## Constraints honored

- Did NOT touch `MacroPatternElem` shape.
- Did NOT touch `MatchBinding` output shape.
- Did NOT bump beyond 0.36.65.
- Non-exhaustive-match reminder honored: `MacroTemplateElem` matches
  in `expand_macro` carry a wildcard `_` arm even though the enum
  isn't currently `#[non_exhaustive]` — this keeps Slice C's future
  variants from breaking Slice B's call site.
- Encoder pitfalls: N/A for this ticket (no assembler code path
  touched); the reserved-label / instruction-encoding constraints do
  not apply to the AST + parser + elaborator surface changed here.

## Build not run

Per softarch protocol, build not run — main should invoke
`bash tools/build.sh` and re-invoke me with the error tail if it
fails.
