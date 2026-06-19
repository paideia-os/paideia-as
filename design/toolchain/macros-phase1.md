# Macros — phase 1 (pattern matcher) + phase 2 outcome (reflection)

This document covers the phase-1 pattern-based macro system that shipped
with PRs #136-#139 + the **phase-2 reflection track** that supersedes
it. Phase-1 macros continue to work as a degenerate case; reflection is
the canonical Phase 2+ form.

For the canonical syntactic specification, see `design/toolchain/custom-assembler.md` §5.

## Phase 1: pattern macros

Phase-1 macros use a Rust-`macro_rules!`-style pattern matcher. A
declaration:

```paideia-as
macro twice {
  ($x:expr) => { x + x }
  ($x:expr, $y:expr) => { x + y }
}
```

introduces zero or more rules; the matcher (`paideia-as-elaborator::macro_match`)
tries each rule first-match-wins at the call site. The expansion stage
substitutes the matched fragments into the template (`macro_expand::expand_template`)
and the result is re-parsed by the caller.

Diagnostic codes:

| Code  | Meaning |
|-------|---------|
| M0308 | No matching macro rule at invocation. |
| M0309 | Unbound metavariable in template. |
| M0311 | Macro expansion depth exceeded (default 100). |

Hygiene at phase 1 (PR #139, Ullrich 2020) attaches a fresh `MacroId`
tag to every identifier introduced by a macro template; name resolution
compares the full `HygienicName { spelling, tags }`, so an identifier
introduced by the template doesn't shadow a use-site identifier with
the same spelling.

## Phase 2 outcome: typed-elaborator reflection

Phase-2's `m2-typed-elaborator-reflection` milestone (#199-#210, PRs
#361-#372) replaces the pattern matcher with a **typed-term
reflection** API in the Lean 4 / Ullrich 2020 tradition. After this
milestone closes:

1. **`Term` is first-class** (#199, #200): a typed value with introspectable
   structure. The type system has a `Type::Term` variant; `quote { e }`
   introduces; `~( v )` antiquote splices.

2. **Reflective inspection API** (#202): `kind(t)`, `children(t)`,
   `type_of(t)`, `span(t)`. The `reflect_api` module is the public
   surface every macro body uses.

3. **Term evaluator** (#203): a small-step interpreter for the macro-body
   expression subset. Pure functional plus the inspection builtins;
   integer arithmetic, conditionals, let, head-match.

4. **Splice + elab + hygiene** (#204, #206, #208): the closure of
   `quote → eval → splice` makes the macro body actually do work.
   `elab(t)` re-enters the typer; `splice_with_hygiene` tags newly-
   introduced identifiers so they don't shadow call-site bindings.

5. **MacroEff effect row** (#207): macro bodies run in
   `!{Diag, Elab, FreshName}`. Any other effect → M0312 (the macro
   counterpart of F1106).

6. **Recursion guards** (#209): per-step fuel counter
   (DEFAULT_FUEL = 65536) + stack-depth limit
   (DEFAULT_STACK_DEPTH = 256). M0311 fires with a message distinguishing
   fuel-exhaustion from depth-exceeded.

### Retired: M0307

M0307 was reserved during phase-1 planning as a placeholder for
"macro feature not yet implemented in phase 1." With the m2 reflection
track live, no such gap exists — every feature M0307 was reserved to
flag is now implemented. The code is marked **`deprecated = true`** in
`crates/paideia-as-diagnostics/catalog.toml` and will be deleted in
phase 3.

### Coexistence

Phase-1 pattern macros continue to work after m2 (PR #367, m2-007):
`macro_expand::expand_template` is unchanged and still handles
`(pattern) => template` declarations. `expand_reflective` is a sibling
function for declarations whose body is a typed term. The caller (the
macro driver in the elaborator) picks the right path at expansion time.

### Phase-2-m12 status

End-to-end CLI wiring of the reflective macro pipeline (parser →
elaborator driver → expansion) is **deferred to m3**. Today the m2
infrastructure is exercised through unit tests + a corpus of accept /
reject fixtures under `tests/end-to-end/codes/m2_*.pdx`. The
reject-corpus tests `#[ignore]`-pending the driver wiring so they
activate automatically when m3 lands.

## References

- `design/toolchain/custom-assembler.md` §5 — macro syntax + semantics.
- Ullrich (2020) — "Beyond Notations: Hygienic Macro Expansion for
  Theorem Proving Languages." The base for the hygiene algorithm.
- PRs #361–#371 — the m2 reflection track.
