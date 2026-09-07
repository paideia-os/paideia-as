//! Three-way `Store` dispatch shared by `emit_block_body`,
//! `emit_block_body_arm`, and `emit_action_stmt`. Chooses between
//! field-assign (module-level record field write via rip-sym),
//! var-assign (module-level `let mut` write via rip-sym), and the
//! default array/pointer store. Extracted verbatim from the pre-split
//! `emit_block_body.rs` (issue #1410).

use paideia_as_ir::{IrArena, IrKind, IrNodeId};

use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// #1115 / #1116: Three-way Store dispatch shared by `emit_block_body` and
    /// `emit_block_body_arm`. Chooses field-assign vs. var-assign vs. array/pointer store
    /// by inspecting the Store node's first child.
    ///
    /// Dispatch order:
    /// 1. FieldAccess → visit_field_assign (module-level record field write via rip-sym)
    /// 2. Var → visit_var_assign (module-level let mut write via rip-sym)
    /// 3. Default → visit_store (array/pointer store via MemSib)
    pub(crate) fn dispatch_store(&mut self, store_id: IrNodeId, arena: &IrArena) {
        let store_children = arena.children(store_id);
        let first_child_kind = store_children
            .first()
            .and_then(|&c| arena.get(c))
            .map(|n| n.kind);

        match first_child_kind {
            Some(IrKind::FieldAccess) => {
                self.visit_field_assign(store_id, arena);
            }
            Some(IrKind::Var) => {
                self.visit_var_assign(store_id, arena);
            }
            _ => {
                // Default: array index or pointer deref
                self.visit_store(store_id, arena);
            }
        }

        // Adversarial-verify of #1094 (aee6935): the mark_store_emitted() call that used
        // to live here was dead code. Every current caller of dispatch_store passes a
        // Store node whose direct parent is an `IrKind::Action` (block, match-arm body,
        // or StmtExpr wrapper) — already skipped by walk_inner's structural
        // `is_child_of_action` scan in emit_walker.rs — or is the #1116 Lambda→Store
        // direct-body pattern, which already explicitly marks itself right after calling
        // dispatch_store (emit_visit_lambda.rs). Confirmed dead by removing this call and
        // running the full workspace suite: 4584/0/212, identical to the baseline with the
        // call present — zero regressions.
    }
}
