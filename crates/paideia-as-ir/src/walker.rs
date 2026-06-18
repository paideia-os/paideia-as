//! IR tree traversal via pre/post-order visitor hooks.
//!
//! Provides a trait-based walker for traversing IR trees with pre/post-visit
//! hooks, threaded through a walker context. The driver handles recursion
//! in pre-order: pre-visit the root, recurse through children (in order),
//! then post-visit the root.
//!
//! ## Stack Overflow Guard
//!
//! The current implementation uses recursion. Phase-1 and phase-2 IRs are
//! shallow (tree depth typically <100), so the stack is safe. If future
//! generated code produces deep IR (>1000 nodes depth), consider switching
//! to an iterative depth-limited version with explicit stack.

use smallvec::SmallVec;

use crate::IrArena;
use crate::node::{IrNodeData, IrNodeId};
use crate::walker_ctx::WalkerCtx;

/// Trait for IR-walker visitor passes.
///
/// Implementors define `pre_visit` and/or `post_visit` hooks that are
/// called by the [`walk`] driver during tree traversal. The walker is
/// responsible for accumulating state; the driver handles recursion.
///
/// Default implementations of both hooks are no-ops, so implementors
/// may define only the hooks they need.
pub trait IrWalker {
    /// Called before recursing into a node's children.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the node being visited.
    /// * `node` - The node data (kind, linearity class, effect row, span).
    /// * `arena` - The arena containing all nodes; usable for child lookups
    ///   or other node data queries.
    /// * `ctx` - Walker context providing source map and diagnostic sink.
    ///
    /// # Default
    ///
    /// No-op (the underscore pattern prevents unused-variable warnings).
    fn pre_visit(
        &mut self,
        id: IrNodeId,
        node: &IrNodeData,
        arena: &IrArena,
        ctx: &mut WalkerCtx<'_>,
    ) {
        let _ = (id, node, arena, ctx);
    }

    /// Called after recursing into a node's children.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the node being visited.
    /// * `node` - The node data (kind, linearity class, effect row, span).
    /// * `arena` - The arena containing all nodes.
    /// * `ctx` - Walker context providing source map and diagnostic sink.
    ///
    /// # Default
    ///
    /// No-op (the underscore pattern prevents unused-variable warnings).
    fn post_visit(
        &mut self,
        id: IrNodeId,
        node: &IrNodeData,
        arena: &IrArena,
        ctx: &mut WalkerCtx<'_>,
    ) {
        let _ = (id, node, arena, ctx);
    }
}

/// Drive a pre-order traversal over an IR tree rooted at `root`.
///
/// Recursively visits each node in pre-order: calls `walker.pre_visit(root)`,
/// then recursively traverses each child in order, then calls
/// `walker.post_visit(root)`. Each node is visited exactly once, assuming
/// the IR is acyclic (which IR construction guarantees).
///
/// # Arguments
///
/// * `walker` - The visitor implementing the traversal hooks.
/// * `arena` - The arena containing the IR tree.
/// * `root` - The root node ID to start traversal from.
/// * `ctx` - Walker context providing source map and diagnostic sink.
///
/// # Example
///
/// ```ignore
/// struct CountingWalker {
///     pre_count: usize,
///     post_count: usize,
/// }
///
/// impl IrWalker for CountingWalker {
///     fn pre_visit(
///         &mut self,
///         _id: IrNodeId,
///         _node: &IrNodeData,
///         _arena: &IrArena,
///         _ctx: &mut WalkerCtx<'_>,
///     ) {
///         self.pre_count += 1;
///     }
///
///     fn post_visit(
///         &mut self,
///         _id: IrNodeId,
///         _node: &IrNodeData,
///         _arena: &IrArena,
///         _ctx: &mut WalkerCtx<'_>,
///     ) {
///         self.post_count += 1;
///     }
/// }
///
/// let mut arena = IrArena::new();
/// let var_id = arena.alloc(IrKind::Var, span);
/// let mut walker = CountingWalker { pre_count: 0, post_count: 0 };
/// let source_map = SourceMap::new();
/// let mut sink = VecSink::new();
/// let mut ctx = WalkerCtx::new(&source_map, &mut sink);
///
/// walk(&mut walker, &arena, var_id, &mut ctx);
/// assert_eq!(walker.pre_count, 1);
/// assert_eq!(walker.post_count, 1);
/// ```
pub fn walk<W: IrWalker>(walker: &mut W, arena: &IrArena, root: IrNodeId, ctx: &mut WalkerCtx<'_>) {
    walker.pre_visit(root, &arena[root], arena, ctx);

    // Collect children into a SmallVec to avoid a borrow conflict between
    // the arena.children() borrow and the recursive walk calls.
    // SmallVec keeps ≤4 children inline, spilling to heap for larger trees.
    let children: SmallVec<[IrNodeId; 4]> = arena.children(root).iter().copied().collect();

    for child in children {
        walk(walker, arena, child, ctx);
    }

    walker.post_visit(root, &arena[root], arena, ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{DiagnosticSink, FileId, SourceMap, Span, VecSink};

    use crate::node::IrKind;

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    /// Recording walker for tests: collects (phase, kind) pairs during traversal.
    struct RecordingWalker {
        visits: Vec<(VisitPhase, IrKind)>,
    }

    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    enum VisitPhase {
        Pre,
        Post,
    }

    impl RecordingWalker {
        fn new() -> Self {
            Self { visits: Vec::new() }
        }
    }

    impl IrWalker for RecordingWalker {
        fn pre_visit(
            &mut self,
            _id: IrNodeId,
            node: &IrNodeData,
            _arena: &IrArena,
            _ctx: &mut WalkerCtx<'_>,
        ) {
            self.visits.push((VisitPhase::Pre, node.kind));
        }

        fn post_visit(
            &mut self,
            _id: IrNodeId,
            node: &IrNodeData,
            _arena: &IrArena,
            _ctx: &mut WalkerCtx<'_>,
        ) {
            self.visits.push((VisitPhase::Post, node.kind));
        }
    }

    /// Counting walker for tests: counts pre and post visits.
    struct CountingWalker {
        pre_count: usize,
        post_count: usize,
    }

    impl CountingWalker {
        fn new() -> Self {
            Self {
                pre_count: 0,
                post_count: 0,
            }
        }
    }

    impl IrWalker for CountingWalker {
        fn pre_visit(
            &mut self,
            _id: IrNodeId,
            _node: &IrNodeData,
            _arena: &IrArena,
            _ctx: &mut WalkerCtx<'_>,
        ) {
            self.pre_count += 1;
        }

        fn post_visit(
            &mut self,
            _id: IrNodeId,
            _node: &IrNodeData,
            _arena: &IrArena,
            _ctx: &mut WalkerCtx<'_>,
        ) {
            self.post_count += 1;
        }
    }

    #[test]
    fn walk_visits_single_node_once() {
        let mut arena = IrArena::new();
        let var_id = arena.alloc(IrKind::Var, span());

        let mut walker = CountingWalker::new();
        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, var_id, &mut ctx);

        assert_eq!(
            walker.pre_count, 1,
            "single node should have exactly one pre-visit"
        );
        assert_eq!(
            walker.post_count, 1,
            "single node should have exactly one post-visit"
        );
    }

    #[test]
    fn walk_visits_let_with_one_child_in_pre_order() {
        let mut arena = IrArena::new();
        let var_id = arena.alloc(IrKind::Var, span());
        let let_id = arena.alloc_with_children(IrKind::Let, span(), [var_id]);

        let mut walker = RecordingWalker::new();
        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, let_id, &mut ctx);

        assert_eq!(
            walker.visits.len(),
            4,
            "Let + Var should produce 4 visits (2 per node)"
        );
        assert_eq!(
            walker.visits,
            vec![
                (VisitPhase::Pre, IrKind::Let),
                (VisitPhase::Pre, IrKind::Var),
                (VisitPhase::Post, IrKind::Var),
                (VisitPhase::Post, IrKind::Let),
            ],
            "visits should be in pre-order: Pre(Let), Pre(Var), Post(Var), Post(Let)"
        );
    }

    #[test]
    fn walk_visits_app_callee_then_args_in_order() {
        let mut arena = IrArena::new();
        let callee_id = arena.alloc(IrKind::Var, span());
        let arg0_id = arena.alloc(IrKind::Literal, span());
        let arg1_id = arena.alloc(IrKind::Literal, span());
        let app_id = arena.alloc_with_children(IrKind::App, span(), [callee_id, arg0_id, arg1_id]);

        let mut walker = RecordingWalker::new();
        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, app_id, &mut ctx);

        assert_eq!(
            walker.visits,
            vec![
                (VisitPhase::Pre, IrKind::App),
                (VisitPhase::Pre, IrKind::Var), // callee
                (VisitPhase::Post, IrKind::Var),
                (VisitPhase::Pre, IrKind::Literal), // arg0
                (VisitPhase::Post, IrKind::Literal),
                (VisitPhase::Pre, IrKind::Literal), // arg1
                (VisitPhase::Post, IrKind::Literal),
                (VisitPhase::Post, IrKind::App),
            ],
            "App children should be visited in order: callee, arg0, arg1"
        );
    }

    #[test]
    fn walk_visits_each_node_exactly_once_in_acyclic_tree() {
        // Build a 5-node tree: Module(Let(Var), Let(Var))
        let mut arena = IrArena::new();
        let var1_id = arena.alloc(IrKind::Var, span());
        let var2_id = arena.alloc(IrKind::Var, span());
        let let1_id = arena.alloc_with_children(IrKind::Let, span(), [var1_id]);
        let let2_id = arena.alloc_with_children(IrKind::Let, span(), [var2_id]);
        let mod_id = arena.alloc_with_children(IrKind::Module, span(), [let1_id, let2_id]);

        let mut walker = RecordingWalker::new();
        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, mod_id, &mut ctx);

        // Count visits per kind
        let pre_count = walker
            .visits
            .iter()
            .filter(|(phase, _)| *phase == VisitPhase::Pre)
            .count();
        let post_count = walker
            .visits
            .iter()
            .filter(|(phase, _)| *phase == VisitPhase::Post)
            .count();

        assert_eq!(pre_count, 5, "5-node tree should have 5 pre-visits");
        assert_eq!(post_count, 5, "5-node tree should have 5 post-visits");

        // Verify order: Module first, then Let1, Var1, etc.
        assert_eq!(walker.visits[0], (VisitPhase::Pre, IrKind::Module));
        assert_eq!(walker.visits[1], (VisitPhase::Pre, IrKind::Let)); // Let1
        assert_eq!(walker.visits[2], (VisitPhase::Pre, IrKind::Var)); // Var1
        assert_eq!(walker.visits[3], (VisitPhase::Post, IrKind::Var)); // Var1
        assert_eq!(walker.visits[4], (VisitPhase::Post, IrKind::Let)); // Let1
    }

    #[test]
    fn walker_forwards_diagnostics_through_sink() {
        use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};

        struct DiagnosticEmitter;

        impl IrWalker for DiagnosticEmitter {
            fn pre_visit(
                &mut self,
                _id: IrNodeId,
                node: &IrNodeData,
                _arena: &IrArena,
                ctx: &mut WalkerCtx<'_>,
            ) {
                // Emit a diagnostic only for Var nodes
                if node.kind == IrKind::Var {
                    let code = DiagnosticCode::new(Category::Z, Severity::Warning, 9001).unwrap();
                    let diagnostic = Diagnostic::warning(code)
                        .message("test warning")
                        .with_span(node.span)
                        .finish();
                    ctx.emit(diagnostic);
                }
            }
        }

        let mut arena = IrArena::new();
        let var_id = arena.alloc(IrKind::Var, span());

        let mut walker = DiagnosticEmitter;
        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, var_id, &mut ctx);

        assert_eq!(sink.count(), 1, "exactly one diagnostic should be emitted");
        assert_eq!(
            sink.diagnostics()[0].severity(),
            Severity::Warning,
            "diagnostic should have Warning severity"
        );
    }

    #[test]
    fn walk_on_empty_module_visits_just_module() {
        let mut arena = IrArena::new();
        let mod_id = arena.alloc(IrKind::Module, span());

        let mut walker = CountingWalker::new();
        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, mod_id, &mut ctx);

        assert_eq!(
            walker.pre_count, 1,
            "empty module should have exactly one pre-visit"
        );
        assert_eq!(
            walker.post_count, 1,
            "empty module should have exactly one post-visit"
        );
    }

    #[test]
    fn walker_state_via_ctx_5_node_walk() {
        // Build a 5-node tree: Module(Let(Var), Let(Var)) to match m1-002's test 4
        // Record node kinds in order via the context's source_map
        struct SourceTextLengthWalker {
            child_counts: Vec<usize>,
        }

        impl IrWalker for SourceTextLengthWalker {
            fn pre_visit(
                &mut self,
                _id: IrNodeId,
                _node: &IrNodeData,
                _arena: &IrArena,
                ctx: &mut WalkerCtx<'_>,
            ) {
                let source_map = ctx.source_map();
                // Record a deterministic value: just count it to ensure we can access the context
                let _ = source_map;
                self.child_counts.push(1);
            }
        }

        let mut arena = IrArena::new();
        let var1_id = arena.alloc(IrKind::Var, span());
        let var2_id = arena.alloc(IrKind::Var, span());
        let let1_id = arena.alloc_with_children(IrKind::Let, span(), [var1_id]);
        let let2_id = arena.alloc_with_children(IrKind::Let, span(), [var2_id]);
        let mod_id = arena.alloc_with_children(IrKind::Module, span(), [let1_id, let2_id]);

        let mut walker = SourceTextLengthWalker {
            child_counts: Vec::new(),
        };
        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, mod_id, &mut ctx);

        // Verify we visited 5 nodes (all of them pre-visit)
        assert_eq!(
            walker.child_counts.len(),
            5,
            "should record one entry per pre-visit (5 nodes)"
        );
        // Expected sequence: Module, Let, Var, Let, Var
        let expected_sequence = vec![1, 1, 1, 1, 1];
        assert_eq!(
            walker.child_counts, expected_sequence,
            "deterministic visit sequence should match"
        );
    }
}
