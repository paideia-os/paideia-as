# CHANGELOG — FMT-9 (paideia-as#1573)

## Summary

Doc-comment grouping + attachment pass. New `paideia-fmt::doc_comment`
module turns FMT-1b's raw `leading` doc trivia into semantic
`DocBlock`s (groups of consecutive `///` / `/**` / `//!` / `/*!`
lines), re-homes `//!` / `/*!` runs to their enclosing module/
structure, and detaches blank-line-separated outer docs to the
enclosing scope per style spec #1561 §9.2.

## Changes

- `crates/paideia-fmt/src/doc_comment.rs` (new, ~500 LOC inc. tests +
  docs):
  - `pub struct DocBlock { pub lines: Vec<String>, pub is_inner: bool }`
    with `new`, `len`, `is_empty`.
  - `pub fn attach_doc_blocks(&mut TriviaAttachmentMap, &AstArena,
    &SourceText)` — walks each attachment's `leading`, groups doc
    trivia, re-homes `//!` / `/*!` and blank-line-detached runs to
    enclosing nodes.
  - Helpers: `group_doc_runs`, `classify_doc` (recognises `//!` /
    `/*!` by scanning raw bytes — the lexer stamps them as plain
    `LineComment` / `BlockComment` because their 3rd byte is `!`),
    `count_newlines`, `enclosing_node` (smallest strict container).
- `crates/paideia-fmt/src/attach.rs`:
  - `TriviaAttachment` gains `pub doc_blocks: Vec<DocBlock>` field;
    `is_empty` extended to check it.
  - `TriviaAttachmentMap` gains `pub fn push_doc_block(id, block)`
    (creates entry on demand) and `pub fn node_ids() -> Vec<NodeId>`
    (snapshot for mutation-while-iteration).
- `crates/paideia-fmt/src/walker.rs`:
  - `pub fn emit_outer_doc_blocks(ctx, nid) -> Doc` and
    `pub fn emit_inner_doc_blocks(ctx, nid) -> Doc` — dormant
    emission helpers for FMT-4..8 to adopt. Follow the FMT-3
    `emit_leading_trivia` dormancy pattern: present, documented, not
    yet called from any per-kind formatter (which still fall back to
    whole-span slices).
- `crates/paideia-fmt/src/lib.rs`:
  - `pub mod doc_comment;` added.
  - `format()` pipeline threads `trivia_map` as `mut` and calls
    `doc_comment::attach_doc_blocks` between FMT-1b and the walker.
    Additive — raw `leading` buckets stay intact, so byte-identity
    holds for inputs without doc comments.

## Storage decision — doc_blocks on TriviaAttachment

Extended `TriviaAttachment` with a `doc_blocks: Vec<DocBlock>` field
(preferred option per issue brief). Rationale: one-stop lookup for
format emission — the per-kind formatter already fetches
`trivia_map.get(nid)` for raw leading / trailing; adding a parallel
`DocBlockMap` would mean a second lookup and two sources of truth to
keep aligned when a `//!` re-homes. The additive discipline (raw
buckets stay intact) means the switch is cheap: FMT-9 is purely a
semantic view on top of FMT-1b.

## Tests (10 under `#[cfg(test)]`)

1. `single_doc_line_attaches_as_outer_block`
2. `multi_line_doc_block_groups_into_one`
3. `module_doc_bang_rehomes_to_enclosing`
4. `doc_block_separated_by_blank_line_detaches_to_enclosing`
5. `two_doc_blocks_split_on_blank_line`
6. `nested_module_bang_attaches_to_nested_module` (smallest-strict
   enclosing is the middle scope, not the file root)
7. `round_trip_preserves_exact_wording` (verbatim raw-slice storage)
8. `block_doc_comment_single_entry` (`/** */` as one entry)
9. `no_doc_comments_leaves_doc_blocks_empty` (byte-identity guard)
10. `mixed_outer_then_inner_splits_on_style` (`///` + `//!` without
    blank-line → two separate blocks)
11. `enclosing_node_prefers_smallest_strict_container` (two candidate
    containers, pick the smaller)

## Ambiguity / open questions

1. **Inner-slot vs outer-slot after detachment.** The style spec says
   a blank-line-separated outer doc "attaches to the preceding scope's
   inner". FMT-9 implements this by moving the block to the enclosing
   node and setting `is_inner = true` on the DocBlock. The `is_inner`
   flag thus overloads two meanings: original comment polarity (`//!`
   vs `///`) AND rhetorical slot (inner vs leading). The emission
   helpers in walker.rs resolve this unambiguously (one filter per
   slot), but a downstream consumer that wants the ORIGINAL syntax
   back would need to re-sniff the source bytes.
2. **`//!` on the root node.** When `//!` sits at the top of the file
   and no enclosing node exists, FMT-9 keeps it on the current node
   with `is_inner = true` rather than inventing a synthetic
   file-level owner. The current `format_module` fallback (byte-
   identity) is unaffected; FMT-4a's structural module walk will
   decide whether this surfaces as the file module's inner.
3. **Doc-block emission between leading and trailing.** The dormant
   helpers do not currently coordinate with
   `emit_leading_trivia` / `emit_trailing_trivia`: a per-kind
   formatter that calls both and the raw helpers would double-emit
   the doc trivia. FMT-4..8 structural emission replaces the raw
   leading walk with the doc-block walk by convention; this is a
   per-kind formatter concern, not a FMT-9 one. Documented in the
   doc_comment module header.

## Byte-identity

Preserved. The raw `leading` / `trailing` / `inner` trivia buckets
from FMT-1b are not mutated — FMT-9 is purely additive. Inputs
without doc comments do not populate `doc_blocks`, and the walker's
whole-source fallback is byte-identical by construction (unchanged
from FMT-3).

## Build

Not run per sub-agent discipline. Main invokes `bash tools/build.sh`
and re-invokes this agent with the error tail if it fails.
