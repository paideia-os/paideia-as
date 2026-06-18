//! Capability-set inference walker for IR trees.
//!
//! Implements an [`IrWalker`] that tracks capability sets through an IR tree,
//! running composition at App nodes and validation at Lambda boundaries.
//! Produces C1300 (required capability not held) diagnostics.
//!
//! ## Phase-2-m1 Status
//!
//! This walker runs on IR. Capability metadata for Lambda / App nodes
//! arrives via injection tables (the `lambda_declared` and `app_required`
//! HashMaps) — the walker has no way to recover capability sets from the IR
//! until the IR gains structured per-Lambda / per-App capability payloads
//! (planned for m5, modules/functors milestone). The walker's logic is
//! verified end-to-end on test fixtures that populate the injection tables.

use std::collections::HashMap;

use paideia_as_ir::{IrArena, IrKind, IrNodeData, IrNodeId, IrWalker, WalkerCtx};
use paideia_as_types::CapSet;

use crate::cap_infer::{check_capabilities, compose_caps};

/// IrWalker that tracks the required capability set and runs
/// check_capabilities at Lambda boundaries.
///
/// Phase-2-m1 minimum: the IR doesn't yet carry per-Lambda /
/// per-App capability metadata, so the walker uses injection
/// tables (analogous to EffectRowWalker's perform_ops /
/// handle_effects / call_rows tables) to associate IR nodes with
/// their declared / required cap sets. The production wiring lands
/// in m5 (modules/functors).
#[derive(Debug)]
pub struct CapWalker {
    /// Capability set required by the current expression context.
    /// Accumulates as the walker descends through nodes that
    /// require capabilities.
    current_required: CapSet,
    /// Stack of saved required sets across Lambda boundaries.
    required_stack: Vec<CapSet>,
    /// Per-Lambda declared capability set. Tests inject; m5 wires
    /// from the IR.
    pub lambda_declared: HashMap<IrNodeId, CapSet>,
    /// Per-App required capability set (the callee's declared caps).
    pub app_required: HashMap<IrNodeId, CapSet>,
}

impl CapWalker {
    /// Construct a new capability walker with an empty required set and empty side-tables.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_required: CapSet::empty(),
            required_stack: Vec::new(),
            lambda_declared: HashMap::new(),
            app_required: HashMap::new(),
        }
    }

    /// Inject the declared capability set for a Lambda node (phase-2-m1).
    pub fn inject_lambda_declared(&mut self, node_id: IrNodeId, declared: CapSet) {
        self.lambda_declared.insert(node_id, declared);
    }

    /// Inject the required capability set for an App node (phase-2-m1).
    pub fn inject_app_required(&mut self, node_id: IrNodeId, required: CapSet) {
        self.app_required.insert(node_id, required);
    }
}

impl Default for CapWalker {
    fn default() -> Self {
        Self::new()
    }
}

impl IrWalker for CapWalker {
    fn pre_visit(
        &mut self,
        _id: IrNodeId,
        node: &IrNodeData,
        _arena: &IrArena,
        _ctx: &mut WalkerCtx<'_>,
    ) {
        if node.kind == IrKind::Lambda {
            // Enter a Lambda scope: push the current required set and reset to empty.
            // This isolates the Lambda's body capability requirements from the outer scope.
            // After post_visit, the body's accumulated requirements will be checked
            // against the Lambda's declared capabilities.
            self.required_stack.push(self.current_required.clone());
            self.current_required = CapSet::empty();
        }
    }

    fn post_visit(
        &mut self,
        id: IrNodeId,
        node: &IrNodeData,
        _arena: &IrArena,
        ctx: &mut WalkerCtx<'_>,
    ) {
        match node.kind {
            IrKind::Lambda => {
                // Exiting a Lambda scope: check that the Lambda's declared capabilities
                // contain every capability required by the body.
                let body_required = self.current_required.clone();

                if let Some(declared) = self.lambda_declared.get(&id).cloned() {
                    let diags = check_capabilities(&declared, &body_required, node.span);
                    for diag in diags {
                        ctx.emit(diag);
                    }
                }

                // Restore the outer scope's required set.
                if let Some(saved_required) = self.required_stack.pop() {
                    self.current_required = saved_required;
                }
            }
            IrKind::App => {
                // At an App: compose the App's required caps into the current required set.
                // This models how the caller's requirement accumulates as it invokes callees.
                if let Some(app_required) = self.app_required.get(&id).cloned() {
                    self.current_required = compose_caps(&self.current_required, &app_required);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_diagnostics::{DiagnosticSink, FileId, SourceMap, Span, VecSink};
    use paideia_as_ir::walk;
    use paideia_as_types::CapId;

    fn span(start: u32) -> Span {
        Span::new(FileId::new(1).unwrap(), start, 1)
    }

    fn cid(n: u32) -> CapId {
        CapId::new(n).expect("cap id")
    }

    fn caps(ns: &[u32]) -> CapSet {
        CapSet::from_ids(ns.iter().map(|n| cid(*n)).collect())
    }

    #[test]
    fn walker_emits_c1300_on_missing_capability() {
        // Build IR: Module → Lambda → App
        // Inject: Lambda declared with empty caps, App requires {1}
        // Expected: C1300 (required cap not held).
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(0);

        let app_id = arena.alloc(paideia_as_ir::IrKind::App, s);
        let lambda_id = arena.alloc_with_children(paideia_as_ir::IrKind::Lambda, s, [app_id]);
        let module_id = arena.alloc_with_children(paideia_as_ir::IrKind::Module, s, [lambda_id]);

        let mut walker = CapWalker::new();
        // Lambda declared with empty caps
        walker.inject_lambda_declared(lambda_id, caps(&[]));
        // App requires capability {1}
        walker.inject_app_required(app_id, caps(&[1]));

        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, module_id, &mut ctx);

        // Should have one C1300 for the missing capability.
        assert_eq!(sink.count(), 1, "exactly one C1300 expected");
        let diag = sink.diagnostics()[0].clone();
        assert_eq!(diag.code().number(), 1300);
    }

    #[test]
    fn walker_no_c1300_when_caps_held() {
        // Build IR: Module → Lambda → App
        // Inject: Lambda declared {1}, App requires {1}
        // Expected: No C1300.
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(0);

        let app_id = arena.alloc(paideia_as_ir::IrKind::App, s);
        let lambda_id = arena.alloc_with_children(paideia_as_ir::IrKind::Lambda, s, [app_id]);
        let module_id = arena.alloc_with_children(paideia_as_ir::IrKind::Module, s, [lambda_id]);

        let mut walker = CapWalker::new();
        // Lambda declared {1}
        walker.inject_lambda_declared(lambda_id, caps(&[1]));
        // App requires {1}
        walker.inject_app_required(app_id, caps(&[1]));

        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, module_id, &mut ctx);

        // No diagnostics should be emitted.
        assert_eq!(sink.count(), 0, "no diagnostics expected");
    }

    #[test]
    fn walker_c1300_lists_each_missing_cap() {
        // Build IR: Module → Lambda → App
        // Inject: Lambda declared {1}, App requires {1, 2, 3}
        // Expected: 2× C1300 (for missing caps 2 and 3).
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(0);

        let app_id = arena.alloc(paideia_as_ir::IrKind::App, s);
        let lambda_id = arena.alloc_with_children(paideia_as_ir::IrKind::Lambda, s, [app_id]);
        let module_id = arena.alloc_with_children(paideia_as_ir::IrKind::Module, s, [lambda_id]);

        let mut walker = CapWalker::new();
        // Lambda declared {1}
        walker.inject_lambda_declared(lambda_id, caps(&[1]));
        // App requires {1, 2, 3}
        walker.inject_app_required(app_id, caps(&[1, 2, 3]));

        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, module_id, &mut ctx);

        // Should have 2 C1300s (for caps 2 and 3).
        assert_eq!(sink.count(), 2, "exactly two C1300s expected");
        for diag in sink.diagnostics() {
            assert_eq!(diag.code().number(), 1300);
        }
    }

    #[test]
    fn walker_over_declaration_no_c1300() {
        // Build IR: Module → Lambda → App
        // Inject: Lambda declared {1, 2, 3}, App requires {1}
        // Expected: No C1300 (over-declaration is fine).
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(0);

        let app_id = arena.alloc(paideia_as_ir::IrKind::App, s);
        let lambda_id = arena.alloc_with_children(paideia_as_ir::IrKind::Lambda, s, [app_id]);
        let module_id = arena.alloc_with_children(paideia_as_ir::IrKind::Module, s, [lambda_id]);

        let mut walker = CapWalker::new();
        // Lambda declared {1, 2, 3}
        walker.inject_lambda_declared(lambda_id, caps(&[1, 2, 3]));
        // App requires {1}
        walker.inject_app_required(app_id, caps(&[1]));

        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, module_id, &mut ctx);

        // No diagnostics should be emitted.
        assert_eq!(sink.count(), 0, "no diagnostics expected");
    }

    #[test]
    fn walker_cap_stack_restored_across_lambdas() {
        // Build IR: Module → Lambda1 → App1, Lambda2 → App2
        // Inject: Lambda1 declared {1}, App1 requires {1}
        //         Lambda2 declared {}, App2 requires {}
        // Expected: No C1300 for Lambda1 or Lambda2 (stack isolation).
        //           Lambda2's empty required set doesn't trigger spurious C1300.
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(0);

        let app1_id = arena.alloc(paideia_as_ir::IrKind::App, s);
        let lambda1_id = arena.alloc_with_children(paideia_as_ir::IrKind::Lambda, s, [app1_id]);

        let app2_id = arena.alloc(paideia_as_ir::IrKind::App, s);
        let lambda2_id = arena.alloc_with_children(paideia_as_ir::IrKind::Lambda, s, [app2_id]);

        let module_id =
            arena.alloc_with_children(paideia_as_ir::IrKind::Module, s, [lambda1_id, lambda2_id]);

        let mut walker = CapWalker::new();
        // Lambda1 declared {1}, App1 requires {1}
        walker.inject_lambda_declared(lambda1_id, caps(&[1]));
        walker.inject_app_required(app1_id, caps(&[1]));
        // Lambda2 declared {}, App2 requires {}
        walker.inject_lambda_declared(lambda2_id, caps(&[]));
        walker.inject_app_required(app2_id, caps(&[]));

        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, module_id, &mut ctx);

        // No diagnostics should be emitted (both lambdas are clean).
        assert_eq!(sink.count(), 0, "no diagnostics expected");
    }

    #[test]
    fn walker_empty_module_no_diagnostics() {
        // Regression test: empty Module should not emit diagnostics.
        let mut arena = paideia_as_ir::IrArena::new();
        let s = span(0);

        let module_id = arena.alloc(paideia_as_ir::IrKind::Module, s);

        let mut walker = CapWalker::new();
        let sm = SourceMap::new();
        let mut sink = VecSink::new();
        let mut ctx = WalkerCtx::new(&sm, &mut sink);

        walk(&mut walker, &arena, module_id, &mut ctx);

        // No diagnostics for an empty module.
        assert_eq!(sink.count(), 0, "no diagnostics for empty module");
    }
}
