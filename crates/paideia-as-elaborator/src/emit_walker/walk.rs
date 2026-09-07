//! EmitWalker — top-level `walk` driver and its co-located helpers.
//!
//! Contains:
//!   * `walk`, `walk_with_typer`, `walk_inner` — the flat traversal that
//!     dispatches per-kind lowering.
//!   * `mark_matches_recursive` — #1208 pre-pass that suppresses Match
//!     double-emission when a Lambda body wraps a Match in a Block.
//!   * `sync_state_instructions_to_arena` — the one-way transfer of
//!     `state.instructions` into `arena.instructions_mut()` (#1146).
//!   * `populate_data_table`, `populate_jump_tables`,
//!     `populate_jump_tables_from_arena` — thin wrappers into
//!     `crate::data_encoder`.
//!
//! Split from `emit_walker.rs` (paideia-as#1411). The `walk_inner` fn body
//! is a single ~590-line monolith and stays in one file so its ordered
//! pre-pass → dispatch sequence remains readable end-to-end without
//! cross-file jumps.

use paideia_as_ir::instruction::InstrMode;
use paideia_as_ir::{DataSideTable, IrArena, IrKind, IrNodeId, Symbol, SymbolKind};

use super::EmitWalker;

impl EmitWalker {
    /// #1208: Recursively mark all Match nodes reachable from a given node.
    ///
    /// Called during the #1085 pre-pass to mark Match nodes that are owned by
    /// emit_block_body's dispatch (lines 656 and 1124 in emit_block_body.rs).
    /// Prevents the flat walker's Match arm (line 663) from double-emitting when
    /// the direct body of a Lambda is a Block containing a Match.
    ///
    /// Descends recursively through all children but stops at Lambda/Unsafe
    /// boundaries (those are owned-dispatch boundaries).
    pub(super) fn mark_matches_recursive(&mut self, id: IrNodeId, arena: &IrArena) {
        if let Some(n) = arena.get(id) {
            match n.kind {
                IrKind::Match => self.state.mark_match_emitted(id.get()),
                IrKind::Lambda | IrKind::Unsafe => return,
                _ => {}
            }
            for &child_id in arena.children(id) {
                self.mark_matches_recursive(child_id, arena);
            }
        }
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

        // paideia-as#1276 phase 3 pre-pass: propagate `LetInfo::no_frame`
        // from each Let→Lambda binding into the walker's
        // `lambda_no_frame` set BEFORE the main flat-walker loop visits
        // Lambdas. Node ids in the arena are allocated child-first, so a
        // Lambda's id is smaller than its wrapping Let's — without this
        // pre-pass the Lambda visit would fire before the Let-handler mark
        // and `is_lambda_no_frame` would return `false`, causing the
        // frame prologue to arm on annotated functions too.
        for i in 1..=arena.len() as u32 {
            let Some(node_id) = IrNodeId::new(i) else { continue };
            let Some(node) = arena.get(node_id) else { continue };
            if node.kind != IrKind::Let {
                continue;
            }
            let Some(no_frame) = arena.let_meta().get(node_id).map(|m| m.no_frame) else {
                continue;
            };
            if !no_frame {
                continue;
            }
            let Some(&rhs_id) = arena.children(node_id).first() else { continue };
            let Some(rhs_node) = arena.get(rhs_id) else { continue };
            if rhs_node.kind == IrKind::Lambda {
                self.state.mark_lambda_no_frame(rhs_id.get());
            }
        }

        // paideia-as#1301 (v0.21-003c, phase-2) pre-pass: build a
        // binding-name → AtomicOrdering lookup from every Let whose
        // `LetInfo::atomic` is set. The two RIP-relative memory-op
        // emit sites (`emit_mem_read_via_rip_sym`,
        // `emit_mem_write_via_rip_sym`) consult this map to decide
        // whether to bracket the mov with mfence for SeqCst accesses.
        // Keyed by the binding NAME (not IR id) because those emit
        // sites already receive a symbol name (the linker-visible name
        // used for the RIP-relative reloc) and cannot cheaply recover
        // the originating Let node id at the call site.
        for i in 1..=arena.len() as u32 {
            let Some(node_id) = IrNodeId::new(i) else { continue };
            let Some(node) = arena.get(node_id) else { continue };
            if node.kind != IrKind::Let {
                continue;
            }
            let Some(ord) = arena.let_meta().get(node_id).and_then(|m| m.atomic) else {
                continue;
            };
            let Some(binding_name) = arena.binding_names().get(node_id) else {
                continue;
            };
            self.state.atomic_bindings.insert(binding_name.to_string(), ord);
        }

        // paideia-as#1278 phase 2 pre-pass: propagate `LetInfo::interrupt`
        // from each Let→Lambda binding into the walker's `lambda_interrupt`
        // map BEFORE the main flat-walker loop visits Lambdas. Same
        // rationale as the `no_frame` pre-pass above — Lambda IR node ids
        // are strictly smaller than their wrapping Let's, so
        // `visit_lambda`'s ISR-entry hook would run before the Let-handler
        // stamp and see no interrupt marker without this pass.
        //
        // Phase-1 lower.rs already stamps `no_frame = true` alongside
        // `interrupt = Some(_)`, so a lambda that reaches this branch is
        // guaranteed to already be recorded in `lambda_no_frame` by the
        // pre-pass above. The `visit_lambda` interrupt-entry hook still
        // consults `lambda_interrupt` independently — it does not chain
        // through `lambda_no_frame` — so the two sets are populated in
        // parallel here rather than derived from each other.
        for i in 1..=arena.len() as u32 {
            let Some(node_id) = IrNodeId::new(i) else { continue };
            let Some(node) = arena.get(node_id) else { continue };
            if node.kind != IrKind::Let {
                continue;
            }
            let Some(intr) = arena
                .let_meta()
                .get(node_id)
                .and_then(|m| m.interrupt.clone())
            else {
                continue;
            };
            let Some(&rhs_id) = arena.children(node_id).first() else { continue };
            let Some(rhs_node) = arena.get(rhs_id) else { continue };
            if rhs_node.kind == IrKind::Lambda {
                self.state.mark_lambda_interrupt(rhs_id.get(), intr);
            }
        }

        // Fix B (#1085): Pre-pass to prevent Match double-emission.
        // #1208: Extended to recursively mark EVERY Match reachable from Lambda bodies,
        // not just direct children. For `fn(...) -> { match ... }`, the direct child
        // is a Block wrapping the Match. Without this recursion, the flat walker's
        // Match arm fires at wrong offset, then emit_block_body dispatches Match at
        // correct offset → double visit_match with ID collision and offset overflow.
        for i in 1..=arena.len() as u32 {
            if let Some(lambda_id) = IrNodeId::new(i) {
                if let Some(lambda_node) = arena.get(lambda_id) {
                    if lambda_node.kind == IrKind::Lambda {
                        let children = arena.children(lambda_id);
                        // Lambda has one child: the body expression
                        if let Some(&body_id) = children.first() {
                            self.mark_matches_recursive(body_id, arena);
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
                            let is_data_let = arena.data().get(node_id).is_some();
                            if is_data_let {
                                self.state.mark_data_let_handled(node_id.get());
                            }

                            let children = arena.children(node_id);
                            // Statement-level Let children: [name_var, value, ty?], RHS at index 1.
                            // Direct allocations (unit tests): [value], RHS at index 0.
                            let rhs_idx = if children.len() > 1 { 1 } else { 0 };
                            if let Some(&rhs_id) = children.get(rhs_idx) {
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
                                        // #1145: Let → EnumCons → RecordCons (record payload
                                        // nested inside an enum variant constructor): owned by
                                        // data_encoder::encode_enum_cons, which recursively
                                        // encodes the payload via encode_record_cons. The
                                        // walker's visit_record_cons only understands the
                                        // Phase 6 m3-004 cap-mint shape (4 u64 fields at
                                        // offsets [0,8,16,24]) and must not independently
                                        // visit a RecordCons payload that data_encoder has
                                        // already correctly serialised to bytes. Gated on
                                        // `is_data_let` so a RecordCons payload is only
                                        // suppressed when data_encoder actually produced a
                                        // data-table entry for this Let (i.e. encode_enum_cons
                                        // succeeded); if it didn't, visit_record_cons should
                                        // still get a chance to diagnose a real problem.
                                        IrKind::EnumCons => {
                                            self.state.mark_enum_cons_handled(rhs_id.get());
                                            if is_data_let {
                                                if let Some(&payload_id) =
                                                    arena.children(rhs_id).first()
                                                {
                                                    if let Some(payload_node) =
                                                        arena.get(payload_id)
                                                    {
                                                        if payload_node.kind == IrKind::RecordCons {
                                                            self.state.mark_record_cons_handled(
                                                                payload_id.get(),
                                                            );
                                                        }
                                                    }
                                                }
                                            }
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
                                            // #1198: Mark EnumCons arguments as handled so emit_call_args_and_call
                                            // and main-loop visit_enum_cons don't double-emit the variant index load.
                                            for &arg_id in app_children.iter().skip(1) {
                                                if let Some(arg_node) = arena.get(arg_id) {
                                                    if arg_node.kind == IrKind::EnumCons {
                                                        self.state.mark_enum_cons_handled(arg_id.get());
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
                                        // #1224: Lambda -> EnumCons body owned by visit_lambda; preempt flat pass at ~line 670.
                                        IrKind::EnumCons => {
                                            self.state.mark_enum_cons_handled(body_id.get());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        IrKind::Store => {
                            // #1146: Store → FieldAccess(Deref(...))     — owned by visit_field_assign.
                            // #1184-corr: Store → FieldAccess(Var-module) — owned by visit_field_assign's
                            //   module_field_refs fallback (emit_module_field_write). module_field_refs
                            //   is populated by lower/field_access.rs ONLY for non-deref receivers whose
                            //   binding is not a struct-typed local (i.e. module names), so membership is
                            //   authoritative — no receiver-kind test needed for case B.
                            // Mark the FieldAccess handled BEFORE flat dispatch reaches its (lower) id,
                            // else visit_field_access_with_reg emits a spurious orphan load under the
                            // FieldAccess node id (regression on 05f2017).
                            let children = arena.children(node_id);
                            if let Some(&fa_id) = children.first() {
                                if let Some(fa_node) = arena.get(fa_id) {
                                    if fa_node.kind == IrKind::FieldAccess {
                                        let is_deref_recv = arena.children(fa_id).first()
                                            .and_then(|&r| arena.get(r))
                                            .map(|n| n.kind == IrKind::Deref)
                                            .unwrap_or(false);
                                        let is_module_recv = arena.module_field_refs().get(fa_id).is_some();
                                        if is_deref_recv || is_module_recv {
                                            self.state.mark_field_access_handled(fa_id.get());
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // #1233 Phase B: Pre-pass 1 to register closure body Lambda symbols.
        // Scans arena for ClosureCons nodes and extracts their body Lambda children,
        // registering them with mangled names like `closure_<parent>_<lambda_id>`.
        self.register_closure_body_symbols(arena);

        // #1233 Phase B: Pre-pass 2 to compute caller Lambda frame layouts for closures.
        // Scans arena for ClosureCons descendants under each Lambda, assigns (fat_off, env_off) slots.
        for i in 1..=arena.len() as u32 {
            if let Some(caller_lambda_id) = IrNodeId::new(i) {
                if let Some(caller_node) = arena.get(caller_lambda_id) {
                    if caller_node.kind == IrKind::Lambda {
                        self.precompute_caller_frame(caller_lambda_id, arena);
                    }
                }
            }
        }

        // #1239 Phase B: Pre-pass 3 to detect nested closure invocations (T0575).
        // Scans each closure body lambda for invocations of inner closure-typed bindings,
        // which would clobber R14 (the outer closure's env-ptr) with no preservation.
        self.check_nested_closure_invocations(arena);

        // Iterate over all nodes, looking for Let, Lambda, and Unsafe nodes.
        for i in 1..=arena.len() as u32 {
            if let Some(node_id) = IrNodeId::new(i) {
                if let Some(node) = arena.get(node_id) {
                    let node_kind = node.kind;
                    match node_kind {
                        IrKind::Let => {
                            // Issue #1212: Skip statement-scope Lets (function-local bindings).
                            // These are handled by emit_block_body's Let arm via insert_pair.
                            if arena.is_stmt_let(node_id) {
                                continue;
                            }

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
                                    // paideia-as#1276 phase 3: record the `@no_frame` opt-out
                                    // for this Lambda binding so `visit_lambda` / `emit_ret`
                                    // suppress the default SysV frame-pointer
                                    // prologue/epilogue. The flag lives on the Let's
                                    // `LetInfo::no_frame` (populated by phase-1 lower.rs's
                                    // `populate_let_meta`); mirror it into the emit-state
                                    // set keyed by the Lambda's IR node id — the same key
                                    // `visit_lambda` and `emit_ret` see (`current_function`).
                                    let no_frame = arena.let_meta()
                                        .get(node_id)
                                        .map(|meta| meta.no_frame)
                                        .unwrap_or(false);
                                    if no_frame {
                                        self.state.mark_lambda_no_frame(rhs_id.get());
                                    }

                                    // paideia-as#1278 phase 2: mirror the ISR-entry
                                    // marker into the emit state. The pre-pass above
                                    // (`lambda_interrupt`) covers the id-preorder gap,
                                    // but stamping here as well keeps the two paths
                                    // symmetric with `no_frame` — a follow-up that
                                    // populates `LetInfo::interrupt` after the pre-pass
                                    // (e.g. from a late lowering stage) would still be
                                    // visible to `visit_lambda`.
                                    let interrupt = arena.let_meta()
                                        .get(node_id)
                                        .and_then(|meta| meta.interrupt.clone());
                                    if let Some(intr) = interrupt {
                                        self.state.mark_lambda_interrupt(rhs_id.get(), intr);
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
                                    // #1187: module-qualified FA RHS is owned by emit_block_body's Let-arm
                                    // FieldAccess branch (runs inside visit_lambda's Action arm, after
                                    // pending_first_instr_lambda = Some(L) is set — load captured as
                                    // lambda_first_instr[L], keeping bytes inside the function symbol range).
                                    // Struct-typed FA RHS keeps the pre-existing flat-walker path.
                                    if arena.module_field_refs().get(rhs_id).is_none() {
                                        self.visit_let_field_access(node_id, rhs_id, arena);
                                    }
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

                            // #1139: also record stmt-position Unsafes (visit_lambda's IrKind::Unsafe
                            // arm only covers Lambda→Unsafe body form). current_function was last set
                            // on the enclosing Lambda in id-preorder.
                            self.state.unsafe_body_to_lambda
                                .entry(node_id.get())
                                .or_insert(self.state.current_function);
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
                            // #1198: skip if another lowering path owns this node
                            if !self.state.was_enum_cons_handled(node_id.get()) {
                                self.visit_enum_cons(node_id, arena);
                            }
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
        self.sync_state_instructions_to_arena(arena);
    }

    /// Copy every instruction accumulated in `self.state.instructions` into
    /// the arena's instruction side-table.
    ///
    /// #1146 follow-up: the encoder and `resolve_var_operands` read only
    /// from `arena.instructions()` — never from the walker's own
    /// `self.state.instructions`. `walk_inner` used to perform this copy
    /// exactly once, as its final step. That left every instruction emitted
    /// by code paths that run *after* `walk()` returns — chiefly
    /// `emit_pending_unsafe_bodies` (issue #1088: call/field-write
    /// statements inside `unsafe { block: {...} } }`, routed through
    /// `emit_action_stmt` → `dispatch_store`/`emit_call_stmt`) — stranded in
    /// `self.state.instructions` and silently absent from the emitted
    /// `.text`, with no diagnostic. Idempotent: re-inserting an
    /// already-transferred entry is harmless, so callers may call this any
    /// number of times as new instructions accumulate.
    pub(crate) fn sync_state_instructions_to_arena(&self, arena: &mut IrArena) {
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
}
