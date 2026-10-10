# CHANGELOG — paideia-as#1565 (FMT-3: walker scaffold + parse-fail fallback)

## Scope
Critical-path expander for the Phase-2 formatter pipeline. Lands the AST
walker scaffold `paideia_fmt::walker` and the top-level
`paideia_fmt::format(source, opts) -> Result<String, FormatError>` entry
that threads lex → parse → trivia-attach → walk → Doc layout. Every
per-kind formatter today returns the fallback `Doc::text(source[span])`;
the root entry emits `Doc::text(source.content())` to guarantee
byte-identity until FMT-4..8 land their structural Doc constructors.

## Changes

### New modules
- `crates/paideia-fmt/src/walker.rs` (new, ~520 LOC)
  - `FormatContext<'a>` with `source` / `stream` / `arena` / `trivia_map`
    / `opts` borrows + a `slice(span) -> &str` convenience.
  - `format_module(ctx, root)` — byte-identity entry; emits the whole
    source as one `Doc::text`.
  - `format_node(ctx, nid)` — kind-switched dispatch helper, routes
    every `NodeKind` to its category formatter and falls back on
    `_` so future variants (NodeKind is `#[non_exhaustive]`) land
    safely.
  - Per-category stub formatters `format_item`, `format_expr`,
    `format_type`, `format_pattern`, `format_attribute` — all return
    `fallback_slice(ctx, node.span)` today; FMT-4..8 replace each.
  - Public `fallback_slice(ctx, span) -> Doc` so future per-kind
    formatters can defer on recovered sub-trees.
  - Dormant trivia helpers `emit_leading_trivia`, `emit_trailing_trivia`
    (not called by any stub; adopted by FMT-4..8 as their Doc
    construction stops emitting the whole span as one Text).

### Updated module
- `crates/paideia-fmt/src/lib.rs`
  - Added `pub mod walker;`.
  - Added `pub fn format(source, opts) -> Result<String, FormatError>`
    — top-level pipeline entry (fast-paths empty / invalid-UTF-8 /
    lex-error / parse-error / parse-recovered-with-errors all to
    `Ok(source.to_string())`).
  - Added `#[non_exhaustive] pub enum FormatError { Reserved }` as the
    forward-compat placeholder.
  - Added `FormatOptions.max_width` (default 100) forwarded into
    `layout::FormatOptions`.
  - Renamed the Phase-2-m8-010 whitespace normaliser from
    `fn format` → `pub fn format_whitespace` (same behaviour, same
    signature); legacy tests renamed in step.

### Caller migrations (preserve legacy behaviour)
- `crates/paideia-fmt/src/main.rs` — CLI binary now calls
  `paideia_fmt::format_whitespace`. CLI test unchanged.
- `crates/paideia-as/src/cmd_fmt.rs` — `use paideia_fmt::format_whitespace
  as format;` keeps the file's local shape intact.
- `crates/paideia-lsp/src/server.rs` — LSP format-on-save calls
  `format_whitespace`.

### New tests
- `walker.rs` (6 tests): byte-identity on `pub let`, empty source,
  leading comment preserved, trailing comment preserved, invalid
  `NodeId` → `Nil`, `fallback_slice` emits span text, dormant
  `emit_leading_trivia` returns `Nil` on empty map, idempotence of
  `format_module` output through the pipeline.
- `lib.rs` (10 new tests under `tests` mod): empty source, byte-identity
  on single `pub let`, parse-error fallback, lex-error (lone-CR)
  fallback, leading / trailing line-comment preservation, idempotence
  on simple `let`, idempotence on commented source, idempotence on
  multi-item module, `FormatError::Reserved` compile-guard.

### Cargo
- `crates/paideia-fmt/Cargo.toml` — add `paideia-as-parser` dep (no
  cycle; parser has no fmt dep).

## Build status
Not run — main invokes `bash tools/build.sh` and re-dispatches on
failure.
