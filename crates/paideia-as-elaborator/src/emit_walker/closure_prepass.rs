//! EmitWalker — closure-related pre-passes.
//!
//! Three pre-passes that run at the top of `walk_inner` before any
//! construct is lowered:
//!   * `register_closure_body_symbols` (#1233 phase B) — mangles + inserts
//!     ELF symbol table entries for every ClosureCons body Lambda.
//!   * `precompute_caller_frame` (#1233 phase B) — assigns (fat_ptr,
//!     env_record) slots for each ClosureCons descendant of a Lambda,
//!     under 16-byte SysV alignment.
//!   * `check_nested_closure_invocations` (#1239) — T0575 diagnostic for
//!     closures invoking inner closure-typed bindings (would clobber R14).
//!
//! Split from `emit_walker.rs` (paideia-as#1411).

use paideia_as_diagnostics::Diagnostic;
use paideia_as_ir::{IrArena, IrKind, IrNodeId, Symbol, SymbolKind};

use super::EmitWalker;

impl EmitWalker {
    /// #1233 Phase B: Register closure body Lambda symbols in the arena's symbol table.
    ///
    /// Scans all ClosureCons nodes in the arena and extracts their body Lambda children.
    /// Each closure body Lambda is registered with a mangled symbol name:
    /// `closure_<parent_binding_name>_<lambda_id>`.
    ///
    /// Closure body symbols are registered as Function kind with Local visibility.
    /// The symbol's ir_node field points to the Lambda node ID, enabling offset
    /// lookup through function_offsets in the downstream ELF builder.
    pub(super) fn register_closure_body_symbols(&mut self, arena: &mut IrArena) {
        // Scan all nodes looking for ClosureCons instances
        for i in 1..=arena.len() as u32 {
            if let Some(cc_id) = IrNodeId::new(i) {
                if let Some(cc_node) = arena.get(cc_id) {
                    if cc_node.kind == IrKind::ClosureCons {
                        // Extract the closure body Lambda (first child of ClosureCons)
                        let cc_children = arena.children(cc_id);
        if let Some(&lambda_id) = cc_children.first() {
                            if let Some(lambda_node) = arena.get(lambda_id) {
                                if lambda_node.kind == IrKind::Lambda {
                                    // Issue #994: read the mangled name from ClosureMetaTable — the
                                    // single source of truth populated by the elaborator's
                                    // closure-dispatch pass (closure_dispatch::convert_closure_lets),
                                    // which runs before EmitWalker::walk. `emit_closure_cons` also
                                    // reads `mangled_name` from this same table for its `lea [rip +
                                    // name]` relocation target, so defining the ELF symbol under any
                                    // independently-recomputed name here would silently desync from
                                    // that relocation and produce an unresolved reference at link
                                    // time. Fall back to a stable id-derived name only if closure_meta
                                    // was never populated for this lambda (should not happen for any
                                    // ClosureCons produced by the dispatch pass, but keeps this pass
                                    // total rather than silently skipping symbol registration).
                                    let mangled_name = arena
                                        .closure_meta()
                                        .get(lambda_id)
                                        .map(|meta| meta.mangled_name.clone())
                                        .unwrap_or_else(|| format!("closure_anon_{}", lambda_id.get()));

                                    // Create and insert symbol for the closure body Lambda
                                    let sym = Symbol::new_with_visibility(
                                        mangled_name,
                                        SymbolKind::Function,
                                        lambda_id,
                                        paideia_as_ir::Visibility::Local,
                                    );
                                    arena.symbols_mut().insert(sym);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// #1233 Phase B: Precompute frame layout for a caller Lambda containing ClosureCons nodes.
    ///
    /// Recursively scans the caller's body for all ClosureCons descendants and assigns
    /// non-overlapping stack slots (fat_ptr + env_record) for each. Populates
    /// closure_frame_meta_mut() with the computed layout.
    ///
    /// Frame layout is 16-byte aligned per SysV calling convention requirements.
    /// Slot assignments are computed from rsp (not rbp; paideia-as uses rsp-relative addressing).
    pub(super) fn precompute_caller_frame(&mut self, caller_lambda_id: IrNodeId, arena: &mut IrArena) {
        // Scan caller body for ClosureCons descendants
        let caller_children = arena.children(caller_lambda_id);
        if caller_children.is_empty() {
            return; // No body, no closures
        }

        let body_id = caller_children[0];
        let mut frame_layout = paideia_as_ir::FrameLayout::new();
        let mut current_offset: u64 = 0;

        // Recursively collect all ClosureCons nodes in the caller body
        fn collect_closure_cons_nodes(
            id: IrNodeId,
            arena: &IrArena,
            nodes: &mut Vec<IrNodeId>,
        ) {
            if let Some(n) = arena.get(id) {
                match n.kind {
                    IrKind::ClosureCons => {
                        nodes.push(id);
                    }
                    IrKind::Lambda | IrKind::Unsafe => {
                        // Stop at Lambda/Unsafe boundaries
                        return;
                    }
                    _ => {}
                }
                for &child_id in arena.children(id) {
                    collect_closure_cons_nodes(child_id, arena, nodes);
                }
            }
        }

        let mut closure_cons_nodes = Vec::new();
        collect_closure_cons_nodes(body_id, arena, &mut closure_cons_nodes);

        // Assign slots for each ClosureCons node
        for cc_id in closure_cons_nodes {
            if let Some(cc_node) = arena.get(cc_id) {
                if cc_node.kind == IrKind::ClosureCons {
                    // Get the closure meta to determine env size
                    let cc_children = arena.children(cc_id);
                    let env_size = if let Some(&lambda_id) = cc_children.first() {
                        arena
                            .closure_meta()
                            .get(lambda_id)
                            .map(|meta| meta.env_size)
                            .unwrap_or(0)
                    } else {
                        0
                    };

                    // Fat pair is always 16 bytes (env_ptr:8 + code_ptr:8)
                    let fat_size = 16u64;

                    // Assign slots: fat pair at current_offset, env record follows
                    let fat_offset = current_offset;
                    let env_offset = current_offset + fat_size;

                    frame_layout.assign_slot(cc_id, fat_offset, env_offset);

                    // Advance for next closure
                    current_offset = env_offset + env_size;
                }
            }
        }

        // Round total size up to 16-byte alignment (SysV requirement)
        if current_offset > 0 {
            frame_layout.total_size = ((current_offset + 15) / 16) * 16;
            arena.closure_frame_meta_mut().insert(caller_lambda_id, frame_layout);
        }
    }

    /// #1239: Detect nested closure invocations (T0575).
    ///
    /// For each Lambda L that is a closure body (arena.closure_meta().get(L).is_some()),
    /// walks L's body to find:
    /// 1. Let bindings whose RHS is ClosureCons (first walk, collects binding names)
    /// 2. App nodes whose callee is a Var/Placeholder matching those binding names (second walk, fires T0575)
    ///
    /// Stops at nested Lambda/Unsafe boundaries. Records rejected lambdas in t0575_rejects.
    pub(super) fn check_nested_closure_invocations(&mut self, arena: &IrArena) {
        use crate::emit_visit_lambda::t0575_code;

        // Helper: recursively collect Lets whose RHS is ClosureCons, stopping at Lambda/Unsafe.
        fn collect_inner_closure_names(
            id: IrNodeId,
            arena: &IrArena,
            names: &mut std::collections::HashSet<String>,
        ) {
            if let Some(n) = arena.get(id) {
                match n.kind {
                    IrKind::Let => {
                        let children = arena.children(id);
                        // stmt-form Let: [name_var, value, ty?]
                        let rhs_idx = if children.len() > 1 { 1 } else { 0 };
                        if let Some(&rhs_id) = children.get(rhs_idx) {
                            if let Some(rhs_node) = arena.get(rhs_id) {
                                if rhs_node.kind == IrKind::ClosureCons {
                                    if let Some(binding_name) = arena.binding_names().get(id) {
                                        names.insert(binding_name.to_string());
                                    }
                                }
                            }
                        }
                    }
                    IrKind::Lambda | IrKind::Unsafe => {
                        // Stop at boundaries
                        return;
                    }
                    _ => {}
                }
                for &child_id in arena.children(id) {
                    collect_inner_closure_names(child_id, arena, names);
                }
            }
        }

        // Helper: recursively find App nodes whose callee is a Var/Placeholder in names.
        fn find_nested_invocations(
            id: IrNodeId,
            arena: &IrArena,
            inner_closure_names: &std::collections::HashSet<String>,
            walker: &mut EmitWalker,
        ) -> bool {
            if let Some(n) = arena.get(id) {
                match n.kind {
                    IrKind::App => {
                        let children = arena.children(id);
                        if let Some(&callee_id) = children.first() {
                            if let Some(callee_node) = arena.get(callee_id) {
                                if matches!(callee_node.kind, IrKind::Var | IrKind::Placeholder) {
                                    if let Some(callee_name) = arena.binding_names().get(callee_id) {
                                        if inner_closure_names.contains(callee_name) {
                                            // Fire T0575 on this App node
                                            let diag = Diagnostic::error(t0575_code())
                                                .message("nested closure invocation not yet supported")
                                                .with_span(n.span.clone())
                                                .finish();
                                            walker.structured_diagnostics.push(diag);
                                            return true; // Found a violation
                                        }
                                    }
                                }
                            }
                        }
                    }
                    IrKind::Lambda | IrKind::Unsafe => {
                        // Stop at boundaries
                        return false;
                    }
                    _ => {}
                }
                for &child_id in arena.children(id) {
                    if find_nested_invocations(child_id, arena, inner_closure_names, walker) {
                        return true;
                    }
                }
            }
            false
        }

        // Main loop: check each closure body lambda
        for i in 1..=arena.len() as u32 {
            if let Some(lambda_id) = IrNodeId::new(i) {
                if let Some(lambda_node) = arena.get(lambda_id) {
                    if lambda_node.kind == IrKind::Lambda && arena.closure_meta().get(lambda_id).is_some() {
                        let lambda_children = arena.children(lambda_id);
                        if let Some(&body_id) = lambda_children.first() {
                            // First pass: collect closure binding names
                            let mut inner_closure_names = std::collections::HashSet::new();
                            collect_inner_closure_names(body_id, arena, &mut inner_closure_names);

                            // Second pass: check for invocations
                            if find_nested_invocations(body_id, arena, &inner_closure_names, self) {
                                self.state.t0575_rejects.insert(lambda_id.get());
                            }
                        }
                    }
                }
            }
        }
    }
}
