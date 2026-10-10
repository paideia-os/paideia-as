# CHANGELOG — FMT-1b (paideia-as#1563)

## Summary

Parser-side trivia-to-AST attachment. New `paideia-fmt::attach` module
turns the lossless [`TriviaStream`] from FMT-1a plus an [`AstArena`]
into a `TriviaAttachmentMap` keyed by `NodeId`, partitioning each
node's comments into `leading` / `trailing` / `inner` per the style
spec #1561 §6-§7 rules.

## Changes

- `crates/paideia-fmt/src/attach.rs` (new, 638 LOC inc. tests + docs).
  - `pub struct TriviaAttachment { leading, trailing, inner: Vec<Trivia> }`.
  - `pub struct TriviaAttachmentMap` with `get`, `len`, `is_empty`,
    `iter`.
  - `pub fn attach_trivia(&TriviaStream, &AstArena, root: NodeId) ->
    TriviaAttachmentMap`.
- `crates/paideia-fmt/src/lib.rs`: `pub mod attach;`.
- `crates/paideia-fmt/Cargo.toml`: adds `paideia-as-ast` path dep.
- 6 unit tests under `#[cfg(test)]`:
  1. `leading_on_top_level_let` — single `//` above first token.
  2. `trailing_on_end_of_line_comment` — same-line `//` after last
     token lands in the EOF trailing bucket.
  3. `inner_comment_in_block` — comment inside token range (not
     adjacent to first token) attaches as inner.
  4. `doc_comment_attaches_as_leading_across_blank_line` — `///`
     exempt from blank-line gap rule.
  5. `shadow_inner_absorbs_parent_inner` — inner-child's trailing is
     NOT re-claimed by enclosing node.
  6. `blank_line_gap_detaches_leading_non_doc` — comment with ≥2
     newlines before the token does NOT attach; resurfaces as root
     inner stray.

## Rules — exact (style spec #1561 §6)

For a node with token range `[first_tok, last_tok]`:

- **Leading** — the bucket `leading_trivia[first_tok]` is partitioned
  into comment groups separated by blank-line runs (≥2 Newlines).
  The group immediately above `tokens[first_tok]` (post-run < 2
  newlines) attaches entirely. Doc comments (`///`, `/**`) from
  non-adjacent groups also attach (always-attach exception). All
  other non-doc comments do not attach as leading.
- **Trailing** — the bucket after `last_tok`
  (`leading_trivia[last_tok+1]` or `stream.trailing_trivia` for the
  final token) is walked forward; comments collected until the first
  `Newline`.
- **Inner** — any comment in a bucket `leading_trivia[j]` for
  `first_tok < j <= last_tok` not already claimed by an inner child.

**Shadowing** — nodes are processed in ascending
`(span_len, byte_start, NodeId)` order; each claims its leading /
trailing / inner trivia into a `HashSet<(bucket_idx,
within_bucket_idx)>` before the next node runs. Outer nodes thus
only see unclaimed trivia inside their inner range.

**Strays** — unclaimed comments after the per-node pass are
assigned to the `root` node's inner (file-level owner). This is
how a non-doc comment above a blank-line gap at the top of the
file still survives in the attachment map.

## Ambiguity / open questions

1. **Doc-comment grouping with a non-adjacent run** — the style spec
   says doc comments ride with the next item regardless of gap, but
   is silent on whether a `/// a` followed by a blank line and then
   `/// b` should produce ONE leading group of two doc lines, or
   two separate attachments to the same node. FMT-1b emits both as
   leading of the next node in source order; FMT-9 (doc attachment)
   may need to re-group them.
2. **Root parameter semantics** — issue text says `root: NodeId`
   but is silent on whether strays must land in root inner. We
   chose yes (most-conservative-rewrite — comments survive in the
   map) and documented it on `attach_trivia`.

## Walker-reuse decision

`paideia-as-ast` ships per-category visitors (`ItemVisitor`,
`ExprVisitor`, `StmtVisitor`, …) but no *recursive* arena walker;
each `walk_*` dispatches on the kind of a single node without
descending into its children. For FMT-1b we need every node in the
arena, so building a kind-dispatching recursive walker in
`paideia-fmt` would mean duplicating the structural knowledge
embedded in `ItemData` / `ExprData` / `StmtData` / `TypeData` /
`PatternData` — a FMT-3 concern, not FMT-1b.

Instead, this pass iterates the arena linearly (`NodeId 1..=len`),
reads each node's `Span`, and lets span containment pick first-/
last-token indices. This is correct independent of the arena's
allocation order (recursive-descent parsers tend to allocate the
parent LAST, which would break any NodeId-ordered walker) and does
not require growing `paideia-as-ast`. FMT-3 may ship a proper
recursive walker; attach can migrate to it then without API change
(the signature already takes `&AstArena`).

## Build

Not run per sub-agent discipline. Main invokes `bash tools/build.sh`
and re-invokes this agent with the error tail if it fails.
