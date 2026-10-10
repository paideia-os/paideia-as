# FMT-15 — corpus comment-preservation harness (paideia-as#1579)

Opt-in corpus sweep that locks in the style-spec §9.2 wording-preservation
guarantee at scale: every `.pdx` file's comments (line, block, doc-line,
doc-block) must round-trip byte-exact and position-exact through
`paideia_fmt::format`.

## Files added

- `crates/paideia-fmt/src/comment_extract.rs` (~245 LOC):
  - `pub struct CommentToken { kind, byte_offset, text }`.
  - `pub fn extract_comments(source: &str) -> Vec<CommentToken>` — re-uses
    the FMT-1a `cst::tokenize_with_trivia` pass; no parse needed.
  - Slices into `SourceText::content()` so CRLF and BOM-stripped inputs
    round-trip faithfully.
  - 10 unit tests: empty, no-comments, line, trailing, block (preserves
    internal whitespace), nested block (one trivia), doc-line + doc-block,
    document-order mixed, CRLF, lone-CR graceful empty-return,
    monotonic offsets.
- `crates/paideia-fmt/tests/corpus_comment_preservation.rs` (~370 LOC):
  - `#[test] #[ignore] fn fmt_corpus_preserves_comments()` — the sweep.
  - Reuses `paideia_fmt::corpus::walk_pdx` (FMT-13 shared walker).
  - Workspace discovery via `PAIDEIA_FMT_CORPUS` override or climbing
    `CARGO_MANIFEST_DIR` for `Cargo.toml` + `crates/`.
  - `CommentReport::{Preserved, Mismatch, Skipped}` with first-diff
    index + input/output comment at that index.
  - 11 sanity tests that run by default: line / trailing / doc / nested
    block / multi-kind mix preservation, `first_diff` positive/negative
    coverage, corpus-dir resolution, workspace-root heuristic, skip-list
    emptiness guard, per-file checker on preserved and zero-comment
    fixtures.

## Files modified

- `crates/paideia-fmt/src/lib.rs`:
  - Added `pub mod comment_extract;` (alphabetically between `attach`
    and `config`).

## Skip list

**Empty.** Style-spec §9.2 promises every comment survives, so a
corpus fixture that cannot satisfy the invariant is either a documented
spec exception or a formatter bug. The sanity test
`skip_list_is_empty_at_landing` guards drift.

## Corpus corner-cases surveyed

- **Shebang (`#!/usr/bin/env ...`)**: the paideia-as lexer does not
  recognise `#!` as trivia (see
  `crates/paideia-fmt/src/cst.rs::tests::shebang_prefix_is_not_recognized_as_trivia`).
  Any shebang-prefixed file routes through the FMT-3 lex-error fallback;
  `format` returns the source verbatim, so comment preservation holds
  trivially. No corpus file carries a shebang today.
- **Doc-only / comment-only files**: `tokenize_with_trivia` dumps every
  trivia into `trailing_trivia`; `extract_comments` iterates that bucket
  identically to leading runs. The FMT-3 fallback returns the file
  byte-identical, so comment lists match.
- **Nested block comments** (`/* /* */ */`): the lexer flattens them to
  one `BlockComment` trivia (confirmed by
  `scan_comment::tests::nested_block_comment_increments_byte_len_by_full_length`).
  The extractor surfaces one `CommentToken` per nested block, so input
  == output trivially. Unit-tested explicitly in both modules.
- **CRLF line endings**: `SourceText` preserves CRLF (only BOM is
  stripped). Comment byte-slices therefore round-trip CRLF intact.
- **Reject fixtures** (`parse_error_*`, `*_malformed_*`, `tests/*/reject/`):
  no exclusion needed — the FMT-3 fallback returns them verbatim, so the
  invariant is trivially satisfied; they classify as `Preserved`.

## Running

```sh
# Fast sanity tests (default):
cargo test --package paideia-fmt --test corpus_comment_preservation

# Full corpus sweep (opt-in):
cargo test --package paideia-fmt --test corpus_comment_preservation \
    -- --include-ignored --nocapture

# Narrow to one subdir while bisecting:
PAIDEIA_FMT_CORPUS=examples \
    cargo test --package paideia-fmt --test corpus_comment_preservation \
    -- --include-ignored --nocapture
```

## FMT-3 interaction

Every FMT-4..8 walker still returns `Doc::text(source[node.span])` and
`format_module` returns `Doc::text(source.content())`, so the sweep is a
trivial pass today — exactly the point: it's a *gate* that will start
finding regressions as the structural walkers emit non-verbatim output.
Landing it now means the first FMT-9/10 structural change gets guarded
without a parallel harness scramble.
