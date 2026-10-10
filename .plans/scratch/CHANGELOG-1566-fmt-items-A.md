# CHANGELOG — paideia-as#1566 (FMT-4a Bucket A: per-item formatters)

## Scope
Bucket A of the FMT-4a sprint — nine item-kind formatters hooked into
the walker's `format_item` dispatch slot:

| variant     | strategy                                                 |
|-------------|----------------------------------------------------------|
| `Let`       | structural reconstruction from AST fields                |
| `Module`    | structural: `module <name> = <body>` + recursive dispatch|
| `Structure` | byte-identical fallback on whole-item span               |
| `Signature` | byte-identical fallback on whole-item span               |
| `Enum`      | byte-identical fallback on whole-item span               |
| `Struct`    | byte-identical fallback on whole-item span               |
| `Trait`     | byte-identical fallback on whole-item span               |
| `Impl`      | byte-identical fallback on whole-item span               |

The seven span-slice paths all depend on sub-formatters (FMT-5
expressions, FMT-6 types, FMT-4b composite items) that have not shipped;
Bucket A's contract is to put each variant on its own dispatch slot and
prove the plumbing while FMT-4b/5/6 hollow each out in place.

## Changes

### New module
- `crates/paideia-fmt/src/format_item.rs` (new, ~430 LOC)
  - `pub fn format_item(ctx, nid) -> Doc` — dispatcher on
    `NodeKind::{Let, Module, Structure, Signature, Enum, Struct, Trait,
    Impl}`; fall-through to `fallback_slice` for FMT-4b kinds
    (`Functor`, `Effect`, `Capability`, `UnsafeBlock`, `MacroDecl`, …).
  - `fn fmt_let` — structural reconstruction:
    `[pub ]let [mut ]<name>[ : <type>] = <value>[ <symbol-attrs>]`;
    re-emits `@align/@ring/@link_section/@abi/@no_frame/@interrupt` from
    the AST field in canonical order. Generic lets punt to the whole-
    item span until FMT-6 lands (needs type formatter for bound
    rendering).
  - `fn fmt_module` — `module <name> = <body>` with recursive
    `format_item(body)` so a module whose body is a `Structure` goes
    through its own per-kind formatter rather than being slurped as one
    text blob.
  - `fn fmt_structure`, `fn fmt_signature`, `fn fmt_enum`, `fn fmt_struct`,
    `fn fmt_trait`, `fn fmt_impl` — each wraps `fallback_slice` on the
    item span in `doc::group`; byte-identical by construction because
    the parser's item spans cover the full decl from leading keyword to
    closing brace (verified against
    `parse_item/module_decl.rs:131-136`,
    `parse_item/module_decl.rs:229-233`,
    `parse_item/struct_enum.rs:134-137` + `322-325`,
    `parse_item/trait_impl.rs:145-148`).
  - Private `fn child_slice(ctx, id) -> Doc` — helper that re-slices
    a sub-node (identifier, type, expression) by its own span. Returns
    `doc::nil` on invalid id rather than panic.

### Updated modules
- `crates/paideia-fmt/src/lib.rs`
  - Added `pub mod format_item;` to the module list.
- `crates/paideia-fmt/src/walker.rs`
  - `pub fn format_item(ctx, nid) -> Doc` now delegates to
    `crate::format_item::format_item`. The dispatcher's doc-comment is
    updated to describe the Bucket A coverage and the FMT-4b handoff.
    No other walker code moved.

### New tests (`format_item::tests`, 18 tests)
Pipeline: parse → pull first top-level item from the synthetic root
`Structure` → `format_item(id)` → render through FMT-2 layout.

- `Let`: simple pub, no-type, mutable, pub-mut-with-type, structural-
  reconstruction byte-identity, double-format idempotence.
- `Module`: single-item body, multi-item body, nested `pub let` body
  recursion.
- `Structure`: covered via module path.
- `Signature`: `signature Sig = structure { let x = 1 }` byte-identity.
- `Enum`: unit variants, tuple variant, multiline source preservation.
- `Struct`: two fields, multiline source preservation.
- `Trait`: minimal trait with default-body method.
- `Impl`: minimal impl with one `let`-method.
- Dispatcher plumbing: invalid NodeId → empty Doc.

## Visibility note
Only `ItemData::Let` tracks the `pub` prefix in the AST. The parser
(`parse_item.rs:68-88`) rejects `pub` on every other item kind at the
dispatcher level (P0110: "'pub' is only valid before 'let'"), so the
absence of a `pub` prefix on the span-slice paths is correct by
construction — the item spans never had a `pub ` to lose. For `Let`,
the span starts at the `let` keyword (not `pub`), so `fmt_let`
re-emits the `pub ` prefix from the `public` flag.

## Style spec gap
None discovered. §4.1 (let bindings) and §5.3 / §5.4 (struct / enum
containers) cover every shape exercised by the Bucket A tests.

## Build status
Not run — main invokes `bash tools/build.sh` and re-dispatches on
failure.
