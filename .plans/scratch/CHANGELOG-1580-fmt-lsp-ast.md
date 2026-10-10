# paideia-as#1580 — FMT-16: LSP → AST pipeline + rangeFormatting

**Scope:** switch `textDocument/formatting` from the legacy
`format_whitespace` path to `paideia_fmt::format` (AST pipeline), add a
new `textDocument/rangeFormatting` handler, and advertise
`document_range_formatting_provider` in `InitializeResult`.

## Changed

- `crates/paideia-lsp/src/server.rs`
  - **Handler swap**: `formatting` now drives `paideia_fmt::format` with
    options discovered via `paideia_fmt::config::discover`. The function's
    verbatim-fallback contract (lex/parse failure → `Ok(source)`) folds
    cleanly into the handler's `formatted == doc.text` short-circuit, so
    no secondary error-arm is needed.
  - **New handler**: `range_formatting` implements
    `textDocument/rangeFormatting`. The algorithm is "smallest enclosing
    node + slice back the diff":
    1. Format the whole document via the AST pipeline.
    2. If the output is byte-identical to the source (always true today
       under the FMT-3..15 contract), return empty edits.
    3. Otherwise widen the user-requested byte range to the smallest
       line-aligned enclosing region (`widen_to_full_lines`) — the
       pre-FMT-9/10 approximation of smallest-enclosing-AST-node.
    4. Slice the formatted output at the ORIGINAL byte offsets of the
       user-requested range and return a single `TextEdit` whose
       `range` is the user range (NOT the widened enclosing region) —
       the "slice back the diff" discipline per the FMT-16 spec.
    5. Guard: if offsets slip past `formatted.len()` or the slice equals
       the original bytes, return empty edits.
  - **Shared helpers**: `load_fmt_options(uri)` walks ancestors from the
    document's parent directory for `.pdxfmt` via FMT-11 discovery, with
    a defaults fallback on every URI → path failure. `full_document_range`
    centralises the whole-file LSP range computation.
  - **Capability**: `document_range_formatting_provider:
    Some(OneOf::Left(true))` added to `capabilities()`.
  - **Unit tests**: added `widen_to_full_lines` identity /
    mid-line-start / mid-line-end / out-of-range cases, plus
    `capabilities_advertises_document_range_formatting_provider`.

- `crates/paideia-lsp/tests/range_formatting.rs` (**new**)
  - 6 integration-shape tests pinning the FMT-15 byte-identical contract
    through the LSP shape: formatting round-trip, rangeFormatting on a
    small sub-range, rangeFormatting across two top-level items,
    formatting parse-error fallback, rangeFormatting parse-error
    fallback, rangeFormatting on an unknown URI.

## Deferred

- `FIXME(fmt-16-ast-span)` in `widen_to_full_lines`: once FMT-9/10 emit
  real structural edits, replace the line-aligned approximation with an
  AST walk against the arena produced by `paideia_fmt::format` and pick
  the smallest `NodeData::span` fully containing the user's byte range.
  This lands as a follow-up — the current wiring already slices back
  through the ORIGINAL range, so the handler is forward-compatible with
  that change.

## Not run

Build + smoke not invoked (sub-agent policy — builds are main-only).
Main should run `bash tools/build.sh` and re-invoke on failure with the
error tail.
