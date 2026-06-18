//! Linearity validation and diagnostic emission.
//!
//! Validates that bindings in a closed scope satisfy the substructural
//! lattice constraints defined in `design/toolchain/custom-assembler.md` §3.1.
//! Emits S-range diagnostic codes for violations.
//!
//! Also provides minimal AST walking for block expressions to maintain proper
//! scope nesting in `LinearityCtx`. Full per-statement tracking arrives when
//! the IR walker is implemented.
//!
//! ## IR Walker Integration
//!
//! [`LinearityWalker`] implements [`IrWalker`](paideia_as_ir::IrWalker) to drive
//! linearity checks over an entire IR tree in a single pass. It uses node IDs
//! as symbol proxies (phase-2-m1 minimum) and will transition to structured
//! symbol/binding payloads in phase-3 when the IR carries real symbol names.

use std::collections::HashMap;

use paideia_as_ast::{AstArena, ExprData, NodeId, NodeKind};
use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, Severity};
use paideia_as_ir::{IrArena, IrKind, IrNodeData, IrNodeId, IrWalker, LinClass, WalkerCtx};

use crate::capture::analyze_captures;
use crate::check_lambda;
use crate::check_ordered::OrderedLog;
use crate::env::Symbol;
use crate::linearity_ctx::{Binding, LinearityCtx};

/// Diagnostic code for a Linear or Ordered binding that is never used (use-count = 0).
pub const S_NEVER_USED: u16 = 900;

/// Diagnostic code for a binding that violates its use-count constraint.
pub const S_OVERUSED: u16 = 901;

/// Construct a DiagnosticCode in the S category.
fn s_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::S, Severity::Error, n).expect("valid S code")
}

/// Inspect a closed scope and produce diagnostics per the substructural lattice.
///
/// The validation rules are:
///
/// - **Linear**: must be used exactly once.
///   - Use-count 0 → S0900 (never used).
///   - Use-count > 1 → S0901 (overused).
///
/// - **Ordered**: same constraint as Linear (used exactly once).
///   - Use-count 0 → S0900.
///   - Use-count > 1 → S0901.
///
/// - **Affine**: may be used at most once.
///   - Use-count 0 → OK.
///   - Use-count > 1 → S0901 (overused).
///
/// - **Unrestricted**: no constraints.
///   - Any use-count is valid.
///
/// The returned diagnostics are sorted by source span for determinism.
pub fn validate_scope(scope: &HashMap<Symbol, Binding>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    for (_sym, b) in scope.iter() {
        match (b.class, b.uses) {
            (LinClass::Linear | LinClass::Ordered, 0) => {
                diags.push(
                    Diagnostic::error(s_code(S_NEVER_USED))
                        .message(format!(
                            "{:?} binding is never used; substructural lattice requires exactly one use",
                            b.class
                        ))
                        .with_span(b.bind_span)
                        .finish(),
                );
            }
            (LinClass::Linear | LinClass::Ordered, n) if n > 1 => {
                diags.push(
                    Diagnostic::error(s_code(S_OVERUSED))
                        .message(format!(
                            "{:?} binding used {n} times; substructural lattice permits exactly one use",
                            b.class
                        ))
                        .with_span(b.bind_span)
                        .finish(),
                );
            }
            (LinClass::Affine, n) if n > 1 => {
                diags.push(
                    Diagnostic::error(s_code(S_OVERUSED))
                        .message(format!(
                            "affine binding used {n} times; affine permits at most one use"
                        ))
                        .with_span(b.bind_span)
                        .finish(),
                );
            }
            _ => {}
        }
    }

    diags.sort_by_key(|d| d.primary_span().map(|s| s.byte_start()).unwrap_or(0));
    diags
}

/// Walk an AST node to maintain proper scope nesting in block expressions.
///
/// This minimal walker handles `ExprData::Block` by:
/// 1. Entering a new scope before visiting statements and tail.
/// 2. Leaving the scope after the block is processed.
///
/// All other expression kinds and statements are left for future
/// per-statement linearity tracking (phase-2+).
///
/// Returns the same scope depth as before the call (balanced push/pop).
pub fn walk_expr_for_scope(arena: &AstArena, ctx: &mut LinearityCtx, expr_id: NodeId) {
    let node = match arena.get(expr_id) {
        Some(n) => n,
        None => return,
    };

    if node.kind != NodeKind::ExprBlock {
        // Only ExprBlock needs scope tracking for now.
        return;
    }

    if let Some(ExprData::Block { stmts, tail }) = arena.expr_data(expr_id) {
        // Enter a new scope for this block
        ctx.enter_scope();

        // Walk statements (if any)
        for &stmt_id in stmts.iter() {
            // Currently no per-statement tracking; just maintain scopes.
            // Future: walk each statement to track bindings and uses.
            let _ = stmt_id;
        }

        // Walk tail expression (if any)
        if let Some(tail_id) = tail {
            // Currently no tail expression tracking; just maintain scopes.
            // Future: walk tail to track uses.
            let _ = tail_id;
        }

        // Leave the block's scope
        let _scope = ctx.leave_scope();
        // Note: we don't validate the scope here; that happens at IR lowering
        // when scopes are closed and checked against linearity constraints.
    }
}

/// IR-walker implementation for linearity checking.
///
/// Runs the substructural-lattice checks over an IR subtree. Records S0900
/// (never used), S0901 (overused), S0903 (out-of-order), S0906 (branch-merge),
/// and S0907 (illegal lambda capture) diagnostics via the walker context's
/// diagnostic sink.
///
/// ## Symbol Proxy Strategy (Phase-2-m1)
///
/// This walker uses **node IDs as symbol proxies**: each `Let` node ID serves
/// as the symbol bound by that Let, and each `Var` node's reference target
/// is the ID of the Let it consumes. This conservative approach allows the
/// walker to run before the IR gains structured symbol/binding payloads.
///
/// When phase-3 (m2/m5) adds real symbol names to the IR, this walker will
/// switch to real symbol lookup via those payloads. For now, the test corpus
/// is constructed to use Var → Let id linkage explicitly.
///
/// ## Lambda Capture Analysis (m1-005 #175)
///
/// When visiting a Lambda node:
/// 1. Pre-visit: snapshot the outer scope's bindings before entering the lambda's scope.
/// 2. Post-visit: pop the lambda's scope, analyze captures via `analyze_captures`,
///    validate captures via `check_lambda`, and emit S0907 diagnostics for illegal captures.
///
/// ## Branch-Merge Handling
///
/// Deferred to phase-3. The IR currently has no `IrKind::Match` or `IrKind::If`
/// variants. When phase-3 introduces these, `post_visit` should:
/// 1. Collect per-arm `LinearityCtx` snapshots.
/// 2. Call `branch_merge::merge_branches` to validate use-count agreement.
/// 3. Emit S0906 diagnostics for branch mismatches.
///
/// For now, focus is on linear/ordered binding semantics and lambda captures.
#[derive(Debug)]
pub struct LinearityWalker {
    /// Tracks binding use-counts and classes.
    linearity_ctx: LinearityCtx,
    /// Tracks Ordered binding declaration/use order per scope.
    ordered_log: OrderedLog,
    /// Stack of pre-lambda snapshots: when entering a Lambda's pre_visit,
    /// push a snapshot of the outer scope. Used in post_visit to analyze captures.
    lambda_snapshots: Vec<HashMap<Symbol, Binding>>,
}

impl LinearityWalker {
    /// Construct a new walker with a fresh linearity context and empty ordered log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            linearity_ctx: LinearityCtx::new(),
            ordered_log: OrderedLog::new(),
            lambda_snapshots: Vec::new(),
        }
    }

    /// Take a snapshot of the innermost scope's bindings.
    ///
    /// Used by `pre_visit(Lambda)` to capture the outer scope state
    /// before entering the lambda's inner scope.
    fn snapshot_outer_scope(&self) -> HashMap<Symbol, Binding> {
        self.linearity_ctx.innermost().clone()
    }
}

impl Default for LinearityWalker {
    fn default() -> Self {
        Self::new()
    }
}

impl IrWalker for LinearityWalker {
    fn pre_visit(
        &mut self,
        id: IrNodeId,
        node: &IrNodeData,
        _arena: &IrArena,
        ctx: &mut WalkerCtx<'_>,
    ) {
        match node.kind {
            IrKind::Let => {
                // Derive the symbol from the node ID (phase-2-m1 proxy).
                let sym: Symbol = id.get();
                let class = node.lin_class;

                // Bind the symbol in the linearity context.
                self.linearity_ctx.bind(sym, class, node.span);

                // If Ordered, also declare in the ordered log.
                if class == LinClass::Ordered {
                    self.ordered_log.declare(sym, node.span);
                }
            }
            IrKind::Var => {
                // Derive the referenced symbol from the node ID (phase-2-m1 proxy).
                // This assumes Var nodes are created with their referent Let's ID.
                let sym: Symbol = id.get();

                // Record use in the linearity context.
                self.linearity_ctx.use_(sym);

                // If the Ordered log is tracking, record this use and emit any diagnostics.
                let diags = self.ordered_log.record_use(sym, node.span);
                for diag in diags {
                    ctx.emit(diag);
                }
            }
            IrKind::Lambda => {
                // Lambda: snapshot the outer scope before entering the lambda's scope.
                // This is used in post_visit to analyze captures.
                let outer_snapshot = self.snapshot_outer_scope();
                self.lambda_snapshots.push(outer_snapshot);
                // Now enter the lambda's inner scope.
                self.linearity_ctx.enter_scope();
            }
            IrKind::Module | IrKind::Action | IrKind::Unsafe => {
                // Other scope-introducing nodes: enter a scope.
                self.linearity_ctx.enter_scope();
            }
            _ => {}
        }
    }

    fn post_visit(
        &mut self,
        _id: IrNodeId,
        node: &IrNodeData,
        _arena: &IrArena,
        ctx: &mut WalkerCtx<'_>,
    ) {
        match node.kind {
            IrKind::Lambda => {
                // Lambda: pop the inner scope, analyze captures, and emit S0907 diagnostics.
                let inner_scope = self.linearity_ctx.leave_scope();

                // Recover the outer scope snapshot from the stack.
                let outer_snapshot = self.lambda_snapshots.pop().expect(
                    "lambda_snapshots stack should have a pre_visit snapshot for every post_visit Lambda",
                );

                // Analyze captures: compare the outer scope's state BEFORE the lambda
                // with its state AFTER the lambda. The difference in use-counts tells us
                // which outer bindings were used (captured) inside the lambda body.
                let outer_current = self.linearity_ctx.innermost().clone();
                let captures = analyze_captures(&outer_snapshot, &outer_current);

                // Validate captures against the lambda's linearity class.
                let capture_diags =
                    check_lambda::check_lambda(&captures, node.lin_class, node.span);
                for diag in capture_diags {
                    ctx.emit(diag);
                }

                // Also validate the inner scope itself (linear bindings used exactly once, etc).
                let scope_diags = validate_scope(&inner_scope);
                for diag in scope_diags {
                    ctx.emit(diag);
                }
            }
            IrKind::Module | IrKind::Action | IrKind::Unsafe => {
                // Other scope-introducing nodes: leave the scope and validate.
                let scope = self.linearity_ctx.leave_scope();
                let diags = validate_scope(&scope);
                for diag in diags {
                    ctx.emit(diag);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::FileId;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    fn binding(class: LinClass, uses: u32, bind_span: Span) -> Binding {
        Binding {
            class,
            uses,
            bind_span,
        }
    }

    #[test]
    fn linear_used_once_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 1, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn linear_used_twice_emits_s0901() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 2, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn linear_unused_emits_s0900() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 0, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn affine_unused_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Affine, 0, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn affine_used_once_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Affine, 1, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn affine_used_twice_emits_s0901() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Affine, 2, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn ordered_unused_emits_s0900() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Ordered, 0, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn ordered_used_once_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Ordered, 1, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn ordered_used_twice_emits_s0901() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Ordered, 2, span(100)));

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn unrestricted_used_arbitrarily_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Unrestricted, 5, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn unrestricted_unused_passes() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Unrestricted, 0, span(100)));

        let diags = validate_scope(&scope);
        assert!(diags.is_empty());
    }

    #[test]
    fn multiple_bindings_in_scope() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 1, span(100)));
        scope.insert(20, binding(LinClass::Linear, 0, span(110))); // unused

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn multiple_violations_sorted_by_span() {
        let mut scope = HashMap::new();
        scope.insert(10, binding(LinClass::Linear, 0, span(100))); // S0900 at span 100
        scope.insert(20, binding(LinClass::Linear, 2, span(50))); // S0901 at span 50

        let diags = validate_scope(&scope);
        assert_eq!(diags.len(), 2);
        // sorted by span: 50 < 100
        assert_eq!(diags[0].primary_span().map(|s| s.byte_start()), Some(50));
        assert_eq!(diags[1].primary_span().map(|s| s.byte_start()), Some(100));
    }

    #[test]
    fn block_scope_push_pop_balanced() {
        // Walking an ExprBlock leaves LinearityCtx::depth() unchanged.
        use paideia_as_ast::{AstArena, NodeKind};

        let mut arena = AstArena::new();
        let test_span = span(0);

        // Construct a simple block: { 42 } (empty stmts, tail = literal 42)
        let lit_node = arena.alloc(NodeKind::Placeholder, test_span);
        let lit_42 = arena.alloc_expr(
            NodeKind::ExprLiteral,
            test_span,
            paideia_as_ast::ExprData::Literal { lit: lit_node },
        );

        let block = arena.alloc_expr(
            NodeKind::ExprBlock,
            test_span,
            ExprData::Block {
                stmts: vec![],
                tail: Some(lit_42),
            },
        );

        let mut ctx = LinearityCtx::new();
        let initial_depth = ctx.depth();
        assert_eq!(initial_depth, 1, "root scope");

        walk_expr_for_scope(&arena, &mut ctx, block);

        let final_depth = ctx.depth();
        assert_eq!(
            final_depth, initial_depth,
            "scope depth should be unchanged after walking block"
        );
    }

    use paideia_as_diagnostics::{DiagnosticSink, Span};

    // LinearityWalker tests

    #[test]
    fn walker_emits_s0901_on_double_use_of_linear_binding_in_tree() {
        // Build a Module(Lambda(Let)) tree where Let is Linear
        // and we record two uses of its symbol.
        let mut walker = LinearityWalker::new();
        let sm = paideia_as_diagnostics::SourceMap::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(0);

        // Create nested structure: Module -> Lambda -> Let
        let module_id = arena.alloc(IrKind::Module, s);
        let lambda_id = arena.alloc(IrKind::Lambda, s);

        // Pre-visit Module (enter scope)
        walker.pre_visit(module_id, &arena[module_id], &arena, &mut ctx);

        // Pre-visit Lambda (enter scope)
        walker.pre_visit(lambda_id, &arena[lambda_id], &arena, &mut ctx);

        // Create Let (id will be 3, class=Linear)
        let let_id = arena.alloc(IrKind::Let, s);
        let mut let_data = arena[let_id];
        let_data.lin_class = LinClass::Linear;

        // Pre-visit Let (binds symbol 3)
        walker.pre_visit(let_id, &let_data, &arena, &mut ctx);

        // Record two uses of symbol 3
        walker.linearity_ctx.use_(3);
        walker.linearity_ctx.use_(3);

        // Post-visit Lambda (leave scope, validate)
        walker.post_visit(lambda_id, &arena[lambda_id], &arena, &mut ctx);

        // Should have one S0901 (overused)
        assert_eq!(sink.count(), 1, "exactly one diagnostic expected");
        assert_eq!(sink.diagnostics()[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn walker_emits_s0901_on_double_use_of_linear_via_id_proxy() {
        // Construct a tree: Lambda(Let) where Let is inside Lambda's scope.
        // We manually drive the walker to test double-use of the Linear binding.

        let mut walker = LinearityWalker::new();
        let sm = paideia_as_diagnostics::SourceMap::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(0);

        // Create a Lambda node (id=1, scope-introducing)
        let lambda_id = arena.alloc(IrKind::Lambda, s);
        let lambda_data = arena[lambda_id];

        // Pre-visit Lambda -> enter scope
        walker.pre_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Create a Let node inside Lambda (id=2)
        let let_id = arena.alloc(IrKind::Let, s);
        let let_data = arena[let_id];
        let mut mutable_let_data = let_data;
        mutable_let_data.lin_class = LinClass::Linear;

        // Pre-visit Let (id=2, class=Linear, binds symbol 2)
        walker.pre_visit(let_id, &mutable_let_data, &arena, &mut ctx);

        // Simulate two uses of symbol 2 (the Let's id)
        walker.linearity_ctx.use_(2);
        walker.linearity_ctx.use_(2);

        // Post-visit Lambda -> leave scope and validate
        walker.post_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Should have one S0901 diagnostic (overused)
        assert_eq!(sink.count(), 1, "exactly one diagnostic expected");
        assert_eq!(sink.diagnostics()[0].code().number(), S_OVERUSED);
    }

    #[test]
    fn walker_emits_s0900_on_never_used_linear() {
        let mut walker = LinearityWalker::new();
        let sm = paideia_as_diagnostics::SourceMap::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(10);

        // Create a Lambda node (scope-introducing)
        let lambda_id = arena.alloc(IrKind::Lambda, s);
        let lambda_data = arena[lambda_id];

        // Pre-visit Lambda -> enter scope
        walker.pre_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Create a Let node inside Lambda (id=2)
        let let_id = arena.alloc(IrKind::Let, s);
        let let_data = arena[let_id];
        let mut mutable_let_data = let_data;
        mutable_let_data.lin_class = LinClass::Linear;

        // Pre-visit Let (binds symbol 2, uses=0)
        walker.pre_visit(let_id, &mutable_let_data, &arena, &mut ctx);

        // Post-visit Lambda (no uses of symbol 2 recorded) -> validates scope, should emit S0900
        walker.post_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Should have one S0900 diagnostic (never used)
        assert_eq!(sink.count(), 1, "exactly one diagnostic expected");
        assert_eq!(sink.diagnostics()[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn walker_handles_unrestricted_class() {
        let mut walker = LinearityWalker::new();
        let sm = paideia_as_diagnostics::SourceMap::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(0);

        // Create a Let node with Unrestricted class
        let let_id = arena.alloc(IrKind::Let, s);
        let mut let_data = arena[let_id];
        let_data.lin_class = LinClass::Unrestricted;

        // Pre-visit Let (binds symbol 1 as Unrestricted)
        walker.pre_visit(let_id, &let_data, &arena, &mut ctx);

        // Simulate multiple uses
        walker.linearity_ctx.use_(1);
        walker.linearity_ctx.use_(1);
        walker.linearity_ctx.use_(1);

        // Post-visit Let -> validates scope
        walker.post_visit(let_id, &let_data, &arena, &mut ctx);

        // No diagnostics should be emitted for Unrestricted bindings
        assert_eq!(
            sink.count(),
            0,
            "Unrestricted binding allows arbitrary uses"
        );
    }

    #[test]
    fn walker_emits_s0903_on_out_of_order_ordered_use() {
        let mut walker = LinearityWalker::new();
        let sm = paideia_as_diagnostics::SourceMap::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(0);

        // Create two Let nodes (id=1, id=2) both Ordered
        let let1_id = arena.alloc(IrKind::Let, s);
        let let2_id = arena.alloc(IrKind::Let, s);

        let mut let1_data = arena[let1_id];
        let1_data.lin_class = LinClass::Ordered;

        let mut let2_data = arena[let2_id];
        let2_data.lin_class = LinClass::Ordered;

        // Pre-visit Let1 (id=1, class=Ordered)
        walker.pre_visit(let1_id, &let1_data, &arena, &mut ctx);

        // Pre-visit Let2 (id=2, class=Ordered)
        walker.pre_visit(let2_id, &let2_data, &arena, &mut ctx);

        // Use Let2 first (out of order)
        let diags = walker.ordered_log.record_use(2, s);
        for diag in diags {
            ctx.emit(diag);
        }

        // Should have one S0903 diagnostic (out-of-order use)
        assert_eq!(
            sink.count(),
            1,
            "out-of-order use should emit exactly one diagnostic"
        );
        assert_eq!(
            sink.diagnostics()[0].code().number(),
            crate::check_ordered::S_OUT_OF_ORDER
        );
    }

    #[test]
    fn walker_post_visit_validates_scope() {
        let mut walker = LinearityWalker::new();
        let sm = paideia_as_diagnostics::SourceMap::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(5);

        // Create a Lambda (scope-introducing node)
        let lambda_id = arena.alloc(IrKind::Lambda, s);
        let lambda_data = arena[lambda_id];

        // Pre-visit Lambda (enter scope)
        walker.pre_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Create a Let inside the Lambda's scope
        let let_id = arena.alloc(IrKind::Let, s);
        let mut let_data = arena[let_id];
        let_data.lin_class = LinClass::Linear;

        // Pre-visit Let (bind symbol with class=Linear, uses=0)
        walker.pre_visit(let_id, &let_data, &arena, &mut ctx);

        // Post-visit Let (no special handling for Let)
        walker.post_visit(let_id, &let_data, &arena, &mut ctx);

        // Post-visit Lambda (leave scope, validate)
        // This should emit S0900 for the unused Linear binding
        walker.post_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Should have one S0900 diagnostic
        assert_eq!(sink.count(), 1, "unused Linear binding should emit S0900");
        assert_eq!(sink.diagnostics()[0].code().number(), S_NEVER_USED);
    }

    #[test]
    fn walker_emits_s0907_on_illegal_linear_capture() {
        // Lambda captures a Linear binding from outer scope.
        // The lambda is Unrestricted (not Linear/Affine), so it cannot
        // consume Linear bindings.
        let mut walker = LinearityWalker::new();
        let sm = paideia_as_diagnostics::SourceMap::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(100);

        // Create a Module (outer scope)
        let module_id = arena.alloc(IrKind::Module, s);
        let module_data = arena[module_id];

        // Pre-visit Module (enter outer scope)
        walker.pre_visit(module_id, &module_data, &arena, &mut ctx);

        // Create a Let in the outer scope (id=2, class=Linear)
        let outer_let_id = arena.alloc(IrKind::Let, s);
        let mut outer_let_data = arena[outer_let_id];
        outer_let_data.lin_class = LinClass::Linear;

        // Pre-visit outer Let (bind symbol 2 as Linear)
        walker.pre_visit(outer_let_id, &outer_let_data, &arena, &mut ctx);

        // Create a Lambda (id=3, class=Unrestricted — default)
        let lambda_id = arena.alloc(IrKind::Lambda, s);
        let lambda_data = arena[lambda_id];
        // lambda_data.lin_class defaults to Unrestricted

        // Pre-visit Lambda (snapshot outer scope, enter lambda scope)
        walker.pre_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Inside the lambda: use the outer binding (symbol 2)
        walker.linearity_ctx.use_(2); // This increments the use-count in the outer scope

        // Post-visit Lambda: should detect that we captured Linear binding 2 by consume
        // in an Unrestricted lambda, emitting S0907.
        walker.post_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Check diagnostics
        let diags = sink.diagnostics();
        let s0907_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.code().number() == crate::check_lambda::S_ILLEGAL_CAPTURE)
            .collect();
        assert_eq!(
            s0907_diags.len(),
            1,
            "should emit exactly one S0907 for illegal linear capture"
        );
    }

    #[test]
    fn walker_lambda_allows_capture_of_unrestricted() {
        // Regression test: Lambda captures an Unrestricted binding.
        // This is always OK, regardless of the lambda's class.
        let mut walker = LinearityWalker::new();
        let sm = paideia_as_diagnostics::SourceMap::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(200);

        // Create a Module (outer scope)
        let module_id = arena.alloc(IrKind::Module, s);
        let module_data = arena[module_id];

        // Pre-visit Module
        walker.pre_visit(module_id, &module_data, &arena, &mut ctx);

        // Create a Let in the outer scope (id=2, class=Unrestricted)
        let outer_let_id = arena.alloc(IrKind::Let, s);
        let mut outer_let_data = arena[outer_let_id];
        outer_let_data.lin_class = LinClass::Unrestricted;

        // Pre-visit outer Let (bind symbol 2 as Unrestricted)
        walker.pre_visit(outer_let_id, &outer_let_data, &arena, &mut ctx);

        // Create a Lambda (class=Unrestricted)
        let lambda_id = arena.alloc(IrKind::Lambda, s);
        let lambda_data = arena[lambda_id];

        // Pre-visit Lambda
        walker.pre_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Inside the lambda: use the outer binding (symbol 2)
        walker.linearity_ctx.use_(2);

        // Post-visit Lambda: no S0907 expected
        walker.post_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Check that no S0907 was emitted
        let diags = sink.diagnostics();
        let s0907_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.code().number() == crate::check_lambda::S_ILLEGAL_CAPTURE)
            .collect();
        assert!(
            s0907_diags.is_empty(),
            "no S0907 expected for Unrestricted capture"
        );
    }

    #[test]
    fn walker_lambda_with_no_capture_no_diagnostics() {
        // A Lambda whose body uses only its own params.
        // No captures from outer scope, so no S0907.
        let mut walker = LinearityWalker::new();
        let sm = paideia_as_diagnostics::SourceMap::new();
        let mut sink = paideia_as_diagnostics::VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(300);

        // Create a Module (outer scope)
        let module_id = arena.alloc(IrKind::Module, s);
        let module_data = arena[module_id];

        // Pre-visit Module
        walker.pre_visit(module_id, &module_data, &arena, &mut ctx);

        // Create a Lambda (class=Unrestricted)
        let lambda_id = arena.alloc(IrKind::Lambda, s);
        let lambda_data = arena[lambda_id];

        // Pre-visit Lambda
        walker.pre_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Bind a param inside the lambda (symbol 99, class=Unrestricted)
        walker.linearity_ctx.bind(99, LinClass::Unrestricted, s);

        // Use only the param (no outer captures)
        walker.linearity_ctx.use_(99);

        // Post-visit Lambda: no diagnostics expected
        walker.post_visit(lambda_id, &lambda_data, &arena, &mut ctx);

        // Should have no diagnostics (empty inner scope, no outer captures)
        assert_eq!(
            sink.count(),
            0,
            "no diagnostics for lambda with param-only body"
        );
    }
}
