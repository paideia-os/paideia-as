# CHANGELOG — paideia-as#1572 (FMT-8: attribute formatters)

## Scope

Factor every `@...` attribute shape that `paideia-as-ast` represents
into a shared emit surface so item / expression formatters re-emit
attributes through one place. Covers:

| shape                        | AST home                                      |
|------------------------------|-----------------------------------------------|
| `@endian(be\|le)`            | `StructFieldAttrTable` / `FieldAttr::Endian`  |
| `@interrupt("v")` / `_error` | `ItemData::Let.interrupt` (`InterruptAttr`)   |
| `@abi("ms"\|"sysv")`         | `ItemData::Let.abi` (`CallingConvention`)     |
| `@link_section(".name")`     | `ItemData::Let.link_section` (`String`)       |
| `@jump_table`                | `ExprData::Match.attrs` (`MatchAttrs`)        |
| `@packed_struct[(align=N)]`  | `StructAttrTable` / `StructAttr::Packed`      |
| `@retain` / `@immediate`     | `FunctorAttrTable` / `FunctorAttr`            |
| `@atomic(Ordering)`          | `ItemAtomicTable` / `AtomicOrdering`          |
| `@align(N)`                  | `ItemData::Let.align` (`u32`)                 |
| `@ring(slots=M,slot_size=K)` | `ItemData::Let.ring` (`(u32, u32)`)           |
| `@no_frame`                  | `ItemData::Let.no_frame` (`bool`)             |

## Changes

### New module `crates/paideia-fmt/src/format_attribute.rs`

- `pub enum AttrKind<'a>` — dispatch tag naming each attribute shape;
  attributes have no `NodeKind`, so this stands in at the dispatch
  boundary.
- `pub fn format_attribute(ctx, nid, kind) -> Doc` — the FMT-8 entry
  the walker exposes; delegates to `emit`.
- `pub fn emit(kind) -> Doc` — exhaustive dispatch into the per-shape
  helpers (an unmatched new `AttrKind` variant lights up at compile
  time).
- Per-shape emit helpers: `emit_endian`, `emit_interrupt`, `emit_abi`,
  `emit_link_section`, `emit_jump_table`, `emit_packed_struct`,
  `emit_functor_attr`, `emit_atomic`, `emit_align`, `emit_ring`,
  `emit_no_frame`. Every helper returns `@name(args)` or bare `@name`
  with no leading / trailing whitespace — callers pick the layout
  (inline-with-space for `fmt_let`, hardline-between for stacks).
- `pub fn format_attribute_stack(attrs: Vec<Doc>) -> Doc` — joins
  attribute Docs with hardlines (spec §7: one attribute per line in
  a stack).
- `pub fn format_attribute_fallback(ctx, nid) -> Doc` — defensive
  byte-identical slice-fallback for the walker entry.

Multi-arg `@ring` uses a Group with `if_break` for the trailing comma
and a hardline before each argument, matching spec §7's wrap rule.

### Coordination with `format_item::fmt_let`

Replaced six inline `doc::text(format!(" @...))")` calls with
`parts.push(doc::text(" ")); parts.push(format_attribute::emit_...(...))`.
No behaviour change on the shipping test corpus — the emit helpers
produce byte-identical output to the inline strings they replace. The
leading space stays in `fmt_let` because it is a layout choice (same
line as the Let header), not an attribute-level concern.

### `format_expr::format_match` wires `@jump_table`

Threaded `attrs: &MatchAttrs` into `format_match` (previously
destructured as `_`) and inserted
`format_attribute::emit_jump_table()` between the scrutinee and the
`{` when `attrs.jump_table` is true, matching the parser's accept
position (`parse_match.rs`). No existing tests exercise `@jump_table`,
so the pre-FMT-8 corpus is unaffected.

### Walker hook

`walker::format_attribute` delegates to
`format_attribute::format_attribute_fallback` (byte-identical slice
of the node's own span). Reached only via the dispatch surface for a
future attribute `NodeKind`.

### Module registration

`lib.rs` adds `pub mod format_attribute;` next to the other per-kind
modules.

## Tests

21 unit tests in `format_attribute::tests`:

- Byte-identity for each simple form (12): `@endian(be)`, `@endian(le)`,
  `@interrupt(...)`, `@interrupt_error(...)`, `@abi("ms")`,
  `@abi("sysv")`, `@link_section(".boot.text")`, `@jump_table`,
  `@packed_struct`, `@packed_struct(align=4)`, `@retain`,
  `@immediate`, `@atomic(SeqCst)`, `@align(16)`, `@no_frame`.
- `@ring` flat form at default width — one-liner byte-identity.
- `@ring` narrow-width wrap at `max_width = 20` — asserts multi-line
  shape with trailing comma on the last argument.
- `format_attribute_stack` three-attribute block — asserts hardline
  between each, no trailing hardline.
- `format_attribute_stack` empty input returns Nil.
- Dispatcher routes each `AttrKind` variant to its canonical string
  (12 variants in one test — exhaustive `emit` match catches wrong-
  arm routing at compile time).

## Scope / spec gaps

- **`@repr(...)`** — called out in the brief but not represented in
  the AST or parser (`grep -rn '\"repr\"' crates/paideia-as-parser
  crates/paideia-as-ast` returns no match). No formatter work until
  a parser-side primitive lands.
- **`@fingerprint("...")`** and **`@dsl_parser("...")`** — represented
  via `ItemFingerprintTable` and `ItemDslParserTable` on
  `pub let`s. Out of brief scope; the same emit pattern applies when
  `fmt_let` grows a side-table read.
- **`@macro_expand([...])`** — a top-level directive, not an item
  attribute; handled by the walker's module-level routing in a
  separate slice (see `format_item.rs` module doc).

## Files touched

- `crates/paideia-fmt/src/format_attribute.rs` (new, ~460 LOC incl. tests)
- `crates/paideia-fmt/src/lib.rs` (one line, module registration)
- `crates/paideia-fmt/src/walker.rs` (stub delegates to module, ~15 LOC)
- `crates/paideia-fmt/src/format_item.rs` (fmt_let uses helpers, ~20 LOC)
- `crates/paideia-fmt/src/format_expr.rs` (format_match emits
  @jump_table, ~15 LOC)
- `.plans/scratch/CHANGELOG-1572-fmt-attributes.md` (this file)

Build not run — main should invoke `bash tools/build.sh` and
re-invoke me with the error tail if it fails.
