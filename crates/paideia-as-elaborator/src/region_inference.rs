//! Lexical region inference.
//!
//! Each let-bound borrow gets a region matching its lexical scope.
//! Function-parameter borrows get the call's outermost region.
//!
//! Phase-4-m5-002 minimum: ship the walker + the region assignment.
//! The actual outlives-relation seeding into the RegionGraph activates
//! with m6 borrow checker; today the walker emits debug-dump style.
//!
//! Phase-4-m5-004: RegionInferenceWalker integrates with PositionIndex to
//! record region_id at borrow expression sites during elaboration.

use paideia_as_ir::{IrArena, IrKind, IrNodeData, IrNodeId, IrWalker, WalkerCtx};
use paideia_as_types::{RegionGraph, RegionId, RegionInterner};

use crate::position_index::{ByteOffset, PositionEntry};
use crate::walker_pass_state::PositionIndexWriter;

/// Walks lexical scopes and assigns regions to borrows.
///
/// - `interner`: allocates fresh RegionIds.
/// - `graph`: records outlives-relations.
/// - `scope_stack`: current lexical scope chain. RegionId::STATIC is the root.
pub struct RegionInferenceWalker {
    interner: RegionInterner,
    graph: RegionGraph,
    scope_stack: Vec<RegionId>,
}

impl RegionInferenceWalker {
    /// Create a new region-inference walker.
    ///
    /// Starts with RegionId::STATIC as the outermost scope.
    pub fn new() -> Self {
        Self {
            interner: RegionInterner::new(),
            graph: RegionGraph::new(),
            scope_stack: vec![RegionId::STATIC],
        }
    }

    /// Enter a new lexical scope.
    ///
    /// Returns the fresh RegionId for the new scope.
    /// The new inner scope is recorded as being outlived by the current outer scope.
    pub fn enter_scope(&mut self) -> RegionId {
        let new_scope = self.interner.fresh();
        // The new inner scope is outlived by the current outer scope.
        if let Some(&outer) = self.scope_stack.last() {
            self.graph.add_outlives(outer, new_scope);
        }
        self.scope_stack.push(new_scope);
        new_scope
    }

    /// Exit the current lexical scope.
    ///
    /// Pops the scope from the stack.
    pub fn exit_scope(&mut self) {
        if self.scope_stack.len() > 1 {
            self.scope_stack.pop();
        }
    }

    /// Get the current lexical scope's region.
    pub fn current_scope(&self) -> RegionId {
        *self.scope_stack.last().unwrap_or(&RegionId::STATIC)
    }

    /// Get the region for a let-bound borrow at the current scope.
    ///
    /// Let-bound borrows inherit the region of their lexical scope.
    pub fn let_borrow_region(&self) -> RegionId {
        self.current_scope()
    }

    /// Get the region for a function-parameter borrow.
    ///
    /// Parameter borrows use the call's outermost region (function entry).
    /// In Phase 4 minimum, this is the function-entry or static scope.
    pub fn param_borrow_region(&self) -> RegionId {
        self.scope_stack
            .first()
            .copied()
            .unwrap_or(RegionId::STATIC)
    }

    /// Close the region graph by computing transitive closure.
    ///
    /// Consumes the walker and returns the final RegionGraph and RegionInterner.
    pub fn close_graph(mut self) -> (RegionGraph, RegionInterner) {
        self.graph.close_transitively();
        (self.graph, self.interner)
    }
}

impl Default for RegionInferenceWalker {
    fn default() -> Self {
        Self::new()
    }
}

/// Phase-4-m5-004: Insert region_id into PositionIndex at borrow sites.
///
/// When visiting IR nodes that represent borrow expressions (Borrow, BorrowMut),
/// this walker inserts a PositionEntry with the current region_id into
/// the position index so LSP can render "region: 'r{n}" on hover.
impl IrWalker for RegionInferenceWalker {
    fn pre_visit(
        &mut self,
        _id: IrNodeId,
        node: &IrNodeData,
        _arena: &IrArena,
        ctx: &mut WalkerCtx<'_>,
    ) {
        // Phase-4-m5-004: Insert PositionEntry with region_id for borrow sites.
        // We check if this is a borrow expression (Borrow/BorrowMut) and record its region.
        if let Some(writer) = ctx.pass_state::<crate::WalkerPassState>() {
            // Determine if this node is a borrow site that should get a region.
            // In Phase 4 minimum, we conservatively mark Borrow and BorrowMut nodes.
            let region_id_opt = match node.kind {
                IrKind::Borrow | IrKind::BorrowMut => {
                    // Let-bound borrow: use current lexical scope.
                    Some(self.let_borrow_region().0)
                }
                _ => None,
            };

            if region_id_opt.is_some() {
                let entry = PositionEntry {
                    span_start: ByteOffset(node.span.byte_start()),
                    span_end: ByteOffset(node.span.byte_end()),
                    type_id: None,
                    lin_class: None,
                    effect_row_id: None,
                    cap_set_id: None,
                    region_id: region_id_opt,
                };
                writer.insert_entry(entry);
            }
        }

        // Update scope stack as we traverse.
        // Entering a Lambda opens a new scope.
        if matches!(node.kind, IrKind::Lambda) {
            self.enter_scope();
        }
    }

    fn post_visit(
        &mut self,
        _id: IrNodeId,
        node: &IrNodeData,
        _arena: &IrArena,
        _ctx: &mut WalkerCtx<'_>,
    ) {
        // Exiting a Lambda closes its scope.
        if matches!(node.kind, IrKind::Lambda) {
            self.exit_scope();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_inference_walker_starts_at_static() {
        let walker = RegionInferenceWalker::new();
        assert_eq!(walker.current_scope(), RegionId::STATIC);
    }

    #[test]
    fn region_inference_enter_scope_returns_fresh_id() {
        let mut walker = RegionInferenceWalker::new();
        let r1 = walker.enter_scope();
        let r2 = walker.enter_scope();
        assert_ne!(r1, r2);
        assert_ne!(r1, RegionId::STATIC);
        assert_ne!(r2, RegionId::STATIC);
        // r1 should be RegionId(1), r2 should be RegionId(2).
        assert_eq!(r1, RegionId(1));
        assert_eq!(r2, RegionId(2));
    }

    #[test]
    fn region_inference_let_borrow_uses_current_scope() {
        let mut walker = RegionInferenceWalker::new();
        // Initially at STATIC.
        assert_eq!(walker.let_borrow_region(), RegionId::STATIC);
        // Enter a scope.
        let scope1 = walker.enter_scope();
        assert_eq!(walker.let_borrow_region(), scope1);
        // Enter another scope.
        let scope2 = walker.enter_scope();
        assert_eq!(walker.let_borrow_region(), scope2);
        // Exit to scope1.
        walker.exit_scope();
        assert_eq!(walker.let_borrow_region(), scope1);
    }

    #[test]
    fn region_inference_param_borrow_uses_outermost() {
        let mut walker = RegionInferenceWalker::new();
        // At STATIC, param borrow is STATIC.
        assert_eq!(walker.param_borrow_region(), RegionId::STATIC);
        // Enter scope1: param borrow remains STATIC (outermost).
        let _scope1 = walker.enter_scope();
        assert_eq!(walker.param_borrow_region(), RegionId::STATIC);
        // Enter scope2: param borrow still STATIC.
        walker.enter_scope();
        assert_eq!(walker.param_borrow_region(), RegionId::STATIC);
        // Exit to scope1: param borrow still STATIC.
        walker.exit_scope();
        assert_eq!(walker.param_borrow_region(), RegionId::STATIC);
    }

    #[test]
    fn region_inference_close_graph_computes_transitive_closure() {
        let mut walker = RegionInferenceWalker::new();
        // Build a chain: STATIC > r1 > r2.
        let r1 = walker.enter_scope();
        let r2 = walker.enter_scope();
        let (graph, _interner) = walker.close_graph();
        // After closure, STATIC should outlive r2.
        assert!(graph.outlives(RegionId::STATIC, r1));
        assert!(graph.outlives(RegionId::STATIC, r2));
        // r1 should outlive r2.
        assert!(graph.outlives(r1, r2));
        // r2 should not outlive r1.
        assert!(!graph.outlives(r2, r1));
    }

    #[test]
    fn region_inference_enter_then_exit_returns_outer() {
        let mut walker = RegionInferenceWalker::new();
        let initial = walker.current_scope();
        let scope1 = walker.enter_scope();
        assert_eq!(walker.current_scope(), scope1);
        walker.exit_scope();
        assert_eq!(walker.current_scope(), initial);
    }
}
