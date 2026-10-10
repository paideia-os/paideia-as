//! Trivia-to-AST attachment pass (FMT-1b, paideia-as#1563).
//!
//! Consumes the lossless [`TriviaStream`] produced by FMT-1a
//! ([`crate::cst::tokenize_with_trivia`]) and an [`AstArena`] filled by
//! the parser, and emits a [`TriviaAttachmentMap`] that answers "which
//! comments belong to which node, and in what rhetorical position?"
//! for every node the arena holds.
//!
//! # Rules — short form (style spec #1561 §6-§7)
//!
//! For a node with token range `[first_tok, last_tok]` inside the
//! stream:
//!
//! * **Leading** — comments in `leading_trivia[first_tok]` that sit
//!   *immediately above* the node, i.e., the last comment group in
//!   the bucket (the one adjacent to `tokens[first_tok]` with no
//!   blank-line gap between the group and the token). Comment groups
//!   above a blank-line gap belong to whatever is above them, not to
//!   this node. **Doc comments (`///`, `/**`)** are exempt from the
//!   blank-line rule — they always ride with the next item.
//! * **Trailing** — comments at the start of the trivia bucket
//!   following `last_tok` (`leading_trivia[last_tok+1]` or
//!   `stream.trailing_trivia` for the final token) *before the first
//!   Newline* — i.e., on the same source line as `tokens[last_tok]`.
//! * **Inner** — any comment appearing in a trivia bucket inside the
//!   node's token range (`leading_trivia[j]` for
//!   `first_tok < j <= last_tok`) that *no inner child has already
//!   claimed*. Shadowing is resolved by processing nodes in ascending
//!   span-length order: a smaller inner node claims first, so its
//!   leading / trailing comments are removed from the pool before the
//!   enclosing node looks at its inner range.
//!
//! # Scope — what this module does NOT do
//!
//! * It does not emit formatted output. That is FMT-2 and onward.
//! * It does not rewrite trivia; byte spans flow through unchanged
//!   from the lexer. This keeps the pass loss-less: whatever FMT-1a
//!   captured, FMT-1b indexes.
//! * It does not re-attach whitespace or newlines. Only the four
//!   comment [`TriviaKind`] variants land in a
//!   [`TriviaAttachment`]; whitespace and newlines retain their
//!   stream-level position for the later vertical-space pass.
//!
//! # Walker reuse
//!
//! `paideia-as-ast` ships per-category visitors (`ItemVisitor`,
//! `ExprVisitor`, …) but no *recursive* arena walker: the visitors
//! dispatch on a single node's kind and do not descend. For a
//! trivia-attach pass we need every node, including interior
//! statements and expression operands. Rather than build a new
//! recursive walker in `paideia-fmt`, we iterate the arena linearly
//! (`NodeId` 1..=len) and let span containment determine parent /
//! child — spans are precisely what we need to delimit a node's token
//! range anyway, and this approach is immune to the arena's
//! allocation-order shape (parent-last in recursive descent).

use std::collections::{HashMap, HashSet};

use paideia_as_ast::{AstArena, NodeId};
use paideia_as_lexer::{Trivia, TriviaKind};

use crate::cst::TriviaStream;

/// Comments attached to a single AST node, partitioned by rhetorical
/// position relative to the node's token range.
///
/// Every entry is a comment-kind [`Trivia`] copied verbatim from the
/// stream (no re-spanning, no normalization). The vectors are in
/// source order within each bucket.
#[derive(Clone, Debug, Default)]
pub struct TriviaAttachment {
    /// Comments on the lines immediately above the node, adjacent to
    /// its first token with no blank-line gap. Doc comments always
    /// attach here regardless of gap.
    pub leading: Vec<Trivia>,
    /// Comments on the same source line as, and after, the node's
    /// last token.
    pub trailing: Vec<Trivia>,
    /// Comments inside the node's token range that are not claimed by
    /// any inner child.
    pub inner: Vec<Trivia>,
}

impl TriviaAttachment {
    /// `true` iff this node has no attached comments of any kind.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.leading.is_empty() && self.trailing.is_empty() && self.inner.is_empty()
    }
}

/// Map `NodeId → TriviaAttachment`.
///
/// Only nodes that attracted at least one comment carry an entry;
/// empty entries are not materialized so callers can treat
/// [`TriviaAttachmentMap::len`] as "nodes with comments" directly.
#[derive(Clone, Debug, Default)]
pub struct TriviaAttachmentMap {
    inner: HashMap<NodeId, TriviaAttachment>,
}

impl TriviaAttachmentMap {
    /// Construct an empty map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Fetch the attachment for `id`, if any.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&TriviaAttachment> {
        self.inner.get(&id)
    }

    /// Number of nodes carrying attachments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// `true` iff no node carries any attached comment.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate `(NodeId, &TriviaAttachment)` pairs in HashMap order
    /// (unspecified; callers wanting deterministic order should sort
    /// by `NodeId`).
    pub fn iter(&self) -> impl Iterator<Item = (&NodeId, &TriviaAttachment)> {
        self.inner.iter()
    }

    fn insert_if_nonempty(&mut self, id: NodeId, att: TriviaAttachment) {
        if !att.is_empty() {
            self.inner.insert(id, att);
        }
    }
}

/// Attach every comment in `stream` to the arena node whose token
/// range contains it, following the rules in the module header.
///
/// `root` is the arena's logical top-level node. It is accepted for
/// API clarity (callers hand us the file module they just parsed) and
/// is used only as the default owner for stray comments that fall
/// outside every other node's span — those become `root`'s `inner`.
/// If `root` is out of range, strays are silently dropped (there is
/// no sensible owner).
///
/// # Determinism
///
/// Processing visits nodes in ascending `(span_len, byte_start,
/// NodeId)` order so that smaller inner nodes claim their comments
/// before enclosing nodes consider their inner range. The HashMap
/// return itself is unordered; the per-node vectors inside each
/// attachment are in source order.
#[must_use]
pub fn attach_trivia(
    stream: &TriviaStream,
    arena: &AstArena,
    root: NodeId,
) -> TriviaAttachmentMap {
    let mut map = TriviaAttachmentMap::new();

    // Nothing to attach if the stream has no tokens: all trivia (if
    // any) belongs rhetorically to `root` as inner — a trivia-only
    // file has nowhere else to park comments. We still attempt to
    // populate root's inner from `trailing_trivia`.
    if stream.tokens.is_empty() {
        let strays: Vec<Trivia> = stream
            .trailing_trivia
            .iter()
            .filter(|t| is_comment(t.kind))
            .cloned()
            .collect();
        if !strays.is_empty() && arena.get(root).is_some() {
            map.insert_if_nonempty(
                root,
                TriviaAttachment {
                    leading: Vec::new(),
                    trailing: Vec::new(),
                    inner: strays,
                },
            );
        }
        return map;
    }

    // Collect every node in the arena with a usable span. Nodes with
    // zero-length spans (span_len == 0) are skipped — there is no
    // token range to attach to.
    let mut nodes: Vec<(NodeId, u32, u32)> = Vec::with_capacity(arena.len());
    for raw in 1..=u32::try_from(arena.len()).unwrap_or(u32::MAX) {
        if let Some(id) = NodeId::new(raw)
            && let Some(nd) = arena.get(id)
        {
            let s = nd.span;
            if s.byte_len() == 0 {
                continue;
            }
            nodes.push((id, s.byte_start(), s.byte_end()));
        }
    }

    // Smallest first: a child's span is strictly smaller than its
    // parent's, so processing by ascending length claims innermost
    // first and leaves the outer `inner` scan with only unclaimed
    // trivia (shadow discipline).
    nodes.sort_by_key(|(id, start, end)| (end - start, *start, id.get()));

    // Trivia identifier: (bucket_idx, idx_within_bucket). Bucket
    // index `tokens.len()` means `stream.trailing_trivia`.
    let trailing_bucket: usize = stream.tokens.len();
    let mut claimed: HashSet<(usize, usize)> = HashSet::new();

    for (id, start, end) in &nodes {
        let first_tok = match first_token_in(stream, *start, *end) {
            Some(i) => i,
            None => continue, // No token lies inside this node's span.
        };
        let last_tok = match last_token_in(stream, *start, *end) {
            Some(i) => i,
            None => continue,
        };

        // Leading: look at leading_trivia[first_tok]; keep the last
        // comment group (adjacent to first_tok) + any doc comments
        // from earlier groups.
        let leading_bucket = &stream.leading_trivia[first_tok];
        let (leading, leading_ids) =
            collect_leading(leading_bucket, first_tok, &claimed);

        // Trailing: look at the bucket AFTER last_tok.
        let (trailing_src_bucket_idx, trailing_bucket_ref) = if last_tok + 1 < stream.tokens.len() {
            (last_tok + 1, &stream.leading_trivia[last_tok + 1])
        } else {
            (trailing_bucket, &stream.trailing_trivia)
        };
        let (trailing, trailing_ids) =
            collect_trailing(trailing_bucket_ref, trailing_src_bucket_idx, &claimed);

        // Inner: all comment trivia in buckets strictly between
        // first_tok and last_tok (i.e., leading_trivia[j] for
        // first_tok < j <= last_tok) that is not already claimed.
        let (inner, inner_ids) = collect_inner(stream, first_tok, last_tok, &claimed);

        for cid in leading_ids
            .into_iter()
            .chain(trailing_ids.into_iter())
            .chain(inner_ids.into_iter())
        {
            claimed.insert(cid);
        }

        map.insert_if_nonempty(
            *id,
            TriviaAttachment {
                leading,
                trailing,
                inner,
            },
        );
    }

    // Strays: any comment not claimed by any node ends up as inner
    // of the root (the file-level owner). This includes top-of-file
    // comments that precede the first token when no node covers it
    // (unlikely in practice: root's span usually spans the whole
    // file), and the trailing EOF bucket when the root didn't claim
    // it.
    if let Some(_root_data) = arena.get(root) {
        let mut strays: Vec<Trivia> = Vec::new();
        for (b_idx, bucket) in stream.leading_trivia.iter().enumerate() {
            for (i, t) in bucket.iter().enumerate() {
                if is_comment(t.kind) && !claimed.contains(&(b_idx, i)) {
                    strays.push(t.clone());
                }
            }
        }
        for (i, t) in stream.trailing_trivia.iter().enumerate() {
            if is_comment(t.kind) && !claimed.contains(&(trailing_bucket, i)) {
                strays.push(t.clone());
            }
        }
        if !strays.is_empty() {
            let existing = map
                .inner
                .remove(&root)
                .unwrap_or_default();
            let TriviaAttachment {
                leading,
                trailing,
                mut inner,
            } = existing;
            inner.extend(strays);
            map.insert_if_nonempty(
                root,
                TriviaAttachment {
                    leading,
                    trailing,
                    inner,
                },
            );
        }
    }

    map
}

/// Smallest `i` such that `tokens[i]` lies entirely within `[start,
/// end)`.
fn first_token_in(stream: &TriviaStream, start: u32, end: u32) -> Option<usize> {
    stream
        .tokens
        .iter()
        .position(|t| t.span.byte_start() >= start && t.span.byte_end() <= end)
}

/// Largest `i` such that `tokens[i]` lies entirely within `[start,
/// end)`.
fn last_token_in(stream: &TriviaStream, start: u32, end: u32) -> Option<usize> {
    stream
        .tokens
        .iter()
        .rposition(|t| t.span.byte_start() >= start && t.span.byte_end() <= end)
}

fn is_comment(k: TriviaKind) -> bool {
    matches!(
        k,
        TriviaKind::LineComment
            | TriviaKind::BlockComment
            | TriviaKind::DocLineComment
            | TriviaKind::DocBlockComment
    )
}

fn is_doc(k: TriviaKind) -> bool {
    matches!(k, TriviaKind::DocLineComment | TriviaKind::DocBlockComment)
}

/// Partition `bucket` into comment groups separated by blank-line
/// runs (≥2 consecutive `Newline` trivia) and determine which group
/// (if any) is "adjacent" to the token — i.e., sits immediately above
/// `tokens[first_tok]` with at most one newline between the group's
/// last comment and the token.
///
/// Attach rules (style-spec #1561 §6):
/// * The **adjacent** group — if any — attaches entirely as leading.
///   A group is adjacent iff the newline run between it and the token
///   is < 2 (0 means same-line, 1 means directly-above; ≥2 is a
///   blank-line gap that detaches).
/// * **Doc comments** (`///`, `/**`) in NON-adjacent groups still
///   attach as leading (always-attach rule — the gap doesn't sever
///   them from the item that follows).
/// * Non-doc comments in non-adjacent groups do NOT attach; they
///   belong to whatever was above them (or become strays).
///
/// Trivia positions already in `claimed` are skipped (defensive — in
/// practice the first_tok bucket of an outer node is never a child's
/// bucket, so this is a no-op guard).
fn collect_leading(
    bucket: &[Trivia],
    bucket_idx: usize,
    claimed: &HashSet<(usize, usize)>,
) -> (Vec<Trivia>, Vec<(usize, usize)>) {
    // Walk once, partitioning into groups and recording the newline
    // run that follows each group.
    //
    // Invariants:
    // * `groups[g]` holds the bucket-indices of the comments in group
    //   `g`, in source order.
    // * `nl_after[g]` is the number of Newline trivia between the
    //   last comment of group `g` and the first comment of group
    //   `g+1`, OR between the last comment and the end of the bucket
    //   (which is adjacent to `tokens[first_tok]`) for the final
    //   group.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut nl_after: Vec<usize> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut nl_count: usize = 0;

    for (i, t) in bucket.iter().enumerate() {
        match t.kind {
            TriviaKind::Newline => {
                nl_count += 1;
            }
            TriviaKind::Whitespace => {
                // Whitespace does not count as a line break.
            }
            k if is_comment(k) => {
                if nl_count >= 2 && !current.is_empty() {
                    groups.push(std::mem::take(&mut current));
                    nl_after.push(nl_count);
                }
                current.push(i);
                nl_count = 0;
            }
            _ => {
                nl_count = 0;
            }
        }
    }
    if !current.is_empty() {
        groups.push(current);
        nl_after.push(nl_count);
    }

    // The adjacent group is the FINAL group iff its post-run has < 2
    // newlines. (If > 1 group and the final group is above a
    // blank-line gap, no group is adjacent.)
    let adjacent_idx: Option<usize> = groups
        .last()
        .zip(nl_after.last())
        .and_then(|(_, &gap)| if gap < 2 { Some(groups.len() - 1) } else { None });

    let mut out_ids: Vec<(usize, usize)> = Vec::new();
    for (gi, group) in groups.iter().enumerate() {
        let is_adjacent = Some(gi) == adjacent_idx;
        for &i in group {
            let include = if is_adjacent {
                true
            } else {
                is_doc(bucket[i].kind)
            };
            if include && !claimed.contains(&(bucket_idx, i)) {
                out_ids.push((bucket_idx, i));
            }
        }
    }

    out_ids.sort_by_key(|(_, i)| *i);
    let out: Vec<Trivia> = out_ids
        .iter()
        .map(|&(_, i)| bucket[i].clone())
        .collect();
    (out, out_ids)
}

/// Collect comments at the start of `bucket` that precede the first
/// Newline (same-line trailing comments).
fn collect_trailing(
    bucket: &[Trivia],
    bucket_idx: usize,
    claimed: &HashSet<(usize, usize)>,
) -> (Vec<Trivia>, Vec<(usize, usize)>) {
    let mut out: Vec<Trivia> = Vec::new();
    let mut out_ids: Vec<(usize, usize)> = Vec::new();
    for (i, t) in bucket.iter().enumerate() {
        match t.kind {
            TriviaKind::Newline => break,
            TriviaKind::Whitespace => continue,
            k if is_comment(k) => {
                if !claimed.contains(&(bucket_idx, i)) {
                    out.push(t.clone());
                    out_ids.push((bucket_idx, i));
                }
            }
            _ => continue,
        }
    }
    (out, out_ids)
}

/// Collect any comment trivia inside `(first_tok, last_tok]` not yet
/// claimed.
fn collect_inner(
    stream: &TriviaStream,
    first_tok: usize,
    last_tok: usize,
    claimed: &HashSet<(usize, usize)>,
) -> (Vec<Trivia>, Vec<(usize, usize)>) {
    let mut out: Vec<Trivia> = Vec::new();
    let mut ids: Vec<(usize, usize)> = Vec::new();
    for j in (first_tok + 1)..=last_tok {
        for (i, t) in stream.leading_trivia[j].iter().enumerate() {
            if is_comment(t.kind) && !claimed.contains(&(j, i)) {
                out.push(t.clone());
                ids.push((j, i));
            }
        }
    }
    (out, ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cst::tokenize_with_trivia;
    use paideia_as_ast::{AstArena, NodeKind};
    use paideia_as_diagnostics::{FileId, Span, VecSink};
    use paideia_as_lexer::SourceText;

    fn file_id() -> FileId {
        FileId::new(1).unwrap()
    }

    fn stream_for(src: &str) -> TriviaStream {
        let st = SourceText::from_bytes(file_id(), src.as_bytes()).unwrap();
        let mut sink = VecSink::new();
        tokenize_with_trivia(file_id(), &st, &mut sink)
    }

    /// Build a one-node arena whose single `Placeholder` node spans
    /// `[start, end)`. The parser is not involved; we just need a span
    /// over the whole source for the attach pass to key off.
    fn single_node_arena(start: u32, end: u32) -> (AstArena, NodeId) {
        let mut arena = AstArena::new();
        let span = Span::new(file_id(), start, end - start);
        let id = arena.alloc(NodeKind::Placeholder, span);
        (arena, id)
    }

    #[test]
    fn leading_on_top_level_let() {
        // "// comment\nlet x" — the "// comment" is leading of the
        // single node that spans the whole source.
        let src = "// comment\nlet x";
        let stream = stream_for(src);
        let (arena, root) = single_node_arena(0, src.len() as u32);
        let map = attach_trivia(&stream, &arena, root);
        let att = map.get(root).expect("root should carry an attachment");
        assert_eq!(att.leading.len(), 1, "{:?}", att);
        assert_eq!(att.leading[0].kind, TriviaKind::LineComment);
        assert!(att.trailing.is_empty());
    }

    #[test]
    fn trailing_on_end_of_line_comment() {
        // "let x // trailing\n" — same-line trailing. The node spans
        // `let x`; the trailing `// trailing` sits in
        // stream.trailing_trivia AFTER the last token.
        let src = "let x // trailing\n";
        let stream = stream_for(src);
        // Node spans just "let x" (first 5 bytes).
        let (arena, root) = single_node_arena(0, 5);
        let map = attach_trivia(&stream, &arena, root);
        let att = map.get(root).expect("root attachment");
        assert_eq!(att.trailing.len(), 1, "{:?}", att);
        assert_eq!(att.trailing[0].kind, TriviaKind::LineComment);
    }

    #[test]
    fn inner_comment_in_block() {
        // Single node spanning the whole source. The "// mid" comment
        // lives in `leading_trivia[2]` — the bucket that sits BETWEEN
        // the first `let` and the second `let` — not adjacent to the
        // node's first token, so it attaches as inner rather than
        // leading. No inner-child competes for ownership, so the
        // comment stays with the outer node.
        let src = "let a // mid\nlet b";
        let stream = stream_for(src);
        let (arena, root) = single_node_arena(0, src.len() as u32);
        let map = attach_trivia(&stream, &arena, root);
        let att = map.get(root).expect("root");
        assert_eq!(att.inner.len(), 1, "{:?}", att);
        assert_eq!(att.inner[0].kind, TriviaKind::LineComment);
        assert!(att.leading.is_empty(), "leading should be empty: {:?}", att);
    }

    #[test]
    fn doc_comment_attaches_as_leading_across_blank_line() {
        // Doc comments ride with the next item regardless of
        // blank-line gap. Here `/// doc` sits two blank lines above
        // the first token — a regular `//` would be dropped by the
        // blank-line rule, but `///` stays as leading.
        let src = "/// doc\n\n\nlet x";
        let stream = stream_for(src);
        let (arena, root) = single_node_arena(0, src.len() as u32);
        let map = attach_trivia(&stream, &arena, root);
        let att = map.get(root).expect("root");
        assert!(
            att.leading.iter().any(|t| t.kind == TriviaKind::DocLineComment),
            "doc comment should ride through blank-line gap; got {:?}",
            att
        );
    }

    #[test]
    fn shadow_inner_absorbs_parent_inner() {
        // Outer covers whole source; inner covers "let b // tail\n".
        // The `// tail` is trailing-of-inner (same line as `b`), so
        // outer's inner must NOT contain it.
        let src = "let a\nlet b // tail\n";
        let stream = stream_for(src);
        let mut arena = AstArena::new();
        let outer_span = Span::new(file_id(), 0, src.len() as u32);
        let outer = arena.alloc(NodeKind::Placeholder, outer_span);
        // "let b" starts at byte 6, is 5 bytes long: 6..11.
        let inner_span = Span::new(file_id(), 6, 5);
        let inner = arena.alloc(NodeKind::Placeholder, inner_span);
        let map = attach_trivia(&stream, &arena, outer);

        let inner_att = map.get(inner).expect("inner");
        assert_eq!(inner_att.trailing.len(), 1, "{:?}", inner_att);
        assert_eq!(inner_att.trailing[0].kind, TriviaKind::LineComment);

        // Outer must not re-claim the shadowed comment.
        if let Some(outer_att) = map.get(outer) {
            assert!(
                outer_att
                    .inner
                    .iter()
                    .all(|t| t.kind != TriviaKind::LineComment),
                "outer inner shouldn't contain the inner-shadowed comment; got {:?}",
                outer_att
            );
        }
    }

    #[test]
    fn blank_line_gap_detaches_leading_non_doc() {
        // Non-doc comment above a BLANK LINE gap must NOT attach as
        // leading. "// stale\n\n\nlet x" — the blank line between
        // "// stale" and "let" severs the attachment, so leading is
        // empty and `// stale` surfaces as a stray (which the root
        // reclaims as inner).
        let src = "// stale\n\n\nlet x";
        let stream = stream_for(src);
        let (arena, root) = single_node_arena(0, src.len() as u32);
        let map = attach_trivia(&stream, &arena, root);
        let att = map.get(root).expect("root");
        assert!(
            att.leading.is_empty(),
            "non-doc comment above blank-line gap should not be leading; got {:?}",
            att
        );
        // It still lives — as a stray reclaimed into root.inner.
        assert!(
            att.inner.iter().any(|t| t.kind == TriviaKind::LineComment),
            "detached comment should resurface as root inner; got {:?}",
            att
        );
    }
}
