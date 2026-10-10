# CHANGELOG — paideia-as#1571 (FMT-7: pattern formatter)

## Scope

Structural Doc rendering for every `PatternData` variant that
`paideia-as-ast` currently carries. Replaces the walker's
`format_pattern` stub with a dispatcher onto a new `format_pattern`
module.

## Changes

### New module `crates/paideia-fmt/src/format_pattern.rs`

- `pub fn format_pattern_data(ctx, nid) -> Doc` — top-level entry.
- One helper per `PatternData` variant:
  - `fmt_wildcard` — bare `_` glyph (§6.1).
  - `fmt_ident` — `name` / `mut name` (prefix from `mutable` flag).
  - `fmt_literal` — source slice of the literal's placeholder node
    (lossless by construction).
  - `fmt_tuple` — standard tuple rules (one-line fit else per-line
    + trailing comma, §5.6).
  - `fmt_struct` — `Path { f, g: p, … }` with field-pun detection via
    span-equality on the parser's shorthand convention; multi-line
    `{ ... }` with trailing comma per §3.1 / §5.1.
  - `fmt_enum_variant` — `Variant(p, …)` or `Variant` for unit form;
    parser-driven arity.
  - `fmt_or` — `A | B | C` flat; multi-line form breaks *before*
    each `|` so the pipe sits at the start of the continuation line
    (§4.6, §6.1).
  - `fmt_binding` — `name @ inner` (space-both around `@`).
  - `fmt_range` — `a..b` / `..b` / `a..` / `..` (no spaces around
    `..`, §6.1).
  - `fmt_reference` — `&p` / `&mut p` (§6.2 spacing).
  - `fmt_slice` — `[a, b, ..rest]` with tuple-rules body.
  - `fmt_rest` — `..` or `..name` (no space before the binder).

Width policy: every multi-element construct wraps in a `Group` so
the layout engine picks flat-fits-else-break per style spec §4.6.

### Walker hook

`walker.rs::format_pattern` now delegates to
`crate::format_pattern::format_pattern_data`. The dispatch surface
in `format_node` is unchanged.

### Module registration

`lib.rs` adds `pub mod format_pattern;` next to `format_expr`,
`format_item`, `format_type`.

## Tests

21 unit tests in `format_pattern::tests` covering:

- **Match-arm parse path** (parser-produced variants): simple ident,
  int + string literals, bare enum variant (unit, single arg, multi
  arg), or-pattern (two-alt flat, three-alt flat, narrow-width wrap
  with `|` at continuation column, nested-under-binding), binding
  (`name @ 1`), struct pattern (shorthand two-field, renamed, narrow
  width wrap).
- **Direct-arena construction** (variants not yet in parser):
  Wildcard `_`, mutable Ident, 3-element Tuple, Range (both bounds /
  open start / open end / fully open), Reference (shared / mut),
  Slice with rest-binding, empty Slice, standalone Rest.
- **Defensive fallback**: pattern-kind node with no payload falls
  back to source-slice.

Narrow-width tests set `max_width ∈ {10, 18}` and assert on
`contains('\n')` + `|`-at-line-start / trailing-comma.

## Deferred variants / spec gaps

- **Inclusive range `..=`**: `PatternData::Range` has no
  `inclusive` flag (phase-1 lexer emits no `DotDotEq`); the
  formatter is shaped to accept a future flag without a signature
  break. Tracked in `patterns.rs` (PAS-DEBT-B2-013).
- **Path-qualified enum variants** (`Option::None`): the parser
  discards the type qualifier at build time and keeps only the
  variant Ident. Round-trip on `Option::None` therefore collapses
  to `None`. Fix is at the parser / AST layer, not the formatter.
- **Struct trailing `..` (field-elide)**: `PatternData::Struct`
  has no rest flag, so a user-written `Point { x, .. }` is not
  representable in the arena today. The sprint brief calls for
  the formatter to emit a trailing `..` when the AST signals a
  rest-elide; the AST-side flag is the gate. Fallback is
  verbatim-slice via `format_pattern_data`'s `None`-data branch
  (not applicable here because the parser builds a complete
  Struct), so the current behaviour drops the elide marker when
  parsing — again a parser/AST concern.
- **`mut` prefix on ident patterns**: the current parser never
  sets `PatternData::Ident.mutable = true`; the formatter emits
  `mut name` only when the AST flag is set (direct-arena-tested).

## Corner cases where the style spec is silent

- **Struct field punning `{ x: x }`**: span-equality (not slice-
  equality) distinguishes parser-generated shorthand from an
  explicit same-name `: binding`. Explicit same-name round-trips
  as `x: x`; the parser's shorthand round-trips as `x`.
- **Expression-shaped Range endpoints**: `PatternData::Range`
  endpoints are documented in the AST as either Expression or
  Pattern. The formatter routes through `walker::format_pattern`,
  which falls back to source-slice for non-pattern endpoints —
  byte-identical by construction.

## Files touched

- `crates/paideia-fmt/src/format_pattern.rs` (new, ~660 LOC incl. tests)
- `crates/paideia-fmt/src/walker.rs` (stub → delegate, ~10 LOC)
- `crates/paideia-fmt/src/lib.rs` (one line, module registration)
- `.plans/scratch/CHANGELOG-1571-fmt-pattern.md` (this file)

Build not run — main should invoke `bash tools/build.sh` and
re-invoke me with the error tail if it fails.
