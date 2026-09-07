//! `emit_action_stmt`: statement-position action (StmtExpr) dispatch.
//! Routes App / FieldAccess / Var / Literal / Store children of an
//! Action node to the appropriate emit path. Result is always discarded
//! (no return-value placement).
//!
//! Extracted verbatim from the pre-split `emit_block_body.rs`
//! (issue #1410). No behavior change.

use paideia_as_ir::{IrArena, IrKind, IrNodeId};

use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// Phase 7 m4-003: Emit statement-position action (StmtExpr).
    ///
    /// Issue #1088: Route call expressions inside unsafe blocks through the emit pipeline.
    /// Handles Action nodes whose children are expression kinds (App, FieldAccess, Var, Literal).
    /// Result is discarded (no return-value placement).
    pub(crate) fn emit_action_stmt(
        &mut self,
        action_id: IrNodeId,
        arena: &IrArena,
        _typer: Option<&paideia_as_types::TypeInterner>,
    ) {
        let action_children = arena.children(action_id);
        if let Some(&child_id) = action_children.first() {
            if let Some(child_node) = arena.get(child_id) {
                match child_node.kind {
                    IrKind::App => {
                        // Call expression in statement position.
                        // Extract callee (first child of App) and arguments.
                        let app_children = arena.children(child_id);
                        if app_children.len() > 0 {
                            let callee_id = app_children[0];
                            if let Some(callee_node) = arena.get(callee_id) {
                                if callee_node.kind == IrKind::Var {
                                    if let Some(target_name) = arena.binding_names().get(callee_id) {
                                        let lambda_id = IrNodeId::new(self.state.current_function)
                                            .expect("current_function set by walker");
                                        self.emit_call_stmt(
                                            lambda_id,
                                            target_name.to_string(),
                                            &app_children[1..],
                                            arena,
                                        );
                                        return;
                                    }
                                }
                            }
                        }
                        // Fall through: emit U1614 if callee extraction fails
                        let span = arena
                            .get(child_id)
                            .map(|n| n.span)
                            .unwrap_or_else(|| paideia_as_diagnostics::Span::new(
                                paideia_as_diagnostics::FileId::new(1).unwrap(),
                                0,
                                1,
                            ));
                        self.push_typed_diag_u1614(
                            span,
                            "unroutable call expression in statement position (internal compiler error)".to_string(),
                        );
                    }
                    IrKind::FieldAccess => {
                        // Field access in statement position (e.g., `obj.field;`).
                        // Side effect depends on the target field; for now, skip silently.
                        if cfg!(debug_assertions) {
                            eprintln!(
                                "[emit_action_stmt] FieldAccess in statement position — skipped"
                            );
                        }
                    }
                    IrKind::Var => {
                        // Bare identifier in statement position (e.g., `x;`).
                        // No side effects; skip silently.
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_action_stmt] Var in statement position — skipped");
                        }
                    }
                    IrKind::Literal => {
                        // Literal in statement position (e.g., `42;`).
                        // No side effects; skip silently.
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_action_stmt] Literal in statement position — skipped");
                        }
                    }
                    IrKind::Store => {
                        // #1094: StmtExpr wrapping a Pattern 1..5 assignment. Re-use the same
                        // 3-way store dispatch used when a Store appears directly as a block child.
                        self.dispatch_store(child_id, arena);
                    }
                    _ => {
                        // Unroutable statement kind (Loop/While/Let/Return/etc.).
                        self.push_typed_diag_u1614(
                            child_node.span,
                            format!("unroutable statement kind in Action: {:?}", child_node.kind),
                        );
                    }
                }
            }
        }
    }
}
