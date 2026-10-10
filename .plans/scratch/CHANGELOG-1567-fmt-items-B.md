# paideia-as#1567 (FMT-4b) — Item formatters bucket B

## Summary

Lands FMT-4b bucket B on top of FMT-4a bucket A (paideia-as#1566) and
FMT-6 type formatter: structural Doc reconstruction for the Functor
family of `ItemData` variants, plus coverage tests that defend
byte-identity for the "absent" slice-brief shapes that today live as
side-tables, directive tables, block-scopes, or not at all in the AST.

## Variants implemented structurally

| variant        | strategy                                               |
|----------------|--------------------------------------------------------|
| `Functor`      | `functor (p1)(p2) -> <body>`; multi-line wrap on       |
|                | overflow via `doc::group` + `doc::nest(2)` + softline  |
|                | between params, `doc::line` before `->`                |
| `FunctorParam` | `(<name> : <sig>)` — must be structural because the    |
|                | `FunctorParam` item's own span is just the name token  |

## Variant handled as span-slice (documented, deferred)

| variant       | reason                                                  |
|---------------|---------------------------------------------------------|
| `FunctorDecl` | parser (`toolkit_attrs::parse_functor_with_attrs_into_arena`) |
|               | synthesises identifier `NodeId`s anchored on the whole- |
|               | decl span; `child_slice` would re-emit the full decl    |
|               | per child. Span-slice IS byte-identical for canonical   |
|               | input today.                                            |

## Variants absent from `ItemData` today (documented, follow-up)

The FMT-4b slice brief names eight shapes; only the Functor-family
three above are `ItemData` variants. The rest are:

* **PackedStruct** — `StructAttr::Packed` via `StructAttrTable` on a
  `Struct` item. Byte-identical through `fmt_struct`'s existing
  fallback (prefix lives inside the Struct's span).
* **EndianAttr** — `FieldAttr::Endian` on `StructFieldAttrTable`
  keyed by field-name NodeId. Byte-identical through `fmt_struct`
  (prefix lives inside Struct body).
* **MacroExpandDirective** — flat `Vec<MacroExpandDirective>` on
  `MacroExpandDirectiveTable`; the parser also leaves a
  `NodeKind::Placeholder` entry in the root `Structure`'s `items[]`.
  The dispatcher's `_ => fallback_slice` arm emits it byte-identically.
* **AtomicItem** — `AtomicOrdering` on `ItemAtomicTable` keyed by a
  `Let`'s NodeId. The `@atomic(Ordering)` trails the Let's value
  OUTSIDE the Let span. Follow-up slice extends `fmt_let`.
* **FunctorAttr** — `@retain` / `@immediate` on `FunctorAttrTable`
  keyed by a `FunctorDecl`'s NodeId. Attribute bytes live BEFORE the
  decl's span (parser consumes them before `parse_functor`
  begins). Follow-up slice extends `fmt_functor_decl`.
* **GpuContext** — `BlockScope::GpuContext(GpuContextBlock)` on an
  `ExprBlock`. Not an item; FMT-5b owns it.
* **Timeline** — no parser, no `NodeKind`, no `ItemData` variant, no
  side-table today. Pending a parser-side primitive before any
  formatter work.

## Files touched

* `crates/paideia-fmt/src/format_item.rs`
    * Module header: expanded §Scope to cover FMT-4a + FMT-4b bucket
      maps; added a dedicated §FMT-4b Bucket B — slice-brief vs
      AST-reality mapping section documenting each of the seven
      absent variants and where its canonical formatting lives.
    * `format_item` dispatcher: three new arms for
      `NodeKind::Functor`, `NodeKind::FunctorParam`,
      `NodeKind::FunctorDecl`. Catch-all `_` branch retained for the
      remaining `Effect` / `OpSig` / `Capability` / `UnsafeBlock` /
      `MacroDecl` / `MacroPattern` / `MacroTemplate` kinds.
    * `fmt_functor` — structural: `functor ` + `separate(softline,
      params)` + `line` + `-> ` + recursive `format_item(body)`,
      wrapped in `doc::group(doc::nest(2, …))`. Flat-mode output is
      byte-identical to canonical source; break-mode places each
      param on its own line at the continuation indent.
    * `fmt_functor_param` — structural: `(<name> : <sig>)` using
      `child_slice` on both children (both carry real token spans).
      Canonical colon spacing per style spec §4.3.
    * `fmt_functor_decl` — `doc::group(fallback_slice(data.span))`
      with a module-doc note on why structural is unsafe.

* `crates/paideia-fmt/src/format_item.rs` — tests (12 new):
    * Functor structural shape: single param byte-identity, two
      params, three params, dense-colon normalisation, many-param
      multi-line wrap, idempotent double-format, nested-let body
      recursion.
    * FunctorParam reconstruction: paren restoration (span-only
      would drop them), short-name case.
    * MacroExpandDirective byte-identity through the dispatcher's
      Placeholder fallback.
    * Dispatcher routing probe: verifies `NodeKind::Functor` lands
      on `fmt_functor` (not the `_` fallback) by asserting the
      canonical colon spacing survives.
    * `@endian(be)` struct-field byte-identity (through
      `fmt_struct`'s existing span-slice — regression guard for the
      follow-up slice that decomposes struct bodies).

## LOC

Approximately +260 LOC in `format_item.rs` (3 new `fn fmt_*` bodies +
expanded module header + 12 new `#[test]` functions). No files
outside `crates/paideia-fmt/src/format_item.rs` touched. No style
spec gap discovered.

## Style spec gaps noted

None new. Section §4.3 (parameter-colon spacing: `addr : u64`
canonical) and §5.1/§5.5 (one-line vs multi-line thresholds,
`functor` body = `structure` body rules) cover the structural
functor path.

## Build / verify

Not run from this agent (sub-agent softarch build discipline).
Main should run `bash tools/build.sh` and re-invoke with the error
tail if the module fails to compile or if `cargo test -p
paideia-fmt` surfaces a failure in the new test block.
