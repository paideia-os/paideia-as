//! EmitWalker — Phase 5 m1-001 entry to the build-emit pipeline.
//!
//! Walks the IR; per-construct lowering (m1-002 Let-literal, m1-003 Lambda,
//! m1-004 Unsafe) lands as siblings inside this module. The walker
//! populates an InstructionSideTable + tracks per-function offsets.

use paideia_as_diagnostics::{Diagnostic, DiagnosticCode};
use paideia_as_ir::instruction::{InstrMode, Instruction, Mnemonic, Operand};
#[cfg(test)]
use paideia_as_ir::instruction::{Cond, IntWidth};
#[cfg(test)]
use paideia_as_ir::record_layout::{FieldLayout, RecordLayout, RecordTypeId};
#[cfg(test)]
use paideia_as_ir::{EnumLayout, EnumTypeId};
use paideia_as_ir::{DataSideTable, IrArena, IrKind, IrNodeId, SmallVec, Symbol, SymbolKind, abi};

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

    /// Drain and return the legacy free-form diagnostics accumulated during the walk.
    ///
    /// Called once by `cmd_build::run` after typed diagnostics are drained
    /// (issue #1082). Each message is wrapped in a U1616 Diagnostic before emission.
    /// Post-#1086 migration, this channel holds only non-T-coded internal errors
    /// (invariant violations, missing side-tables, unpopulated layouts).
    /// Any fire indicates a silent-broken-.o class bug.
    #[must_use]
    pub fn take_legacy_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
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
        // paideia-as#1276 phase 3: lazy frame-prologue injection.
        // If `visit_lambda` armed `pending_frame_prologue` for the current
        // function, inject `push rbp; mov rbp, rsp` BEFORE this instruction
        // — that way (a) the prologue sorts first in this function's
        // .text bytes, (b) its `push rbp` claims `lambda_first_instr[L]`
        // so the ELF symbol starts AT the prologue (not after), and (c)
        // lambdas whose body-shape arm never calls `emit_inst` produce
        // zero bytes and continue to flag B1704, preserving the pre-#1276
        // diagnostic contract.
        //
        // Take the arm BEFORE the recursive calls so those calls don't
        // re-enter this branch and blow the stack.
        if self.state.current_function != 0
            && self.state.take_pending_frame_prologue(self.state.current_function)
        {
            self.state.mark_frame_prologue_emitted(self.state.current_function);
            let fn_id = self.state.current_function;

            // Emit: push rbp
            let mut push_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            push_operands.push(Operand::Reg(abi::RBP));
            let push_inst = Instruction {
                mnemonic: Mnemonic::Push,
                operands: push_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let push_id = self.alloc_synthetic_id();
            self.emit_inst(push_id, push_inst);

            // Emit: mov rbp, rsp
            let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            mov_operands.push(Operand::Reg(abi::RBP));
            mov_operands.push(Operand::Reg(abi::RSP));
            let mov_inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: mov_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let mov_id = self.alloc_synthetic_id();
            self.emit_inst(mov_id, mov_inst);

            // Claim `lambda_first_instr[fn_id]` for `push rbp` — the ELF
            // symbol for this function starts at the frame prologue, not at
            // the body's first instruction. `record_lambda_entry` on the
            // body-shape arms (Var/BitNot/Cast/Literal) uses `.or_insert`
            // and may have already recorded a body-instruction id; the
            // App/Action/Match/EnumCons/Store arms arm
            // `pending_first_instr_lambda` and the recursive `emit_inst` for
            // `push rbp` above would have consumed it. Either way, force the
            // entry to `push_id` so the resulting ELF symbol's address
            // matches the first executed byte of the function. Anything else
            // means control jumps INTO the function past `push rbp`,
            // leaving `pop rbp` in the epilogue to pull off the caller's
            // frame instead.
            self.state.lambda_first_instr.insert(fn_id, push_id);
        }

        let bytes = paideia_as_encoder::estimated_bytes(&inst);
        inst.emission_order = self.state.next_emission_order;
        self.state.next_emission_order += 1;
        // #1139: Record which lambda owns this instruction.
        self.state.instr_to_lambda.insert(node_id, self.state.current_function);
        self.state.instructions.insert(node_id, inst);
        // PA8-m1-002c: Capture first instruction of pending lambda before offset advances.
        // NOTE (#994): this is an unconditional overwrite, not entry/or_insert — at
        // least one existing body-shape path (Match, exercised by
        // caller_app_pair_arg_value) relies on a *later* arm+consume cycle for the
        // same lambda id winning over an earlier one. Closures avoid needing a
        // competing semantics here: the closure-frame prologue (visit_lambda,
        // emit_visit_lambda.rs) guards each body-shape arm's own
        // `pending_first_instr_lambda` (re-)arming so it only fires when this
        // lambda has no `lambda_first_instr` entry yet, letting the prologue's
        // claim (the function's true entry point) stick without changing this
        // shared, order-sensitive mechanism's semantics for every other caller.
        if let Some(lid) = self.state.pending_first_instr_lambda.take() {
            self.state.lambda_first_instr.insert(lid, node_id);
            self.state.mark_lambda_emitted(lid);
        }
        self.state.estimated_offset += bytes;
    }

    /// #1141: Allocate a monotonic synthetic IrNodeId for instructions that
    /// have no natural AST-derived id (bridge saves, CALL sites, indirect-call
    /// scaffolds). These are identity-only post-#1140 — `.text` order is
    /// governed by emission_order, so this counter just needs to hand out
    /// unique ids that don't collide with arena ids or each other.
    pub(crate) fn alloc_synthetic_id(&mut self) -> IrNodeId {
        let id = self.state.next_synthetic_id;
        self.state.next_synthetic_id = self.state.next_synthetic_id.saturating_add(1);
        IrNodeId::new(id).expect("synthetic id must be non-zero")
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

    /// Emit epilogue (add rsp if needed, frame-pointer restore if applicable)
    /// followed by ret.
    ///
    /// #1233 Phase A: Looks up frame_size from closure_frame_meta for
    /// current_function. If total_size > 0, emits `add rsp, total_size`
    /// before `ret`.
    ///
    /// paideia-as#1276 phase 3: Also emits the default SysV frame-pointer
    /// epilogue `mov rsp, rbp; pop rbp` before `ret`, mirroring the
    /// `push rbp; mov rbp, rsp` prologue that `visit_lambda` emits at
    /// function entry. Suppressed when `current_function`'s Lambda binding
    /// was annotated `@no_frame`.
    ///
    /// Emission order — enforced by insertion order (each `emit_inst` bumps
    /// `emission_order`):
    ///   1. `add rsp, N`     (closure-frame teardown; existing)
    ///   2. `mov rsp, rbp`   (frame-pointer restore; unless @no_frame)
    ///   3. `pop rbp`        (frame-pointer restore; unless @no_frame)
    ///   4. `ret`
    ///
    /// The `add rsp, N` + `mov rsp, rbp` pair is redundant (the mov
    /// overwrites rsp), but harmless: the add's estimated_bytes are
    /// accounted for and the resulting bytes are equivalent. Keeping the
    /// add preserves byte-exactness for @no_frame closure tests where
    /// only the add fires.
    pub(crate) fn emit_ret(&mut self, ret_id: IrNodeId, arena: &IrArena) {
        // Check if current function has frame layout
        if let Some(lambda_id) = IrNodeId::new(self.state.current_function) {
            if let Some(frame_layout) = arena.closure_frame_meta().get(lambda_id) {
                if frame_layout.total_size > 0 {
                    // Emit: add rsp, total_size
                    let mut add_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                    add_operands.push(Operand::Reg(abi::RSP));
                    add_operands.push(Operand::Imm64(frame_layout.total_size as i64));

                    let add_inst = Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    };
                    let add_id = self.alloc_synthetic_id();
                    self.emit_inst(add_id, add_inst);
                }
            }
        }

        // paideia-as#1276 phase 3: frame-pointer epilogue for non-@no_frame
        // functions whose prologue actually fired. Matches the lazy
        // `push rbp; mov rbp, rsp` prologue injected by `emit_inst` on the
        // first body instruction. Skip when:
        //   * `current_function == 0` (called outside any Lambda scope — a
        //     caller bug; emitting nothing extra is the safe recovery),
        //   * the lambda opted out via `@no_frame`, OR
        //   * the prologue never actually fired (body-shape arm produced no
        //     instructions, `pending_frame_prologue` is still armed). Emitting
        //     `mov rsp, rbp; pop rbp` without a matching prologue would pop
        //     off the caller's frame — silently corrupting whatever value
        //     was above the return address.
        if self.state.current_function != 0
            && !self.state.is_lambda_no_frame(self.state.current_function)
            && self.state.was_frame_prologue_emitted(self.state.current_function)
        {
            // Emit: mov rsp, rbp
            let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            mov_operands.push(Operand::Reg(abi::RSP));
            mov_operands.push(Operand::Reg(abi::RBP));
            let mov_inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: mov_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let mov_id = self.alloc_synthetic_id();
            self.emit_inst(mov_id, mov_inst);

            // Emit: pop rbp
            let mut pop_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            pop_operands.push(Operand::Reg(abi::RBP));
            let pop_inst = Instruction {
                mnemonic: Mnemonic::Pop,
                operands: pop_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let pop_id = self.alloc_synthetic_id();
            self.emit_inst(pop_id, pop_inst);
        }

        // Emit: ret
        let ret_inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        self.emit_inst(ret_id, ret_inst);
    }

    /// Get the set of Lambda IR node IDs that emitted bytecode.
    #[must_use]
    pub fn emitted_lambdas(&self) -> &std::collections::HashSet<u32> {
        &self.state.emitted_lambdas
    }

    /// Record a lambda's entry point instruction and mark it as emitted.
    ///
    /// Called at the START of each emit_*_lambda function to record:
    /// 1. The first instruction's IrNodeId (for post-encoding offset projection via offset_map)
    /// 2. Marks the lambda as emitted for symbol filtering
    pub fn record_lambda_entry(&mut self, lambda_id: IrNodeId, first_instr_id: IrNodeId) {
        // Record the first instruction's IR node ID for offset_map projection
        self.state
            .lambda_first_instr
            .entry(lambda_id.get())
            .or_insert(first_instr_id);

        self.state.mark_lambda_emitted(lambda_id.get());
    }

    /// #1208: Recursively mark all Match nodes reachable from a given node.
    ///
    /// Called during the #1085 pre-pass to mark Match nodes that are owned by
    /// emit_block_body's dispatch (lines 656 and 1124 in emit_block_body.rs).
    /// Prevents the flat walker's Match arm (line 663) from double-emitting when
    /// the direct body of a Lambda is a Block containing a Match.
    ///
    /// Descends recursively through all children but stops at Lambda/Unsafe
    /// boundaries (those are owned-dispatch boundaries).
    fn mark_matches_recursive(&mut self, id: IrNodeId, arena: &IrArena) {
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

    /// #1233 Phase B: Register closure body Lambda symbols in the arena's symbol table.
    ///
    /// Scans all ClosureCons nodes in the arena and extracts their body Lambda children.
    /// Each closure body Lambda is registered with a mangled symbol name:
    /// `closure_<parent_binding_name>_<lambda_id>`.
    ///
    /// Closure body symbols are registered as Function kind with Local visibility.
    /// The symbol's ir_node field points to the Lambda node ID, enabling offset
    /// lookup through function_offsets in the downstream ELF builder.
    fn register_closure_body_symbols(&mut self, arena: &mut IrArena) {
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
    fn precompute_caller_frame(&mut self, caller_lambda_id: IrNodeId, arena: &mut IrArena) {
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
    fn check_nested_closure_invocations(&mut self, arena: &IrArena) {
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

impl Default for EmitWalker {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
#[path = "emit_walker_tests.rs"]
mod tests;
