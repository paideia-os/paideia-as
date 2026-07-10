//! EmitWalker — Phase 5 m1-001 entry to the build-emit pipeline.
//!
//! Walks the IR; per-construct lowering (m1-002 Let-literal, m1-003 Lambda,
//! m1-004 Unsafe) lands as siblings inside this module. The walker
//! populates an InstructionSideTable + tracks per-function offsets.

use paideia_as_diagnostics::{Diagnostic, DiagnosticCode};
use paideia_as_ir::instruction::{InstrMode, Instruction};
#[cfg(test)]
use paideia_as_ir::instruction::{Cond, IntWidth, Mnemonic, Operand};
#[cfg(test)]
use paideia_as_ir::record_layout::{FieldLayout, RecordLayout, RecordTypeId};
#[cfg(test)]
use paideia_as_ir::{EnumLayout, EnumTypeId, abi};
use paideia_as_ir::{DataSideTable, IrArena, IrKind, IrNodeId, Symbol, SymbolKind};

pub use crate::cast_shape::{CastPlan, CastShape, cast_plan};
pub use crate::emit_pass_state::{EmitPassState, LoopContext};

/// EmitWalker — drives IR traversal and instruction emission.
///
/// Skeleton implementation for Phase 5 m1-001. Per-construct lowering
/// hooks (visit_let, visit_lambda, visit_unsafe) land in m1-002..004
/// as siblings of this walker.
///
/// Phase 7 m1-008 (PA7-008): Tracks loop context stack for break validation.
pub struct EmitWalker {
    pub(crate) state: EmitPassState,
    /// Legacy free-form diagnostic buffer. Each entry is a `format!` string
    /// with a `T####:` prefix. Retirement into `structured_diagnostics` is
    /// tracked as a follow-up in `.plans/refactor-2026-07-07.md`.
    pub(crate) diagnostics: Vec<String>,
    /// Canonical typed diagnostic buffer introduced in the v0.17 refactor
    /// (Step 3, 2026-07-07). All NEW EmitWalker diagnostics must be pushed
    /// via `push_typed_diag`, which routes here. Drained by cmd_build.rs
    /// via `take_typed_diagnostics()` into the shared `DiagnosticSink`,
    /// making silent-fire-then-discard impossible for new push sites.
    pub(crate) structured_diagnostics: Vec<Diagnostic>,
    /// Stack of (loop_kind, exit_label) for nested loops/while.
    /// Push on loop/while entry, pop on exit. Used to validate break statements.
    pub(crate) loop_contexts: Vec<(LoopContext, String)>,
}

impl EmitWalker {
    /// Create a new, empty EmitWalker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: EmitPassState::default(),
            diagnostics: Vec::new(),
            structured_diagnostics: Vec::new(),
            loop_contexts: Vec::new(),
        }
    }

    /// Access the emission state (read-only).
    #[must_use]
    pub fn state(&self) -> &EmitPassState {
        &self.state
    }

    /// Access the emission state (mutable).
    #[must_use]
    pub fn state_mut(&mut self) -> &mut EmitPassState {
        &mut self.state
    }

    /// Access the accumulated legacy free-form diagnostics.
    ///
    /// New code should use `push_typed_diag` / `take_typed_diagnostics`
    /// instead — see the v0.17 refactor plan (2026-07-07) for the
    /// migration path.
    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Push a canonical typed diagnostic (Step 3, v0.17 refactor).
    ///
    /// The `code` names a T####/S####/etc. entry in the diagnostic
    /// catalog. `message` is the human-readable body; no `T####:` prefix
    /// is required because the code already carries the identity.
    ///
    /// The diagnostic accumulates in `structured_diagnostics` and is
    /// drained by `cmd_build::run` into the shared `DiagnosticSink` right
    /// after `emit_walker.walk(...)`. Silent-fire-then-discard cannot
    /// happen here — the drain wiring is a static compile-time contract.
    pub fn push_typed_diag(&mut self, code: DiagnosticCode, message: impl Into<String>) {
        let diag = Diagnostic::error(code).message(message).finish();
        self.structured_diagnostics.push(diag);
    }

    /// Drain and return the typed diagnostics accumulated during the walk.
    ///
    /// Called once by `cmd_build::run` after the emit walk completes.
    #[must_use]
    pub fn take_typed_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.structured_diagnostics)
    }

    /// Phase 15 m2-002a: Set the root module's instruction mode.
    /// This initializes the mode_stack for instruction emission.
    /// Must be called before walk() or walk_with_typer().
    pub fn set_root_mode(&mut self, mode: InstrMode) {
        self.state.mode_stack.clear();
        self.state.mode_stack.push(mode);
    }

    /// Phase 7 m1-008: Check if we are currently in a loop body.
    /// Returns Some((loop_kind, exit_label)) if in loop, None if outside.
    #[must_use]
    pub fn current_loop_context(&self) -> Option<(LoopContext, &str)> {
        self.loop_contexts
            .last()
            .map(|(ctx, label)| (*ctx, label.as_str()))
    }

    /// Phase 7 m1-008: Pop loop context on loop/while exit.
    pub fn pop_loop_context(&mut self) {
        let _ = self.loop_contexts.pop();
    }

    /// Phase 15 m2-002: Enter a new instruction mode scope.
    /// Will be used in m2-002b for scope-aware mode propagation.
    #[allow(dead_code)]
    fn enter_mode_scope(&mut self, mode: InstrMode) {
        self.state.mode_stack.push(mode);
    }

    /// Phase 15 m2-002: Exit the current instruction mode scope.
    /// Will be used in m2-002b for scope-aware mode propagation.
    #[allow(dead_code)]
    fn exit_mode_scope(&mut self) {
        self.state.mode_stack.pop();
    }

    /// Insert an `Instruction` into the side-table and advance
    /// `estimated_offset` by exactly the number of bytes the encoder
    /// will emit for it.
    ///
    /// This is the single canonical way to emit an instruction from
    /// `EmitWalker`. Retires ~65 scattered `state.instructions.insert(...);
    /// state.estimated_offset += <literal>;` pairs whose size literals had
    /// drifted from encoder reality on multiple occasions (#985, #986).
    ///
    /// The size is computed by calling the real encoder into a throwaway
    /// buffer via `paideia_as_encoder::estimated_bytes`. If the encoder
    /// cannot handle the instruction, size is 0 — callers must ensure
    /// their instructions actually encode.
    pub(crate) fn emit_inst(&mut self, node_id: IrNodeId, mut inst: Instruction) {
        let bytes = paideia_as_encoder::estimated_bytes(&inst);
        inst.emission_order = self.state.next_emission_order;
        self.state.next_emission_order += 1;
        self.state.instructions.insert(node_id, inst);
        self.state.estimated_offset += bytes;
    }

    /// Phase 15 m2-002: Get the current instruction mode (Mode64 if stack is empty).
    /// Will be used in m2-002b for scope-aware mode propagation.
    pub(crate) fn current_mode(&self) -> InstrMode {
        self.state
            .mode_stack
            .last()
            .copied()
            .unwrap_or(InstrMode::Mode64)
    }

    /// Get the set of Lambda IR node IDs that emitted bytecode.
    #[must_use]
    pub fn emitted_lambdas(&self) -> &std::collections::HashSet<u32> {
        &self.state.emitted_lambdas
    }

    /// Record a lambda's entry point instruction and mark it as emitted.
    ///
    /// Called at the START of each emit_*_lambda function to record BOTH:
    /// 1. The estimated byte offset (for st_value computation, preserves definition order)
    /// 2. The first instruction's IrNodeId (for future post-encoding offset projection)
    pub fn record_lambda_entry(&mut self, lambda_id: IrNodeId, first_instr_id: IrNodeId) {
        // Record the estimated offset for backward compatibility and correct ordering
        self.state
            .function_offsets
            .entry(lambda_id.get())
            .or_insert(self.state.estimated_offset);

        // Also record the first instruction's IR node ID for offset_map projection
        self.state
            .lambda_first_instr
            .entry(lambda_id.get())
            .or_insert(first_instr_id);

        self.state.mark_lambda_emitted(lambda_id.get());
    }

    /// Drive the walker over an IR arena.
    ///
    /// m1-002: processes Let → Literal bindings, emitting Mov instructions.
    /// m1-003: processes Lambda bodies, emitting Mov/Lea/Ret for simple cases.
    /// m1-004: records IrKind::Unsafe nodes for later processing by UnsafeWalker (m3).
    /// m4-003: populates DataSideTable for module-level Let-Literal bindings.
    /// m5-001: populates SymbolTable for module-level Let bindings.
    /// m3-003: processes Let → FieldAccess bindings, assigning scratch registers in sequence.
    pub fn walk(&mut self, arena: &mut IrArena) {
        self.walk_inner(arena, None);
    }

    /// Drive the walker with a type interner available for width threading.
    ///
    /// Phase 7 m4-003 (PA7C-m4-003): identical to [`walk`](Self::walk) but the
    /// supplied `typer` lets typed integer-literal `let` bindings emit the
    /// narrower `MovSized` form (e.g. `let x : u32 = 42` → 5-byte `B8 imm32`).
    /// Bindings without a recorded type, or non-integer types, fall back to the
    /// generic 64-bit `Mov` path, so behaviour is unchanged for untyped IR.
    pub fn walk_with_typer(&mut self, arena: &mut IrArena, typer: &paideia_as_types::TypeInterner) {
        self.walk_inner(arena, Some(typer));
    }

    fn walk_inner(&mut self, arena: &mut IrArena, typer: Option<&paideia_as_types::TypeInterner>) {
        // Phase 15 m2-002a: The mode_stack is initialized via set_root_mode() before walk() is called.
        // If set_root_mode() was not called, default to Mode64.
        if self.state.mode_stack.is_empty() {
            self.state.mode_stack.push(InstrMode::Mode64);
        }

        // Fix B (#1085): Pre-pass to prevent Match double-emission.
        // For every Lambda whose direct body child is a Match, mark the Match as already emitted.
        // This ensures the Match is not visited twice (once in trailing position, once in Discard context).
        for i in 1..=arena.len() as u32 {
            if let Some(lambda_id) = IrNodeId::new(i) {
                if let Some(lambda_node) = arena.get(lambda_id) {
                    if lambda_node.kind == IrKind::Lambda {
                        let children = arena.children(lambda_id);
                        // Lambda has one child: the body expression
                        if let Some(&body_id) = children.first() {
                            if let Some(body_node) = arena.get(body_id) {
                                if body_node.kind == IrKind::Match {
                                    self.state.mark_match_emitted(body_id.get());
                                }
                            }
                        }
                    }
                }
            }
        }

        // #1086: Second pre-pass marks nodes owned by other lowering paths so
        // scope-limited visitors (visit_record_cons, visit_field_access) don't
        // fire T0518/T0516 false positives on nodes they don't own.
        // #1131: Also marks Let nodes handled by populate_data_table so
        // visit_let_literal skips emitting spurious Mov instructions.
        // #1116: Also marks Lambda→Store bodies as emitted so top-level Store dispatch
        // skips double-emission when visit_lambda's Store arm handles them.
        for i in 1..=arena.len() as u32 {
            if let Some(node_id) = IrNodeId::new(i) {
                if let Some(node) = arena.get(node_id) {
                    match node.kind {
                        IrKind::Let => {
                            // #1131: Check if this Let node is in the data table
                            if arena.data().get(node_id).is_some() {
                                self.state.mark_data_let_handled(node_id.get());
                            }

                            let children = arena.children(node_id);
                            if let Some(&rhs_id) = children.get(0) {
                                if let Some(rhs_node) = arena.get(rhs_id) {
                                    match rhs_node.kind {
                                        // Let → RecordCons: owned by data_encoder::encode_record_cons
                                        IrKind::RecordCons => {
                                            self.state.mark_record_cons_handled(rhs_id.get());
                                        }
                                        // Let → FieldAccess: owned by visit_let_field_access
                                        IrKind::FieldAccess => {
                                            self.state.mark_field_access_handled(rhs_id.get());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        IrKind::Lambda => {
                            let children = arena.children(node_id);
                            if let Some(&body_id) = children.first() {
                                if let Some(body_node) = arena.get(body_id) {
                                    match body_node.kind {
                                        // Lambda → FieldAccess when receiver is Var:
                                        // owned by emit_field_access_lambda (RIP-relative for module symbols)
                                        IrKind::FieldAccess => {
                                            let receiver_id = arena.children(body_id).first().copied();
                                            if let Some(rid) = receiver_id {
                                                if let Some(rn) = arena.get(rid) {
                                                    if rn.kind == IrKind::Var {
                                                        self.state.mark_field_access_handled(body_id.get());
                                                    }
                                                }
                                            }
                                        }
                                        // Lambda → App → FieldAccess (Var receiver): callee is a
                                        // module-symbol reference, owned by visit_lambda's App arm
                                        IrKind::App => {
                                            let app_children = arena.children(body_id);
                                            if let Some(&callee_id) = app_children.first() {
                                                if let Some(cn) = arena.get(callee_id) {
                                                    if cn.kind == IrKind::FieldAccess {
                                                        let recv_id = arena.children(callee_id).first().copied();
                                                        if let Some(rid) = recv_id {
                                                            if let Some(rn) = arena.get(rid) {
                                                                if rn.kind == IrKind::Var {
                                                                    self.state.mark_field_access_handled(callee_id.get());
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        // #1116: Lambda → Store with Var LHS (Pattern 5)
                                        // Owned by visit_lambda's Store arm, mark as emitted
                                        IrKind::Store => {
                                            let store_children = arena.children(body_id);
                                            if let Some(&first_child) = store_children.first() {
                                                if let Some(first_node) = arena.get(first_child) {
                                                    if first_node.kind == IrKind::Var {
                                                        self.state.mark_store_emitted(body_id.get());
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Iterate over all nodes, looking for Let, Lambda, and Unsafe nodes.
        for i in 1..=arena.len() as u32 {
            if let Some(node_id) = IrNodeId::new(i) {
                if let Some(node) = arena.get(node_id) {
                    let node_kind = node.kind;
                    match node_kind {
                        IrKind::Let => {
                            // Get the single child (the RHS expression).
                            let children = arena.children(node_id);
                            let rhs_id = if let Some(&rhs) = children.first() {
                                Some(rhs)
                            } else {
                                None
                            };

                            if let Some(rhs_id) = rhs_id {
                                let rhs_kind = arena
                                    .get(rhs_id)
                                    .map(|n| n.kind)
                                    .unwrap_or(IrKind::Placeholder);
                                let has_literal_value =
                                    arena.literal_values().get(rhs_id).is_some();
                                let literal_value = arena.literal_values().get(rhs_id);

                                // Determine if RHS is a Lambda (Function) or something else (Object).
                                let kind = if rhs_kind == IrKind::Lambda {
                                    SymbolKind::Function
                                } else {
                                    SymbolKind::Object
                                };

                                // Extract binding name from binding_names side-table.
                                // Fall back to "_let_<nodeid>" if not found.
                                let binding_name = arena
                                    .binding_names()
                                    .get(node_id)
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| format!("_let_{}", node_id.get()));

                                // Create and insert symbol.
                                // For function symbols, use the lambda's IR node ID so offset lookup works.
                                // For object symbols, use the let's IR node ID.
                                let symbol_ir_node = if rhs_kind == IrKind::Lambda {
                                    rhs_id
                                } else {
                                    node_id
                                };

                                // Check if this let is explicitly marked as public.
                                // PA904: propagate pub flag to symbol visibility.
                                // Belt-and-suspenders: auto-global rule (_start, long_mode_entry) still applies.
                                // Read calling convention annotation from let_meta (issue #1006)
                                let abi = arena.let_meta()
                                    .get(node_id)
                                    .and_then(|meta| meta.abi);

                                let mut sym = Symbol::new_with_abi(
                                    binding_name,
                                    kind,
                                    symbol_ir_node,
                                    abi,
                                );
                                // Override visibility if marked public
                                if arena.is_public_let(node_id) {
                                    sym.visibility = paideia_as_ir::Visibility::Global;
                                }
                                arena.symbols_mut().insert(sym);

                                // PA19-r19-006: Record the ABI for Lambda bindings in the emit state.
                                // This enables lambda emitters to select the correct register pool.
                                if rhs_kind == IrKind::Lambda {
                                    if let Some(cc) = abi {
                                        self.state.insert_lambda_abi(rhs_id.get(), cc);
                                    }
                                }

                                // Handle Literal RHS: emit instructions for m1-002.
                                // #1131: Gate on whether this Let is already handled by populate_data_table.
                                // If the Let is in the data table, skip Mov emission to prevent spurious
                                // .text emission before function bodies.
                                if rhs_kind == IrKind::Literal && has_literal_value {
                                    if !self.state.was_data_let_handled(node_id.get()) {
                                        if let Some(value) = literal_value {
                                            // Phase 7 m4-003: width-thread typed integer literals.
                                            // Resolve the binding's declared type (if recorded) to a
                                            // bit-width and map it to an IntWidth. Untyped bindings, a
                                            // missing typer, or non-integer / unsupported widths yield
                                            // None, preserving the generic 64-bit Mov path.
                                            let width = typer.and_then(|typer| {
                                                Self::resolve_let_width(arena, node_id, typer)
                                            });
                                            self.visit_let_literal(node_id, value, width);
                                        }
                                    }
                                }

                                // Phase 6 m3-003: Handle Let with FieldAccess RHS.
                                if rhs_kind == IrKind::FieldAccess {
                                    self.visit_let_field_access(node_id, rhs_id, arena);
                                }
                            }
                        }
                        IrKind::Lambda => {
                            // Phase 6 m3-003: Reset scratch_assignment at function entry.
                            self.state.clear_scratch();
                            self.state.current_function = node_id.get();

                            // Lambda lowering: emit Mov/Lea/Ret for simple cases.
                            // PA8-m3-001: thread the typer so in-block let-literal
                            // bindings can width-route to MovSized.
                            self.visit_lambda(node_id, arena, typer);
                        }
                        IrKind::Unsafe => {
                            // Record unsafe node for later processing by UnsafeWalker (m3).
                            // We do not inspect block contents here.
                            let pending_idx = self.state.pending_unsafe_count();
                            self.state.push_pending_unsafe(node_id.get());

                            // PA8-m1-002b: If this Unsafe body was referenced by a lambda,
                            // record the pending index for that lambda.
                            if let Some(lambda_id) =
                                self.state.unsafe_body_lambda(node_id.get())
                            {
                                self.state
                                    .insert_unsafe_lambda_pending_idx(lambda_id, pending_idx);
                            }
                        }
                        IrKind::FieldAccess => {
                            // Phase 6 m3-002: emit field access lowering for (*p).field shape.
                            // #1086: skip if another lowering path owns this node
                            if !self.state.was_field_access_handled(node_id.get()) {
                                self.visit_field_access(node_id, arena);
                            }
                        }
                        IrKind::Store => {
                            // #1116: Skip if this Store was already handled by visit_lambda's Store arm.
                            // This prevents double-emission for Lambda → Store patterns.
                            if self.state.was_store_emitted(node_id.get()) {
                                continue;
                            }

                            // #1094: Skip if this Store is a child of an Action (block statement).
                            // Such Stores are handled by emit_block_body → emit_action_stmt → dispatch_store,
                            // not by emit_walker's direct processing. This prevents processing before
                            // lambda parameters have been registered in local_bindings.
                            let mut is_child_of_action = false;
                            for i in 1..=arena.len() as u32 {
                                if let Some(check_id) = IrNodeId::new(i) {
                                    if let Some(check_node) = arena.get(check_id) {
                                        if check_node.kind == IrKind::Action {
                                            let action_children = arena.children(check_id);
                                            if action_children.contains(&node_id) {
                                                is_child_of_action = true;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            if is_child_of_action {
                                continue;
                            }

                            // Check if this is a field assignment (*p).f = value (first child is FieldAccess),
                            // var assignment counter = v (first child is Var), or a regular deref/array store.
                            let children = arena.children(node_id);
                            let first_child_kind = children.first()
                                .and_then(|&c| arena.get(c))
                                .map(|n| n.kind);

                            match first_child_kind {
                                Some(IrKind::FieldAccess) => {
                                    // pa-r17-006 (#984): emit field assignment lowering for (*p).f = value
                                    self.visit_field_assign(node_id, arena);
                                }
                                Some(IrKind::Var) => {
                                    // #1116: emit var assignment lowering for counter = v
                                    self.visit_var_assign(node_id, arena);
                                }
                                _ => {
                                    // Phase 7 m5-001: emit array-index assignment lowering for a[i] = expr.
                                    self.visit_store(node_id, arena);
                                }
                            }
                        }
                        IrKind::RecordCons => {
                            // Phase 6 m3-004: emit record constructor lowering for cap-mint shape.
                            // #1086: skip if another lowering path owns this node
                            if !self.state.was_record_cons_handled(node_id.get()) {
                                self.visit_record_cons(node_id, arena);
                            }
                        }
                        IrKind::EnumCons => {
                            // PA-r17-007: emit enum variant constructor lowering.
                            self.visit_enum_cons(node_id, arena);
                        }
                        IrKind::EnumDiscriminant => {
                            // PA-r17-008: emit enum discriminant extraction.
                            self.visit_enum_discriminant(node_id, arena);
                        }
                        IrKind::Branch => {
                            // Phase 7 m1-001: emit if-then-else expression lowering.
                            self.visit_branch(node_id, arena);
                        }
                        IrKind::While => {
                            // Phase 7 m1-002: emit while-loop lowering.
                            self.visit_while(node_id, arena);
                        }
                        IrKind::Loop => {
                            // Phase 7 m1-008 (PA7-008): emit infinite loop lowering.
                            self.visit_loop(node_id, arena);
                        }
                        IrKind::Match => {
                            // Phase 7 m1-004 (PA7-007): emit match-expression lowering.
                            // PA10-005 §3.2: Thread typer through for arm-body type-routing.
                            // PA-r17-013 (#991): Skip if already emitted in trailing position.
                            // Otherwise emit with Discard tail context (result goes to RAX only).
                            if !self.state.was_match_emitted(node_id.get()) {
                                use crate::emit_block_body::TailContext;
                                self.visit_match(node_id, arena, typer, TailContext::Discard);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Transfer accumulated instructions from state to arena's instruction side-table.
        for (node_id, inst) in self.state.instructions.entries().iter() {
            arena.instructions_mut().insert(*node_id, inst.clone());
        }
    }

    /// Populate the DataSideTable for module-level data bindings.
    ///
    /// Walks the arena, recognizes module-level Let-Literal and Let-Uninit bindings, and
    /// inserts DataEntry records into the provided DataSideTable.
    ///
    /// Routing decisions (Phase 6 m5-002):
    /// - `let x : T = literal_expr` → Rodata (immutable, initialized)
    /// - `let mut x : T = literal_expr` → Data (mutable, initialized)
    /// - `let mut x : T = uninit` → Bss (mutable, uninitialized)
    ///
    /// Symbol names default to the binding identifier (to be resolved via
    /// name resolution in a full implementation).
    ///
    /// # Arguments
    /// * `arena` - The IR arena containing all nodes
    /// * `data_table` - The mutable data side-table to populate
    pub fn populate_data_table(arena: &IrArena, data_table: &mut DataSideTable) {
        crate::data_encoder::populate_data_table(arena, data_table)
    }

    /// PA-r15-009b (#1032): Populate rodata jump tables for @jump_table matches.
    ///
    /// Called after populate_data_table to synthesize rodata entries for dense
    /// match dispatch. Each rodata entry contains W64 relocations to arm body
    /// and default labels, indexed by (arm_value - min_arm).
    ///
    /// # Arguments
    /// * `arena` - The IR arena containing all nodes with jump table metadata
    /// * `data_table` - The mutable data side-table to populate with rodata entries
    pub fn populate_jump_tables(arena: &IrArena, data_table: &mut DataSideTable) {
        crate::data_encoder::populate_jump_tables(arena, data_table)
    }

    /// PA-r15-009b (#1032): Populate rodata jump tables from a mutable arena.
    ///
    /// Helper function that avoids borrow checker issues by taking a mutable
    /// reference to the arena and delegating to the internal implementation.
    pub fn populate_jump_tables_from_arena(arena: &mut IrArena) {
        crate::data_encoder::populate_jump_tables_from_mutable_arena(arena)
    }

    /// Phase 7 m4-003: Emit pending unsafe-block statement bodies.
    ///
    /// Issue #1088: After UnsafeWalker processes raw instructions and labels,
    /// emit any pending action statements (call expressions, etc.) through the
    /// standard IR emit pipeline. Statements not yet routable fire U1614 fallback.
    pub fn emit_pending_unsafe_bodies(
        &mut self,
        pending: Vec<u32>,
        arena: &IrArena,
        typer: Option<&paideia_as_types::TypeInterner>,
    ) {
        for id_u32 in pending {
            let Some(unsafe_id) = IrNodeId::new(id_u32) else { continue };
            for &child in arena.children(unsafe_id).iter() {
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
                        self.emit_action_stmt(child, arena, typer);
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

impl Default for EmitWalker {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
#[path = "emit_walker_tests.rs"]
mod tests;
