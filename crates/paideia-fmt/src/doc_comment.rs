//! Doc-comment grouping and attachment pass (FMT-9, paideia-as#1573).
//!
//! Consumes the per-node [`TriviaAttachment`] produced by FMT-1b
//! ([`crate::attach::attach_trivia`]) and re-partitions its `leading`
//! comment list into semantic *doc blocks*: runs of consecutive doc
//! comments that share rhetorical scope. FMT-1b stores doc trivia
//! verbatim; FMT-9 answers "which doc block belongs to which node, in
//! what position?" so that FMT-4..8 structural formatters can emit the
//! block as one unit (indented, with the right line break discipline).
//!
//! # Scope — what FMT-9 adds on top of FMT-1b
//!
//! * **Grouping** — consecutive `///` lines with no blank-line gap form
//!   a single [`DocBlock`] (style spec #1561 §9.2). A `/** */` block
//!   doc is a single-line block.
//! * **`//!` / `/*!` module-doc re-homing** — these are *inner* docs
//!   that attach to the *enclosing scope* (the module/structure
//!   containing the node whose leading they currently sit in), not to
//!   the item that follows them. The lexer classifies `//!` as a plain
//!   [`TriviaKind::LineComment`] (not `DocLineComment`) because its
//!   third byte is `!` rather than `/` — see
//!   [`paideia_as_lexer::scan_comment`] — so FMT-9 recognises the
//!   style by scanning the raw source bytes.
//! * **Blank-line detach** — a doc block with ≥2 newlines between its
//!   last comment and the node's first token attaches to the *enclosing
//!   scope*'s inner, not to the node that follows (§9.2 edge case).
//!
//! # Output surface
//!
//! The pass writes [`DocBlock`]s into [`TriviaAttachment::doc_blocks`]
//! via [`TriviaAttachmentMap::push_doc_block`]. The raw `leading` /
//! `trailing` / `inner` buckets from FMT-1b remain untouched: FMT-9 is
//! an *additive* semantic view, so byte-level fallback paths that walk
//! raw trivia keep working. The format-emission layer (FMT-4..8) is
//! expected to prefer `doc_blocks` over the raw `leading` doc-comment
//! trivia when it emits structural output. See
//! [`crate::walker::emit_outer_doc_blocks`] and
//! [`crate::walker::emit_inner_doc_blocks`] for the dormant emission
//! helpers.
//!
//! # Invariants
//!
//! * One `DocBlock` per rhetorical group. Grouping terminates at the
//!   first of (a) a blank-line gap (≥2 newlines in the source bytes
//!   between adjacent doc comments), (b) a style change
//!   (outer `///`/`/**` ↔ inner `//!`/`/*!`), or (c) a non-doc comment
//!   intervening in the trivia list.
//! * Exact wording is preserved — [`DocBlock::lines`] stores raw source
//!   slices verbatim, including the leading `///` / `//!` / `/** */`
//!   marker. No reflow, no capitalization change, no whitespace
//!   stripping.
//! * The pass is idempotent: running it on a map that already has
//!   `doc_blocks` populated leaves them in place; it only *appends*
//!   newly-derived blocks. Normal use is a single call.
//! * The raw trivia in `leading` / `inner` is NOT removed when the FMT-9
//!   pass promotes it to a `DocBlock`. This keeps the pass additive —
//!   an emitter that preferred the raw view would still see what FMT-1b
//!   produced. FMT-4..8 structural emitters are expected to prefer
//!   `doc_blocks` and skip doc-comment trivia from `leading`.

use paideia_as_ast::{AstArena, NodeId};
use paideia_as_lexer::{SourceText, Trivia, TriviaKind};

use crate::attach::{TriviaAttachment, TriviaAttachmentMap};

/// A grouped run of doc comments that share rhetorical scope.
///
/// See the module header for grouping rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocBlock {
    /// Raw source text of each line in the block, in source order.
    /// A `///` or `//!` line contributes one entry; a `/** */` or
    /// `/*! */` block contributes one entry carrying the whole block
    /// verbatim (callers that want to split on `\n` for width control
    /// do so themselves). The leading marker (`///` etc.) is kept so
    /// a round-trip emitter does not have to reconstruct it.
    pub lines: Vec<String>,
    /// `true` for `//!` / `/*!` module-level inner docs, `false` for
    /// `///` / `/**` outer docs attached to the following item.
    pub is_inner: bool,
}

impl DocBlock {
    /// Construct an empty block with the given polarity. Convenience
    /// for building test fixtures; the attachment pass itself builds
    /// blocks line-by-line.
    #[must_use]
    pub fn new(is_inner: bool) -> Self {
        Self {
            lines: Vec::new(),
            is_inner,
        }
    }

    /// Number of comment lines in the block.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// `true` iff the block has no lines.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Group every attached node's leading doc comments into [`DocBlock`]s
/// and re-home `//!` / `/*!` blocks to their enclosing scope.
///
/// # Arguments
///
/// * `attachment_map` — the FMT-1b output; mutated in place to add
///   [`TriviaAttachment::doc_blocks`] entries. Non-doc buckets are not
///   touched.
/// * `arena` — the parsed AST, used to resolve each node's span (for
///   blank-line gap detection) and to find the enclosing node (for
///   `//!` re-homing).
/// * `source` — raw source text; needed to classify `//!` vs `//` (the
///   lexer merges them) and to count newlines in gap regions.
///
/// # Determinism
///
/// `node_ids()` is HashMap-ordered, so pass order is unspecified; the
/// output is still deterministic *per node* because within each node
/// the leading list is in source order. Cross-node writes
/// (`//!` re-homing) only ever append to another node's `doc_blocks`,
/// never overwrite, so order of blocks on the receiver matches order
/// of discovery across source nodes — which is HashMap-dependent. If a
/// caller needs a stable receiver order, sort `attachment_map.node_ids()`
/// externally and iterate those before calling.
pub fn attach_doc_blocks(
    attachment_map: &mut TriviaAttachmentMap,
    arena: &AstArena,
    source: &SourceText,
) {
    // Snapshot ids so we can mutate the map while iterating.
    let ids = attachment_map.node_ids();
    let content = source.content();

    // Pre-compute node spans for O(1) lookup during re-homing.
    let node_span = |id: NodeId| arena.get(id).map(|n| n.span);

    // Collect (receiver_id, block) pending writes; apply after the
    // read pass to avoid borrow-checker gymnastics.
    let mut pending: Vec<(NodeId, DocBlock)> = Vec::new();

    for owner_id in &ids {
        // Pull the leading list out of the map without extending the
        // borrow across the group-building code (which may mutate
        // attachment_map via pending→apply later).
        let leading: Vec<Trivia> = match attachment_map.get(*owner_id) {
            Some(att) => att.leading.clone(),
            None => continue,
        };
        if leading.is_empty() {
            continue;
        }
        let owner_span_start = match node_span(*owner_id) {
            Some(s) => s.byte_start(),
            None => continue,
        };

        // Group leading doc trivia into runs: (vec_of_doc_trivia,
        // is_inner, start_byte, end_byte_of_last_doc_in_run,
        // split_reason). The split_reason tells us whether this run
        // was terminated by a blank-line gap vs the end of the leading
        // list — we only care that the *final* run's gap to the owner
        // start determines the detach decision; earlier runs that
        // split off a non-adjacent gap are always "detached".
        let groups = group_doc_runs(&leading, content);
        if groups.is_empty() {
            continue;
        }

        for (gi, g) in groups.iter().enumerate() {
            let is_final_group = gi + 1 == groups.len();

            // Decide the receiver:
            // * `//!` / `/*!` runs always re-home to the enclosing scope.
            // * `///` / `/**` runs attach to this owner UNLESS they sit
            //   above a blank-line gap (§9.2) — the detach fires only
            //   when the gap between this run's last byte and the
            //   NEXT run (or the owner's first byte, for the final
            //   run) is ≥2 newlines. A style-change split
            //   (`///` → `//!`) is NOT a blank-line gap, so the
            //   `///` run stays attached to the owner even though a
            //   `//!` run follows.
            let detach = if g.is_inner {
                true
            } else {
                let gap_end = if is_final_group {
                    owner_span_start as usize
                } else {
                    // Start of the next run's first doc trivium.
                    groups[gi + 1]
                        .trivia
                        .first()
                        .map(|t| t.span.byte_start() as usize)
                        .unwrap_or(owner_span_start as usize)
                };
                let gap_start = g.end as usize;
                let gap_newlines = count_newlines(&content[gap_start..gap_end]);
                gap_newlines >= 2
            };

            let receiver = if detach {
                enclosing_node(arena, *owner_id, *owner_id).unwrap_or(*owner_id)
            } else {
                *owner_id
            };

            // `is_inner` on the DocBlock is set from the comment style
            // for `//!` / `/*!` runs. For `///` runs that got detached,
            // we also mark them `is_inner = true` because they now live
            // in the enclosing scope's rhetorical "inner" slot — the
            // emitter uses `is_inner` to pick between leading and
            // inner slot placement, not to classify the original
            // comment syntax.
            let block = DocBlock {
                lines: g
                    .trivia
                    .iter()
                    .map(|t| slice(content, t.span.byte_start(), t.span.byte_end()).to_string())
                    .collect(),
                is_inner: g.is_inner || (detach && receiver != *owner_id),
            };
            if !block.lines.is_empty() {
                pending.push((receiver, block));
            }
        }
    }

    for (id, block) in pending {
        attachment_map.push_doc_block(id, block);
    }
}

/// A grouped run of doc-comment trivia inside a single node's leading
/// list, together with the metadata the attachment decision needs.
struct DocRun {
    trivia: Vec<Trivia>,
    is_inner: bool,
    end: u32,
}

/// Partition `leading` (comment-only Trivia list from FMT-1b) into
/// doc-comment runs separated by blank-line gaps (≥2 newlines in the
/// intervening source bytes) or by style changes (outer ↔ inner) or
/// by any non-doc comment intervening.
fn group_doc_runs(leading: &[Trivia], content: &str) -> Vec<DocRun> {
    let mut out: Vec<DocRun> = Vec::new();
    let mut cur: Vec<Trivia> = Vec::new();
    let mut cur_is_inner: bool = false;
    let mut cur_end: u32 = 0;

    for t in leading {
        let polarity = classify_doc(t, content);
        match polarity {
            Some(is_inner) => {
                if let Some(last) = cur.last() {
                    // Decide whether to start a new run.
                    let gap_newlines = count_newlines(
                        &content[last.span.byte_end() as usize..t.span.byte_start() as usize],
                    );
                    let style_change = cur_is_inner != is_inner;
                    if gap_newlines >= 2 || style_change {
                        out.push(DocRun {
                            trivia: std::mem::take(&mut cur),
                            is_inner: cur_is_inner,
                            end: cur_end,
                        });
                        cur_is_inner = is_inner;
                    }
                } else {
                    cur_is_inner = is_inner;
                }
                cur_end = t.span.byte_end();
                cur.push(t.clone());
            }
            None => {
                // A non-doc comment flushes any open run (grouping does
                // not jump over foreign trivia).
                if !cur.is_empty() {
                    out.push(DocRun {
                        trivia: std::mem::take(&mut cur),
                        is_inner: cur_is_inner,
                        end: cur_end,
                    });
                }
            }
        }
    }
    if !cur.is_empty() {
        out.push(DocRun {
            trivia: cur,
            is_inner: cur_is_inner,
            end: cur_end,
        });
    }
    out
}

/// `Some(is_inner)` iff `t` is a doc comment; `None` otherwise.
///
/// `//!` / `/*!` are detected by scanning the raw source bytes because
/// the lexer classifies them as plain [`TriviaKind::LineComment`] /
/// [`TriviaKind::BlockComment`] — their third byte is `!`, not `/` or
/// `*`, so `scan_comment` does not stamp them as `DocLine` /
/// `DocBlock`. FMT-9 recognises the style at the source level and
/// routes them as inner docs.
fn classify_doc(t: &Trivia, content: &str) -> Option<bool> {
    let start = t.span.byte_start() as usize;
    let bytes = content.as_bytes();
    match t.kind {
        TriviaKind::DocLineComment | TriviaKind::DocBlockComment => Some(false),
        TriviaKind::LineComment => {
            if bytes.get(start) == Some(&b'/')
                && bytes.get(start + 1) == Some(&b'/')
                && bytes.get(start + 2) == Some(&b'!')
            {
                Some(true)
            } else {
                None
            }
        }
        TriviaKind::BlockComment => {
            if bytes.get(start) == Some(&b'/')
                && bytes.get(start + 1) == Some(&b'*')
                && bytes.get(start + 2) == Some(&b'!')
            {
                Some(true)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Count `\n` bytes in `s`. CRLF is a single newline for counting
/// purposes (we see the `\n`); lone CR is rejected upstream by
/// `SourceText::from_bytes`.
fn count_newlines(s: &str) -> usize {
    s.bytes().filter(|&b| b == b'\n').count()
}

/// Smallest-span node in `arena` whose span strictly contains `child`'s
/// span and is not `child` itself. Returns `None` if no such node
/// exists (e.g. `child` is the file root). `exclude` is a NodeId that
/// must not be returned — used to prevent re-attaching to the same
/// node that owns the trivia.
fn enclosing_node(arena: &AstArena, child: NodeId, exclude: NodeId) -> Option<NodeId> {
    let child_span = arena.get(child)?.span;
    let (cs, ce) = (child_span.byte_start(), child_span.byte_end());
    let mut best: Option<(NodeId, u32)> = None;
    for raw in 1..=u32::try_from(arena.len()).unwrap_or(u32::MAX) {
        let Some(id) = NodeId::new(raw) else { continue };
        if id == exclude {
            continue;
        }
        let Some(nd) = arena.get(id) else { continue };
        let s = nd.span;
        let (ps, pe) = (s.byte_start(), s.byte_end());
        // Strict containment: parent span must cover child AND be
        // strictly larger (equal-span siblings do not qualify).
        if ps <= cs && pe >= ce && (pe - ps) > (ce - cs) {
            let len = pe - ps;
            if best.map_or(true, |(_, blen)| len < blen) {
                best = Some((id, len));
            }
        }
    }
    best.map(|(id, _)| id)
}

/// Defensive byte-slice: returns an empty string if the range is out
/// of bounds. The attachment pass never produces an out-of-bounds
/// range in practice (spans come from the lexer and the parser), but
/// the formatter is a non-fatal path and silent fallback is preferred
/// over a panic.
fn slice(content: &str, start: u32, end: u32) -> &str {
    let s = start as usize;
    let e = end as usize;
    if s <= content.len() && e <= content.len() && s <= e {
        &content[s..e]
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attach::attach_trivia;
    use crate::cst::tokenize_with_trivia;
    use paideia_as_ast::{AstArena, NodeKind};
    use paideia_as_diagnostics::{FileId, Span, VecSink};

    fn file_id() -> FileId {
        FileId::new(1).unwrap()
    }

    fn source(src: &str) -> SourceText {
        SourceText::from_bytes(file_id(), src.as_bytes()).unwrap()
    }

    /// Build an arena with a single Placeholder node spanning the whole
    /// source. Convenient for leading-only tests that do not need a
    /// nested parent/child relationship.
    fn single_node_arena(src: &str) -> (AstArena, NodeId) {
        let mut arena = AstArena::new();
        let span = Span::new(file_id(), 0, src.len() as u32);
        let id = arena.alloc(NodeKind::Placeholder, span);
        (arena, id)
    }

    /// Build an arena with an outer node spanning the whole source and
    /// an inner node spanning `[inner_start, inner_end)`. Returns
    /// `(arena, outer, inner)`. The outer is allocated first so that
    /// (small-first) processing in attach_trivia claims trivia for
    /// `inner` before `outer`.
    fn outer_inner_arena(src: &str, inner_start: u32, inner_end: u32) -> (AstArena, NodeId, NodeId) {
        let mut arena = AstArena::new();
        let outer_span = Span::new(file_id(), 0, src.len() as u32);
        let outer = arena.alloc(NodeKind::Placeholder, outer_span);
        let inner_span = Span::new(file_id(), inner_start, inner_end - inner_start);
        let inner = arena.alloc(NodeKind::Placeholder, inner_span);
        (arena, outer, inner)
    }

    /// Build attach map and run FMT-9 doc-block pass. Returns the
    /// mutated map so tests can assert on `doc_blocks`.
    fn run_fmt9(src: &str, arena: &AstArena, root: NodeId) -> TriviaAttachmentMap {
        let st = source(src);
        let mut sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &st, &mut sink);
        let mut map = attach_trivia(&stream, arena, root);
        attach_doc_blocks(&mut map, arena, &st);
        map
    }

    #[test]
    fn single_doc_line_attaches_as_outer_block() {
        let src = "/// one\nlet x = 1";
        let (arena, root) = single_node_arena(src);
        let map = run_fmt9(src, &arena, root);
        let att = map.get(root).expect("root attachment");
        assert_eq!(att.doc_blocks.len(), 1, "{:?}", att.doc_blocks);
        let blk = &att.doc_blocks[0];
        assert!(!blk.is_inner);
        assert_eq!(blk.lines, vec!["/// one".to_string()]);
    }

    #[test]
    fn multi_line_doc_block_groups_into_one() {
        let src = "/// one\n/// two\n/// three\nlet x";
        let (arena, root) = single_node_arena(src);
        let map = run_fmt9(src, &arena, root);
        let att = map.get(root).expect("root attachment");
        assert_eq!(att.doc_blocks.len(), 1, "{:?}", att.doc_blocks);
        assert_eq!(att.doc_blocks[0].lines.len(), 3);
        assert_eq!(att.doc_blocks[0].lines[0], "/// one");
        assert_eq!(att.doc_blocks[0].lines[2], "/// three");
        assert!(!att.doc_blocks[0].is_inner);
    }

    #[test]
    fn module_doc_bang_rehomes_to_enclosing() {
        // `//! inner doc` lives in leading of the inner (let) node per
        // FMT-1b, but FMT-9 moves it to the enclosing outer node as an
        // inner doc block. The outer does not even have an attachment
        // entry before FMT-9; `push_doc_block` creates one.
        let src = "//! module-doc\nlet x = 1";
        let (arena, outer, inner) = outer_inner_arena(src, 15, 24);
        let map = run_fmt9(src, &arena, outer);

        let outer_att = map.get(outer).expect("outer must receive the inner doc");
        assert_eq!(outer_att.doc_blocks.len(), 1, "{:?}", outer_att.doc_blocks);
        assert!(outer_att.doc_blocks[0].is_inner);
        assert_eq!(outer_att.doc_blocks[0].lines, vec!["//! module-doc".to_string()]);

        // Inner should NOT carry the //! as an outer doc block.
        let inner_doc_blocks: Vec<_> = map
            .get(inner)
            .map(|a| a.doc_blocks.clone())
            .unwrap_or_default();
        assert!(
            inner_doc_blocks.iter().all(|b| b.is_inner || b.lines != vec!["//! module-doc".to_string()]),
            "inner node must not carry the //! as outer leading: {:?}",
            inner_doc_blocks
        );
    }

    #[test]
    fn doc_block_separated_by_blank_line_detaches_to_enclosing() {
        // `/// a` sits 2 blank lines above `let`. FMT-1b still keeps
        // it as leading (doc-always-attaches rule). FMT-9 sees the gap
        // and detaches it: the block moves to the enclosing node, with
        // `is_inner = true` to flag the detachment.
        let src = "/// stale\n\n\nlet x = 1";
        let (arena, outer, inner) = outer_inner_arena(src, 11, 20);
        let map = run_fmt9(src, &arena, outer);

        let outer_att = map.get(outer).expect("outer must receive the detached doc");
        assert!(
            outer_att.doc_blocks.iter().any(|b| b.is_inner && b.lines == vec!["/// stale".to_string()]),
            "detached /// should become inner on enclosing: {:?}",
            outer_att.doc_blocks
        );

        // Inner should not have the stale /// as an outer doc.
        if let Some(inner_att) = map.get(inner) {
            assert!(
                !inner_att
                    .doc_blocks
                    .iter()
                    .any(|b| !b.is_inner && b.lines == vec!["/// stale".to_string()]),
                "inner node must not keep the detached /// as outer: {:?}",
                inner_att.doc_blocks
            );
        }
    }

    #[test]
    fn two_doc_blocks_split_on_blank_line() {
        // `/// a\n\n\n/// b\nlet x` — the blank-line gap splits the
        // doc trivia into two blocks. The first is detached (inner on
        // enclosing); the second is adjacent (outer on the item).
        let src = "/// a\n\n\n/// b\nlet x";
        let (arena, outer, inner) = outer_inner_arena(src, 14, 19);
        let map = run_fmt9(src, &arena, outer);

        // Inner (adjacent group) should carry /// b as outer.
        let inner_att = map.get(inner).expect("inner attachment");
        assert!(
            inner_att
                .doc_blocks
                .iter()
                .any(|b| !b.is_inner && b.lines == vec!["/// b".to_string()]),
            "inner must carry /// b as outer: {:?}",
            inner_att.doc_blocks
        );
        // Outer (detached group) should carry /// a as inner.
        let outer_att = map.get(outer).expect("outer attachment");
        assert!(
            outer_att
                .doc_blocks
                .iter()
                .any(|b| b.is_inner && b.lines == vec!["/// a".to_string()]),
            "outer must carry /// a as inner: {:?}",
            outer_att.doc_blocks
        );
    }

    #[test]
    fn nested_module_bang_attaches_to_nested_module() {
        // Two levels of nesting: outer (whole file), middle (bytes
        // 7..42), inner (bytes 23..40). The //! lives in inner's
        // leading per FMT-1b, but it should re-home to the smallest
        // *strict* enclosing node, which is `middle`.
        let src = "struct { //! m\nlet x = 1\nlet y = 2 }";
        //         0        9      15        24
        // Rough geometry; we lay the inner spans on the let tokens.
        let mut arena = AstArena::new();
        let outer_span = Span::new(file_id(), 0, src.len() as u32);
        let outer = arena.alloc(NodeKind::Placeholder, outer_span);
        // "middle" spans "{ ... }"
        let middle_span = Span::new(file_id(), 7, (src.len() - 8) as u32);
        let middle = arena.alloc(NodeKind::Placeholder, middle_span);
        // "inner" is "let x = 1"
        let inner_pos = src.find("let x = 1").unwrap() as u32;
        let inner_span = Span::new(file_id(), inner_pos, "let x = 1".len() as u32);
        let inner = arena.alloc(NodeKind::Placeholder, inner_span);
        let _ = inner; // silence unused if asserts drop.

        let st = source(src);
        let mut sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &st, &mut sink);
        let mut map = attach_trivia(&stream, &arena, outer);
        attach_doc_blocks(&mut map, &arena, &st);

        let middle_att = map.get(middle).expect("middle must receive //! m");
        assert!(
            middle_att
                .doc_blocks
                .iter()
                .any(|b| b.is_inner && b.lines.first().map_or(false, |l| l.starts_with("//!"))),
            "middle must carry //! as inner block: {:?}",
            middle_att.doc_blocks
        );
    }

    #[test]
    fn round_trip_preserves_exact_wording() {
        // The DocBlock stores raw source slices verbatim. The exact
        // byte sequence (including leading `///`, trailing spaces, and
        // casing) must be preserved.
        let src = "///  Weird  SPACING   \nlet x = 1";
        let (arena, root) = single_node_arena(src);
        let map = run_fmt9(src, &arena, root);
        let att = map.get(root).expect("root");
        assert_eq!(att.doc_blocks.len(), 1);
        assert_eq!(att.doc_blocks[0].lines[0], "///  Weird  SPACING   ");
    }

    #[test]
    fn block_doc_comment_single_entry() {
        // A /** */ doc block is a single-entry DocBlock carrying the
        // whole slash-asterisk block verbatim.
        let src = "/** block doc */\nlet x = 1";
        let (arena, root) = single_node_arena(src);
        let map = run_fmt9(src, &arena, root);
        let att = map.get(root).expect("root");
        assert_eq!(att.doc_blocks.len(), 1, "{:?}", att.doc_blocks);
        assert!(!att.doc_blocks[0].is_inner);
        assert_eq!(att.doc_blocks[0].lines, vec!["/** block doc */".to_string()]);
    }

    #[test]
    fn no_doc_comments_leaves_doc_blocks_empty() {
        // Byte-identity guard: an input with no doc comments should
        // not get any doc_blocks entries.
        let src = "// plain\nlet x = 1";
        let (arena, root) = single_node_arena(src);
        let map = run_fmt9(src, &arena, root);
        // Either no entry, or an entry with empty doc_blocks.
        let empty = map
            .get(root)
            .map_or(true, |a| a.doc_blocks.is_empty());
        assert!(empty, "no doc comments should produce no doc blocks");
    }

    #[test]
    fn mixed_outer_then_inner_splits_on_style() {
        // `/// outer\n//! inner\nlet x` — different styles end the run.
        // Even without a blank line, outer and inner must land in
        // separate blocks.
        let src = "/// outer\n//! inner\nlet x";
        let (arena, outer, inner) = outer_inner_arena(src, 20, 25);
        let map = run_fmt9(src, &arena, outer);

        let inner_att = map.get(inner).expect("inner");
        assert!(
            inner_att
                .doc_blocks
                .iter()
                .any(|b| !b.is_inner && b.lines == vec!["/// outer".to_string()]),
            "/// outer must land on inner as outer-doc: {:?}",
            inner_att.doc_blocks
        );
        let outer_att = map.get(outer).expect("outer");
        assert!(
            outer_att
                .doc_blocks
                .iter()
                .any(|b| b.is_inner && b.lines == vec!["//! inner".to_string()]),
            "//! inner must re-home to outer scope: {:?}",
            outer_att.doc_blocks
        );
    }

    #[test]
    fn enclosing_node_prefers_smallest_strict_container() {
        // Two candidate containers of different sizes: outer (file),
        // middle (inside outer but containing inner). //! should land
        // on middle, not outer.
        let src = "aaa //! mod\nlet x = 1 bbb";
        let mut arena = AstArena::new();
        let outer_span = Span::new(file_id(), 0, src.len() as u32);
        let outer = arena.alloc(NodeKind::Placeholder, outer_span);
        let middle_span = Span::new(file_id(), 3, (src.len() - 7) as u32);
        let middle = arena.alloc(NodeKind::Placeholder, middle_span);
        let inner_pos = src.find("let x = 1").unwrap() as u32;
        let inner_span = Span::new(file_id(), inner_pos, "let x = 1".len() as u32);
        let _inner = arena.alloc(NodeKind::Placeholder, inner_span);

        let st = source(src);
        let mut sink = VecSink::new();
        let stream = tokenize_with_trivia(file_id(), &st, &mut sink);
        let mut map = attach_trivia(&stream, &arena, outer);
        attach_doc_blocks(&mut map, &arena, &st);

        let middle_att = map.get(middle).expect("middle receives //!");
        assert!(
            middle_att
                .doc_blocks
                .iter()
                .any(|b| b.is_inner && b.lines == vec!["//! mod".to_string()]),
            "//! mod must land on middle (smallest strict enclosing): {:?}",
            middle_att.doc_blocks
        );

        // Outer should not duplicate the //! block.
        let outer_doc_blocks: Vec<_> = map
            .get(outer)
            .map(|a| a.doc_blocks.clone())
            .unwrap_or_default();
        assert!(
            !outer_doc_blocks
                .iter()
                .any(|b| b.lines == vec!["//! mod".to_string()]),
            "//! must not also attach to outer: {:?}",
            outer_doc_blocks
        );
    }
}
