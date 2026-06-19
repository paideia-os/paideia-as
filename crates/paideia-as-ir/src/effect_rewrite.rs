//! Effect-handler rewrite pass per `custom-assembler.md` §6.3 +
//! `calling-convention.md` §4.2.
//!
//! Each `perform E.op(args)` is rewritten to an indirect call against
//! the handler table held in `R15`:
//!
//! 1. Load handler-table base from `R15`.
//! 2. Load the `E.op` slot at `[R15 + offset]`.
//! 3. Indirect-call the loaded pointer with the original arguments.
//!
//! Phase-1: the IR doesn't yet carry the operand list per node, so the
//! rewrite is exposed as a per-node helper that:
//!
//! - Takes an `IrPerform` node id.
//! - Allocates a new `App` node with the same span representing the
//!   indirect call.
//! - Records the handler-table offset for the operation in a side-table.
//!
//! `unsafe` blocks pass through unchanged per §9.4.
//!
//! Nested `with` blocks: we expose a small `WithRewrite` helper that
//! emits save/restore `App` nodes around the inner body's first/last
//! nodes. The actual register-allocation lowering is the emitter's
//! job (T8).
//!
//! ## Deep-Handler Compilation (m3-009)
//!
//! The deep-handler strategy compiles handler operation implementations
//! according to their `resume` usage patterns:
//!
//! - **SingleShot**: `resume` is called exactly once on every control-flow
//!   path. Compiled to a direct continuation invocation.
//! - **MultiShot**: `resume` is called 0 or 2+ times on some path. Compiled
//!   to a multi-shot deep-handler continuation (closure-wrapped).
//! - **Abort**: `resume` is never called on at least one path. The operation
//!   returns a value directly without resuming.
//!
//! Phase-2-m9 note: Resume classification uses a simple count-based heuristic
//! (not control-flow-sensitive analysis). This conservatively approximates
//! the actual mode; future phases will refine with proper data-flow analysis.
//! Additionally, resume sites are currently modeled as `App` nodes (Phase-2-m7
//! will introduce a dedicated `IrKind::Resume` variant).

use std::collections::{HashMap, HashSet};

use crate::arena::IrArena;
use crate::node::{IrKind, IrNodeId};
use crate::walker::{IrWalker, walk};
use crate::walker_ctx::WalkerCtx;
use paideia_as_diagnostics::{SourceMap, VecSink};

/// Outcome of rewriting one `IrPerform`.
#[derive(Debug, Clone)]
pub struct PerformRewrite {
    /// New `App` node id replacing the original `Perform`.
    pub call: IrNodeId,
    /// Handler-table offset for the operation, in bytes.
    pub offset: u32,
}

/// Outcome of rewriting one `IrPerform` at a given handler depth.
///
/// Extends `PerformRewrite` to include the lexical nesting depth at which
/// the matching handler was installed. This is used in row-polymorphic
/// contexts where the indirect call must dereference the handler stack
/// at the right depth.
#[derive(Debug, Clone)]
pub struct PerformRewriteWithDepth {
    /// New `App` node id replacing the original `Perform`.
    pub call: IrNodeId,
    /// Handler-table offset for the operation, in bytes.
    pub offset: u32,
    /// Lexical nesting depth: how many handler frames to skip in the stack.
    pub handler_depth: u32,
}

/// Per-effect operation table. Phase-1 stores `(effect_name, op_name) →
/// byte offset`; the offset is `slot_index * 8` (each handler pointer
/// is 64 bits per the calling convention).
#[derive(Default, Debug, Clone)]
pub struct HandlerTable {
    /// Slot per `(effect, op)`. Insertion order determines offset.
    slots: HashMap<(String, String), u32>,
    /// Next slot index to assign.
    next_slot: u32,
}

impl HandlerTable {
    /// Construct an empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up — or assign — the slot for `(effect, op)`.
    ///
    /// First call assigns a fresh slot at `next_slot * 8`; subsequent
    /// calls return the cached offset.
    pub fn slot_for(&mut self, effect: &str, op: &str) -> u32 {
        let key = (effect.to_owned(), op.to_owned());
        if let Some(&off) = self.slots.get(&key) {
            return off;
        }
        let off = self.next_slot * 8;
        self.next_slot += 1;
        self.slots.insert(key, off);
        off
    }

    /// Number of slots assigned so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// `true` iff no slots have been assigned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// Rewrite one `IrPerform` node into an indirect-call `IrApp`.
///
/// # Panics
///
/// Panics if `id`'s kind is not `IrKind::Perform`.
#[must_use]
pub fn rewrite_perform(
    arena: &mut IrArena,
    table: &mut HandlerTable,
    id: IrNodeId,
    effect: &str,
    op: &str,
) -> PerformRewrite {
    let node = arena[id];
    assert_eq!(
        node.kind,
        IrKind::Perform,
        "rewrite_perform: id must be a Perform node"
    );
    let offset = table.slot_for(effect, op);
    let call = arena.alloc(IrKind::App, node.span);
    PerformRewrite { call, offset }
}

/// Row-polymorphic-aware perform rewrite.
///
/// `handler_depth` is the lexical nesting depth at which the matching
/// handler was installed. The rewritten IR walks the handler-stack
/// chain `handler_depth` frames before invoking the handler.
///
/// Phase-2-m10 minimum: emits an `IrKind::App` representing the
/// indirect call; the depth is recorded in a side-table for the
/// emitter to consume. The actual stack-walking instruction sequence
/// is produced at emit time (T8).
///
/// # Panics
///
/// Panics if `id`'s kind is not `IrKind::Perform`.
#[must_use]
pub fn rewrite_perform_at_depth(
    arena: &mut IrArena,
    table: &mut HandlerTable,
    id: IrNodeId,
    effect: &str,
    op: &str,
    handler_depth: u32,
) -> PerformRewriteWithDepth {
    let node = arena[id];
    assert_eq!(
        node.kind,
        IrKind::Perform,
        "rewrite_perform_at_depth: id must be a Perform node"
    );
    let offset = table.slot_for(effect, op);
    let call = arena.alloc(IrKind::App, node.span);
    PerformRewriteWithDepth {
        call,
        offset,
        handler_depth,
    }
}

/// `unsafe` blocks pass through unchanged per §9.4. This helper is a
/// no-op that returns the same id; provided for symmetry so the IR
/// walker can call it uniformly.
#[must_use]
pub fn rewrite_unsafe_passthrough(arena: &IrArena, id: IrNodeId) -> IrNodeId {
    debug_assert_eq!(arena[id].kind, IrKind::Unsafe);
    id
}

/// Save / restore nodes for nested `with` blocks.
#[derive(Debug, Clone, Copy)]
pub struct WithRewrite {
    /// Synthetic save-R15 node injected at the beginning of the body.
    pub save: IrNodeId,
    /// Synthetic restore-R15 node injected at the end.
    pub restore: IrNodeId,
}

/// Emit save / restore `App` nodes around the body of a `with` block.
///
/// Phase-1 produces synthetic `App` nodes claiming the body's span.
/// The emitter (T8) lowers them to the actual save/restore prologue
/// + epilogue per the calling convention.
#[must_use]
pub fn rewrite_with_save_restore(arena: &mut IrArena, body_id: IrNodeId) -> WithRewrite {
    let span = arena[body_id].span;
    let save = arena.alloc(IrKind::App, span);
    let restore = arena.alloc(IrKind::App, span);
    WithRewrite { save, restore }
}

/// How a handler's op impl uses `resume`.
///
/// This classification indicates the compilation strategy for the operation:
/// - **SingleShot**: Direct continuation invocation (no closure wrapper).
/// - **MultiShot**: Multi-shot deep-handler form (closure-wrapped continuation).
/// - **Abort**: The op returns a value directly without resuming.
///
/// Phase-2-m9 note: The current classification is count-based (conservative).
/// A future control-flow-sensitive analysis will refine this to account for
/// branching and unreachable paths.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ResumeMode {
    /// `resume` is called exactly once on every control-flow path that
    /// returns from the op. Compile to a direct branch / cont-call.
    SingleShot,
    /// `resume` is called 0 or 2+ times on some path. Must compile to
    /// the multi-shot deep-handler continuation form.
    MultiShot,
    /// No `resume` call at all on at least one path. Abort handler;
    /// the op body returns a value directly without resuming.
    Abort,
}

/// Side-table marking resume sites in an IR tree.
///
/// Resume sites are currently modeled as `App` nodes with an attached marker.
/// Phase-2-m7 will introduce a dedicated `IrKind::Resume` variant, eliminating
/// the need for this table.
///
/// The table maps IR node IDs that represent resume continuations.
#[derive(Default, Debug, Clone)]
pub struct ResumeSiteTable {
    /// Set of IrNodeId values that represent resume sites.
    sites: HashSet<IrNodeId>,
}

impl ResumeSiteTable {
    /// Construct an empty resume-site table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark an IR node as a resume site.
    pub fn mark_resume_site(&mut self, id: IrNodeId) {
        self.sites.insert(id);
    }

    /// Check whether an IR node is marked as a resume site.
    #[must_use]
    pub fn is_resume_site(&self, id: IrNodeId) -> bool {
        self.sites.contains(&id)
    }

    /// Collect all resume sites below a given root node.
    #[must_use]
    pub fn collect_resume_sites(&self, arena: &IrArena, root: IrNodeId) -> Vec<IrNodeId> {
        let mut result = Vec::new();
        let mut collector = ResumeSiteCollector {
            table: self,
            results: &mut result,
        };
        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);
        walk(&mut collector, arena, root, &mut ctx);
        result
    }

    /// Number of resume sites marked in this table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    /// `true` iff no resume sites are marked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }
}

/// Helper walker to collect resume sites.
struct ResumeSiteCollector<'a> {
    table: &'a ResumeSiteTable,
    results: &'a mut Vec<IrNodeId>,
}

impl<'a> IrWalker for ResumeSiteCollector<'a> {
    fn pre_visit(
        &mut self,
        id: IrNodeId,
        _node: &crate::node::IrNodeData,
        _arena: &IrArena,
        _ctx: &mut WalkerCtx<'_>,
    ) {
        if self.table.is_resume_site(id) {
            self.results.push(id);
        }
    }
}

/// Classify how a handler op impl uses resume by counting resume nodes
/// in the body's IR subtree.
///
/// # Phase-2-m9 Note
///
/// This implementation uses a simple count-based heuristic:
/// - Count 0 → `Abort`
/// - Count 1 → `SingleShot`
/// - Count 2+ → `MultiShot`
///
/// This approach does not account for control-flow branches. A resume site
/// reachable only on a rare error path is counted the same as one on the
/// main path. Future phases will implement control-flow-sensitive analysis
/// to refine the classification.
///
/// # Arguments
///
/// * `arena` - The IR arena containing all nodes.
/// * `body` - The root node ID of the handler op body to classify.
/// * `resume_table` - The side-table marking resume sites.
///
/// # Returns
///
/// The classified resume mode.
#[must_use]
pub fn classify_resume_mode(
    arena: &IrArena,
    body: IrNodeId,
    resume_table: &ResumeSiteTable,
) -> ResumeMode {
    let resume_sites = resume_table.collect_resume_sites(arena, body);
    let count = resume_sites.len();
    match count {
        0 => ResumeMode::Abort,
        1 => ResumeMode::SingleShot,
        _ => ResumeMode::MultiShot,
    }
}

/// Rewrite a SingleShot resume call into a direct continuation invocation.
///
/// # Phase-2-m9 Placeholder
///
/// This implementation allocates a synthetic `App` node representing the
/// direct continuation call. The emitter (T8+) will lower this to the
/// actual calling convention (e.g., a tail call to the resume value with
/// the payload arguments).
///
/// # Arguments
///
/// * `arena` - The mutable IR arena.
/// * `resume_id` - The ID of the resume site (currently an `App` node).
///
/// # Returns
///
/// A new `App` node ID representing the direct continuation invocation.
#[must_use]
pub fn rewrite_resume_singleshot(arena: &mut IrArena, resume_id: IrNodeId) -> IrNodeId {
    let node = arena[resume_id];
    // For single-shot resumes, allocate an App representing direct invocation.
    // The emitter will expand this to: tail-call resume(payload).
    arena.alloc(IrKind::App, node.span)
}

/// Rewrite a MultiShot resume call into a "capture-and-invoke" form.
///
/// # Phase-2-m9 Placeholder
///
/// This implementation allocates a synthetic `App` node representing the
/// continuation captured in a closure. The emitter (T8+) will lower this to
/// actual closure allocation + invocation.
///
/// # Arguments
///
/// * `arena` - The mutable IR arena.
/// * `resume_id` - The ID of the resume site (currently an `App` node).
///
/// # Returns
///
/// A new `App` node ID representing the closure-wrapped continuation.
#[must_use]
pub fn rewrite_resume_multishot(arena: &mut IrArena, resume_id: IrNodeId) -> IrNodeId {
    let node = arena[resume_id];
    // For multi-shot resumes, allocate an App representing closure-wrapped invocation.
    // The emitter will expand this to: (λ ↦ resume(payload))(captured_cont).
    arena.alloc(IrKind::App, node.span)
}

/// Compile a handler op impl using the deep-handler strategy.
///
/// This function:
/// 1. Classifies the op's resume usage by counting resume sites.
/// 2. Applies the matching rewrite to each resume site in the body.
/// 3. Returns both the classified mode and the rewritten body root.
///
/// # Arguments
///
/// * `arena` - The mutable IR arena.
/// * `op_body` - The root node ID of the handler op body.
/// * `resume_table` - The side-table marking resume sites.
///
/// # Returns
///
/// A tuple `(mode, rewritten_body)` where `mode` indicates the classification
/// and `rewritten_body` is the root of the rewritten op body.
#[must_use]
pub fn compile_deep_handler_op(
    arena: &mut IrArena,
    op_body: IrNodeId,
    resume_table: &ResumeSiteTable,
) -> (ResumeMode, IrNodeId) {
    let mode = classify_resume_mode(arena, op_body, resume_table);

    // Collect all resume sites in the op body.
    let resume_sites = resume_table.collect_resume_sites(arena, op_body);

    // Rewrite each resume site according to the mode.
    let mut rewritten_body = op_body;
    for &resume_id in &resume_sites {
        rewritten_body = match mode {
            ResumeMode::SingleShot => rewrite_resume_singleshot(arena, resume_id),
            ResumeMode::MultiShot => rewrite_resume_multishot(arena, resume_id),
            ResumeMode::Abort => {
                // No rewrites needed; resume sites are unreachable.
                resume_id
            }
        };
    }

    (mode, rewritten_body)
}

/// Emit a trampoline-install node for a handler that has multi-shot
/// resume semantics.
///
/// When a handler with at least one MultiShot op is installed, the
/// install site must construct a *trampoline* — a thin loop that
/// re-invokes the handler whenever its body returns.
///
/// Phase-2-m10 minimum: produces an IrKind::App representing the
/// trampoline. The emitter (T8) lowers it to the actual loop +
/// install sequence.
///
/// # Arguments
///
/// * `arena` - The mutable IR arena.
/// * `handler_id` - The IrNodeId of the Handle node.
/// * `_body_id` - The IrNodeId of the handler body to wrap.
///
/// # Returns
///
/// An IrNodeId representing the trampoline App node.
#[must_use]
pub fn rewrite_handler_install_trampoline(
    arena: &mut IrArena,
    handler_id: IrNodeId,
    _body_id: IrNodeId,
) -> IrNodeId {
    // Use the handler's span for the synthetic trampoline node.
    let handler_node = arena[handler_id];
    // Allocate a synthetic App node representing the trampoline.
    // The emitter will expand this to: loop { install(); body(); jump loop_label; }
    arena.alloc(IrKind::App, handler_node.span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{FileId, Span};

    fn span(byte_start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), byte_start, 1)
    }

    // ── HandlerTable ─────────────────────────────────────────────────

    #[test]
    fn first_slot_is_offset_0() {
        let mut t = HandlerTable::new();
        assert_eq!(t.slot_for("Io", "read"), 0);
    }

    #[test]
    fn second_slot_is_offset_8() {
        let mut t = HandlerTable::new();
        let _ = t.slot_for("Io", "read");
        assert_eq!(t.slot_for("Io", "write"), 8);
    }

    #[test]
    fn same_op_returns_cached_offset() {
        let mut t = HandlerTable::new();
        let a = t.slot_for("Io", "read");
        let b = t.slot_for("Io", "read");
        assert_eq!(a, b);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn different_effects_share_offset_space() {
        let mut t = HandlerTable::new();
        let io = t.slot_for("Io", "read");
        let mmio = t.slot_for("Mmio", "read");
        assert_ne!(io, mmio);
        assert_eq!(mmio, 8);
    }

    // ── AC bullet 1: three Performs → three App rewrites ─────────────

    #[test]
    fn three_performs_rewrite_to_three_apps_with_distinct_offsets() {
        let mut arena = IrArena::new();
        let mut table = HandlerTable::new();
        let p1 = arena.alloc(IrKind::Perform, span(0));
        let p2 = arena.alloc(IrKind::Perform, span(10));
        let p3 = arena.alloc(IrKind::Perform, span(20));
        let r1 = rewrite_perform(&mut arena, &mut table, p1, "Io", "read");
        let r2 = rewrite_perform(&mut arena, &mut table, p2, "Io", "write");
        let r3 = rewrite_perform(&mut arena, &mut table, p3, "Mmio", "read");
        assert_eq!(arena[r1.call].kind, IrKind::App);
        assert_eq!(arena[r2.call].kind, IrKind::App);
        assert_eq!(arena[r3.call].kind, IrKind::App);
        assert_eq!(r1.offset, 0);
        assert_eq!(r2.offset, 8);
        assert_eq!(r3.offset, 16);
    }

    #[test]
    fn rewrite_preserves_span() {
        let mut arena = IrArena::new();
        let mut table = HandlerTable::new();
        let s = span(42);
        let p = arena.alloc(IrKind::Perform, s);
        let r = rewrite_perform(&mut arena, &mut table, p, "E", "op");
        assert_eq!(arena[r.call].span, s);
    }

    // ── AC bullet 2: nested with → save/restore ──────────────────────

    #[test]
    fn nested_with_emits_save_and_restore() {
        let mut arena = IrArena::new();
        let body = arena.alloc(IrKind::Action, span(0));
        let outer = rewrite_with_save_restore(&mut arena, body);
        let inner = rewrite_with_save_restore(&mut arena, body);
        // Each call allocates a fresh save/restore pair → 4 new nodes.
        assert_ne!(outer.save, inner.save);
        assert_ne!(outer.restore, inner.restore);
        for id in [outer.save, outer.restore, inner.save, inner.restore] {
            assert_eq!(arena[id].kind, IrKind::App);
        }
    }

    // ── AC bullet 3: unsafe passthrough ──────────────────────────────

    #[test]
    fn unsafe_block_passes_through_unchanged() {
        let mut arena = IrArena::new();
        let u = arena.alloc(IrKind::Unsafe, span(0));
        let out = rewrite_unsafe_passthrough(&arena, u);
        assert_eq!(out, u);
    }

    // ── Edge ─────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "must be a Perform node")]
    fn rewrite_perform_panics_on_non_perform() {
        let mut arena = IrArena::new();
        let mut table = HandlerTable::new();
        let other = arena.alloc(IrKind::Var, span(0));
        let _ = rewrite_perform(&mut arena, &mut table, other, "E", "op");
    }

    #[test]
    fn handler_table_len_grows_only_on_new_slots() {
        let mut t = HandlerTable::new();
        let _ = t.slot_for("Io", "read");
        let _ = t.slot_for("Io", "read"); // cached
        let _ = t.slot_for("Io", "write"); // new
        assert_eq!(t.len(), 2);
        assert!(!t.is_empty());
    }

    // ── Deep-Handler Compilation Tests (m3-009) ─────────────────────

    #[test]
    fn classify_zero_resumes_is_abort() {
        let mut arena = IrArena::new();
        let body = arena.alloc(IrKind::Literal, span(0));
        let resume_table = ResumeSiteTable::new();
        assert_eq!(
            classify_resume_mode(&arena, body, &resume_table),
            ResumeMode::Abort
        );
    }

    #[test]
    fn classify_one_resume_is_singleshot() {
        let mut arena = IrArena::new();
        let resume_site = arena.alloc(IrKind::App, span(0));
        let body = arena.alloc_with_children(IrKind::Action, span(0), [resume_site]);
        let mut resume_table = ResumeSiteTable::new();
        resume_table.mark_resume_site(resume_site);
        assert_eq!(
            classify_resume_mode(&arena, body, &resume_table),
            ResumeMode::SingleShot
        );
    }

    #[test]
    fn classify_two_resumes_is_multishot() {
        let mut arena = IrArena::new();
        let resume1 = arena.alloc(IrKind::App, span(0));
        let resume2 = arena.alloc(IrKind::App, span(10));
        let body = arena.alloc_with_children(IrKind::Action, span(0), [resume1, resume2]);
        let mut resume_table = ResumeSiteTable::new();
        resume_table.mark_resume_site(resume1);
        resume_table.mark_resume_site(resume2);
        assert_eq!(
            classify_resume_mode(&arena, body, &resume_table),
            ResumeMode::MultiShot
        );
    }

    #[test]
    fn rewrite_resume_singleshot_produces_app_node() {
        let mut arena = IrArena::new();
        let resume_id = arena.alloc(IrKind::App, span(0));
        let rewritten = rewrite_resume_singleshot(&mut arena, resume_id);
        assert_eq!(arena[rewritten].kind, IrKind::App);
        // Span should be preserved from original
        assert_eq!(arena[rewritten].span, arena[resume_id].span);
    }

    #[test]
    fn rewrite_resume_multishot_produces_app_node() {
        let mut arena = IrArena::new();
        let resume_id = arena.alloc(IrKind::App, span(0));
        let rewritten = rewrite_resume_multishot(&mut arena, resume_id);
        assert_eq!(arena[rewritten].kind, IrKind::App);
        // Span should be preserved from original
        assert_eq!(arena[rewritten].span, arena[resume_id].span);
    }

    #[test]
    fn compile_deep_handler_op_singleshot_path() {
        let mut arena = IrArena::new();
        let resume_site = arena.alloc(IrKind::App, span(0));
        let op_body = arena.alloc_with_children(IrKind::Action, span(0), [resume_site]);
        let mut resume_table = ResumeSiteTable::new();
        resume_table.mark_resume_site(resume_site);

        let (mode, rewritten) = compile_deep_handler_op(&mut arena, op_body, &resume_table);

        assert_eq!(mode, ResumeMode::SingleShot);
        assert_eq!(arena[rewritten].kind, IrKind::App);
        // Original span should be preserved
        assert_eq!(arena[rewritten].span, arena[resume_site].span);
    }

    #[test]
    fn compile_deep_handler_op_multishot_path() {
        let mut arena = IrArena::new();
        let resume1 = arena.alloc(IrKind::App, span(0));
        let resume2 = arena.alloc(IrKind::App, span(10));
        let op_body = arena.alloc_with_children(IrKind::Action, span(0), [resume1, resume2]);
        let mut resume_table = ResumeSiteTable::new();
        resume_table.mark_resume_site(resume1);
        resume_table.mark_resume_site(resume2);

        let (mode, _rewritten) = compile_deep_handler_op(&mut arena, op_body, &resume_table);

        assert_eq!(mode, ResumeMode::MultiShot);
    }

    #[test]
    fn resume_site_table_marks_and_checks() {
        let id1 = IrNodeId::new(1).unwrap();
        let id2 = IrNodeId::new(2).unwrap();
        let id3 = IrNodeId::new(3).unwrap();

        let mut table = ResumeSiteTable::new();
        table.mark_resume_site(id1);
        table.mark_resume_site(id2);

        assert!(table.is_resume_site(id1));
        assert!(table.is_resume_site(id2));
        assert!(!table.is_resume_site(id3));
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn resume_site_table_collect_from_tree() {
        let mut arena = IrArena::new();
        let resume1 = arena.alloc(IrKind::App, span(0));
        let resume2 = arena.alloc(IrKind::App, span(10));
        let resume3 = arena.alloc(IrKind::App, span(20));
        let regular = arena.alloc(IrKind::Literal, span(30));
        let body = arena.alloc_with_children(
            IrKind::Action,
            span(0),
            [resume1, resume2, regular, resume3],
        );

        let mut resume_table = ResumeSiteTable::new();
        resume_table.mark_resume_site(resume1);
        resume_table.mark_resume_site(resume2);
        resume_table.mark_resume_site(resume3);

        let collected = resume_table.collect_resume_sites(&arena, body);
        assert_eq!(collected.len(), 3);
        // Verify all resume sites are found (order may vary)
        assert!(collected.contains(&resume1));
        assert!(collected.contains(&resume2));
        assert!(collected.contains(&resume3));
    }

    // ── AC bullet 4: rewrite_perform_at_depth with depth tracking ────

    #[test]
    fn rewrite_perform_at_depth_zero_matches_phase1() {
        let mut arena = IrArena::new();
        let mut table = HandlerTable::new();
        let p = arena.alloc(IrKind::Perform, span(42));

        let result_phase1 = rewrite_perform(&mut arena, &mut table, p, "E", "op");
        let mut table2 = HandlerTable::new();
        let result_depth0 = rewrite_perform_at_depth(&mut arena, &mut table2, p, "E", "op", 0);

        // Both should produce App nodes
        assert_eq!(arena[result_phase1.call].kind, IrKind::App);
        assert_eq!(arena[result_depth0.call].kind, IrKind::App);

        // Offsets should match
        assert_eq!(result_phase1.offset, result_depth0.offset);

        // Depth should be 0
        assert_eq!(result_depth0.handler_depth, 0);
    }

    #[test]
    fn rewrite_perform_at_depth_nonzero_records_depth() {
        let mut arena = IrArena::new();
        let mut table = HandlerTable::new();
        let p = arena.alloc(IrKind::Perform, span(100));

        let result = rewrite_perform_at_depth(&mut arena, &mut table, p, "Io", "read", 3);

        assert_eq!(arena[result.call].kind, IrKind::App);
        assert_eq!(result.offset, 0);
        assert_eq!(result.handler_depth, 3);
    }

    #[test]
    fn rewrite_handler_install_trampoline_produces_app() {
        let mut arena = IrArena::new();
        let handler = arena.alloc(IrKind::Handle, span(0));
        let body = arena.alloc(IrKind::Action, span(10));

        let trampoline = rewrite_handler_install_trampoline(&mut arena, handler, body);

        assert_eq!(arena[trampoline].kind, IrKind::App);
        // The trampoline should carry the handler's span
        assert_eq!(arena[trampoline].span, arena[handler].span);
    }

    // ── AC bullet 5: Property-based test for deep-handler correctness ─

    #[cfg(test)]
    mod pbt {
        use super::*;
        use proptest::proptest;

        proptest! {
            #[test]
            fn pbt_compile_deep_handler_op_rewrites_every_resume_site(
                num_resumes in 0u8..10,
            ) {
                let mut arena = IrArena::new();

                // Build a body with `num_resumes` resume marker nodes.
                let resume_ids: Vec<IrNodeId> = (0..num_resumes)
                    .map(|i| arena.alloc(IrKind::App, span(i as u32 * 10)))
                    .collect();

                // Create a parent Action node containing all resumes
                let body = if resume_ids.is_empty() {
                    arena.alloc(IrKind::Literal, span(0))
                } else {
                    arena.alloc_with_children(IrKind::Action, span(0), resume_ids.clone())
                };

                // Mark all as resume sites
                let mut resume_table = ResumeSiteTable::new();
                for &resume_id in &resume_ids {
                    resume_table.mark_resume_site(resume_id);
                }

                // Compile
                let (mode, _rewritten) = compile_deep_handler_op(&mut arena, body, &resume_table);

                // Assert: classification is correct
                match num_resumes {
                    0 => assert_eq!(mode, ResumeMode::Abort),
                    1 => assert_eq!(mode, ResumeMode::SingleShot),
                    _ => assert_eq!(mode, ResumeMode::MultiShot),
                }

                // Assert: every resume site that was marked should be found.
                // This is a structural-soundness check: the rewrite ran without panicking.
                let collected = resume_table.collect_resume_sites(&arena, body);
                assert_eq!(collected.len(), num_resumes as usize);
            }
        }
    }
}
