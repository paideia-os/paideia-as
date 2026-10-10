# CHANGELOG — paideia-as#1570 (FMT-6: type formatter)

## Scope

Structural Doc rendering for every `TypeData` variant that
`paideia-as-ast` currently carries. Replaces the walker's
`format_type` stub with a dispatcher onto a new `format_type` module.

## Changes

### New module `crates/paideia-fmt/src/format_type.rs`

- `pub fn format_type_data(ctx, nid) -> Doc` — top-level entry.
- One helper per `TypeData` variant:
  - `fmt_name` — `Ident` / `Ident(T1, T2)` generic application.
  - `fmt_fnptr` — `(P1, P2) -> R !{E} @{C}` with per-section wrap.
  - `fmt_closure` — `|P1, P2| -> R !{E} @{C}`.
  - `fmt_tuple` — `(T1, T2, …)`.
  - `fmt_linear_class` — keyword + marker forms (`linear`/`↓`/`~`/…).
  - `fmt_ptr` / `fmt_ref` — `*T`, `&T`, `&mut T` (§6.2 spacing).
  - `fmt_record` / `fmt_enum` — §3.1 trailing-comma rules.
  - `fmt_array` — `[T; N]` with length slicing (type-position only).
  - `fmt_forall` — `forall a b. T`.
  - `fmt_effect_row` / `fmt_capability_row` — `!{…}` vs `@{…}`.
  - `fmt_effect_row_body` — comma-separated items + optional `| rest`.

Width policy: every multi-element construct wraps in a `Group` so the
layout engine picks flat-fits-else-break per style spec §5.1.

### Walker hook

`walker.rs::format_type` now delegates to
`crate::format_type::format_type_data`. The dispatch surface in
`format_node` is unchanged.

### Module registration

`lib.rs` adds `pub mod format_type;` next to `attach`, `cst`, etc.

## Tests

16 unit tests in `format_type::tests` covering:

- Scalar byte-identity: `u64`, `bool`, `str`.
- Compound: tuple, array, pointer, shared / mut reference.
- Function pointer: one-line fit, with effects, with effects+caps,
  multi-arg wrap, effect-row wrap.
- Record: one-line fit + narrow-width wrap (trailing comma check).
- Nested: record inside reference.
- Forall: single bound var.

Narrow-width tests set `max_width ∈ {12, 16, 20}` and assert on
`contains('\n')` / trailing-comma presence.

## Deferred variants

- **Scalars as a dedicated set** (`U8..U128`, `I8..I128`, `F32`,
  `F64`, `Char`, `Unit`): the AST encodes all of these as
  `TypeData::Name` with a plain ident, so the Name fast-path handles
  them uniformly. No per-scalar variant is defined.
- **Slice `[T]`**: `TypeData` has no slice variant; the parser treats
  `[T]` as either a named `Slice(T)` application or as an `Array`
  with an elided length. FMT-6 renders both through the existing
  variants.
- **`MutRef`**: `TypeData::Ref { mutable: true }` carries this —
  there is no separate variant.
- **Capability-row structural wrap**: the parser flattens
  `Mmio.read_cap` into `[Mmio, read_cap]` with no dot marker in the
  arena. Reconstruction with commas would mis-reparse, so capability
  rows slice the raw source span (`@{…}`) verbatim. Effect rows are
  rendered structurally because their items are flat idents with no
  dots. The capability-row AST will need a dedicated variant before
  this helper can go structural; see note in `fmt_capability_row`.

## Corner cases where the style spec is silent

- **Forall bound ordering**: §6.2 does not say whether `forall a b. T`
  should be reordered under alphabetisation. FMT-6 preserves source
  order.
- **Forall with kinded bounds** (`forall a: Trait1 + Trait2. T`):
  bounds do not live on the `Forall` variant's `bound` vec — they are
  carried on separate `GenericParam` items. The style for multi-bound
  renderings will fall out of FMT-4's handling of generic parameters.
- **Generic application brackets**: §6.2 writes `Vec<T>` but the
  current parser accepts only `Vec(T)`. We emit the parens form to
  preserve parseability; a parser upgrade to accept angle brackets
  is the gate.
- **1-arg function type punctuation**: §6.2 says always emit `(T)`
  (no trailing comma). The `TypeData::FnPtr` AST agrees — there is
  no tuple-shaped param disambiguator.

## Files touched

- `crates/paideia-fmt/src/format_type.rs` (new, ~560 LOC incl. tests)
- `crates/paideia-fmt/src/walker.rs` (stub → delegate, ~12 LOC)
- `crates/paideia-fmt/src/lib.rs` (one line, module registration)
- `.plans/scratch/CHANGELOG-1570-fmt-type.md` (this file)

Build not run — main should invoke `bash tools/build.sh` and
re-invoke me with the error tail if it fails.
