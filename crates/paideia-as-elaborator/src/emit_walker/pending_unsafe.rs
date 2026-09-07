//! EmitWalker — pending-unsafe-body flush + U1614 diagnostic helper.
//!
//! Split from `emit_walker.rs` (paideia-as#1411). Isolates the post-walk
//! path that lowers action statements collected inside `unsafe { ... }`
//! blocks (issue #1088) plus its co-located `push_typed_diag_u1614` sink.

use paideia_as_diagnostics::{Diagnostic, DiagnosticCode};
use paideia_as_ir::{IrArena, IrKind, IrNodeId};

use super::EmitWalker;

impl EmitWalker {
    /// Phase 7 m4-003: Emit pending unsafe-block statement bodies.
    ///
    /// Issue #1088: After UnsafeWalker processes raw instructions and labels,
    /// emit any pending action statements (call expressions, etc.) through the
    /// standard IR emit pipeline. Statements not yet routable fire U1614 fallback.
    pub fn emit_pending_unsafe_bodies(
        &mut self,
        pending: Vec<u32>,
        arena: &mut IrArena,
        typer: Option<&paideia_as_types::TypeInterner>,
    ) {
        for id_u32 in pending {
            let Some(unsafe_id) = IrNodeId::new(id_u32) else { continue };

            // #1139: Look up the enclosing lambda and re-register its params.
            if let Some(&lid) = self.state.unsafe_body_to_lambda.get(&id_u32) {
                if let Some(l_node) = IrNodeId::new(lid) {
                    // Clear and re-populate local_bindings with this unsafe body's enclosing lambda's params.
                    self.state.local_bindings.clear();
                    self.state.current_function = lid;
                    self.register_nested_lambda_params(l_node, arena, 0);
                }
            }

            for (child_idx, &child) in arena.children(unsafe_id).iter().enumerate() {
                let Some(node) = arena.get(child) else { continue };
                match node.kind {
                    IrKind::RawInstruction | IrKind::Label | IrKind::Placeholder => {
                        // UnsafeWalker already emitted (RawInstruction); IrKind::Label
                        // is a reserved/dead variant kept as a defensive skip; StmtLabel
                        // actually lowers to IrKind::Placeholder, which is a no-op here.
                    }
                    IrKind::Var => {
                        // Bare identifier in unsafe block (e.g., `x;`).
                        // No side effects; skip.
                    }
                    IrKind::Literal => {
                        // Literal in unsafe block (e.g., `42;`).
                        // No side effects; skip.
                    }
                    IrKind::Action => {
                        // Statement-position expression: delegate to emit_action_stmt.
                        //
                        // Issue #1270: UnsafeWalker's raw-instruction pass reserves an
                        // emission_order base for each StmtExpr at the exact block
                        // position it's encountered (keyed by (unsafe_id, stmt_index),
                        // which lines up 1:1 with this child-index enumeration since
                        // every AST statement in the block lowers to exactly one IR
                        // child in the same order). If a reservation exists, resume
                        // the shared counter from it so this call's real instructions
                        // land at their true source position — interleaved correctly
                        // with the surrounding raw asm — instead of wherever
                        // next_emission_order has advanced to after every unsafe block
                        // in the whole file has already been lowered.
                        let reserved_base = self
                            .state
                            .unsafe_stmt_expr_order_base
                            .get(&(id_u32, child_idx))
                            .copied();
                        match reserved_base {
                            Some(reserved_base) => {
                                let resume_from = self.state.next_emission_order;
                                self.state.next_emission_order = reserved_base;
                                self.emit_action_stmt(child, arena, typer);
                                self.state.next_emission_order = resume_from;
                            }
                            None => {
                                self.emit_action_stmt(child, arena, typer);
                            }
                        }
                    }
                    _ => {
                        // Unroutable statement kind (Let, Loop, While, Return, etc.).
                        self.push_typed_diag_u1614(
                            node.span,
                            format!("unroutable statement kind in unsafe block: {:?}", node.kind),
                        );
                    }
                }
            }
        }

        // #1139: DROP the old snapshot-and-restore pattern (was at 87f2076).
        // The prior fix stored `saved` and restored it here, but this was a stop-gap that
        // defeated the real fix for consumers (resolve_var_operands in cmd_build.rs).
        // With per_lambda_bindings + instr_to_lambda, resolve_var_operands now looks up
        // each instruction's enclosing lambda and uses that lambda's binding snapshot,
        // so there's no need to restore the flat state. The snapshot is only needed for
        // emit_action_stmt's direct emissions (handled above via re-register at lines 739-740).

        // #1146 follow-up: instructions emitted above (via emit_action_stmt →
        // dispatch_store/emit_call_stmt → emit_inst) land in
        // `self.state.instructions`, not the arena. `walk()` already did its
        // one-time transfer before this function ever runs, so without this
        // call every such instruction — e.g. the store for `(*p).field = v;`
        // inside an unsafe block — is silently dropped: never reaches
        // `resolve_var_operands` or the encoder, and .text simply omits it
        // with no diagnostic.
        self.sync_state_instructions_to_arena(arena);
    }

    /// Helper to push U1614 diagnostic with span (internal use).
    pub(crate) fn push_typed_diag_u1614(
        &mut self,
        span: paideia_as_diagnostics::Span,
        message: impl Into<String>,
    ) {
        let code = DiagnosticCode::new(
            paideia_as_diagnostics::Category::U,
            paideia_as_diagnostics::Severity::Error,
            1614,
        );
        if let Ok(code) = code {
            let diag = Diagnostic::error(code)
                .message(message)
                .with_span(span)
                .finish();
            self.structured_diagnostics.push(diag);
        }
    }
}
