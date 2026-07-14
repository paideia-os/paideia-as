//! Block-body emit paths (multi-statement function bodies + match arm bodies).
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Hosts the
//! twin lowerings that walk the statement children of an `Action` block:
//!
//! - `emit_block_body`     — Lambda → Action shape at function level
//! - `emit_block_body_arm` — same shape inside a match arm body
//!
//! Both walk `Let` / `StmtExpr` / `RawInstruction` children, allocating
//! scratch registers via `state.local_bindings` and emitting the tail
//! expression to RAX.

use paideia_as_ir::instruction::{Cond, Instruction, IntWidth, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi};
use paideia_as_diagnostics::{Category, DiagnosticCode, Severity};

use crate::emit_walker::EmitWalker;
use crate::emit_store_record::is_operator_callee;

/// PA-r17-013 (#991): Tracks the tail-expression context for proper return-value placement.
/// When an expression appears in trailing position, its result must land in the correct
/// location per the function's return convention, not RAX (which is for discarded values).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TailContext {
    /// Not the trailing expression — value is discarded.
    Discard,
    /// Trailing expression, result must land in RAX only.
    ReturnRax,
    /// Trailing expression, result must land in RAX (discriminant) + RDX (payload).
    ReturnRaxRdx,
    /// Trailing expression, result must be written to [RDI + disp] (indirect return).
    ReturnIndirect {
        /// Discriminant size in bytes for discriminant-only enums.
        disc_size: i32,
    },
}

/// Helper to construct T0527 diagnostic code.
fn t0527_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 527)
        .expect("T0527 is within valid T range")
}

/// Helper to construct U1621 diagnostic code (Branch shape invariant).
/// Shared code with emit_control_flow::visit_branch — slice A1 minted; A3 reuses.
fn u1621_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1621)
        .expect("U1621 is within valid U range")
}

/// Helper to construct U1642 diagnostic code (RawInstruction payload invariant).
/// Slice A3 mint; reclassifies former T0526 emissions in emit_block_body.
fn u1642_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1642)
        .expect("U1642 is within valid U range")
}

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

    /// Phase 7 m1-001: Emit multi-statement block body.
    ///
    /// Handles `Lambda → Action` shape for block-bodied functions:
    /// - For each `Let` statement child: emit value expression, bind result to next scratch reg
    /// - For each `StmtExpr` statement child: emit expression, discard result
    /// - For the final expression (tail): emit to RAX as return value
    pub(crate) fn emit_block_body(
        &mut self,
        block_id: IrNodeId,
        arena: &IrArena,
        typer: Option<&paideia_as_types::TypeInterner>,
    ) {
        let block_children = arena.children(block_id);
        if cfg!(debug_assertions) {
            eprintln!(
                "[emit_block_body] Block {} has {} children",
                block_id.get(),
                block_children.len()
            );
        }

        // Scratch register sequence for in-block let bindings.
        // Exclude RAX since it's clobbered by function calls (used for return values).
        // Use RCX, RDX, R8, R9 instead to avoid conflicts with call results.
        let scratch_regs = [abi::RCX, abi::RDX, abi::R8, abi::R9]; // RCX, RDX, R8, R9

        // Walk all children: statements + optional tail.
        for (i, &child_id) in block_children.iter().enumerate() {
            if let Some(child_node) = arena.get(child_id) {
                match child_node.kind {
                    IrKind::Let => {
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body] Let statement at index {}", i);
                        }
                        // This is a let binding. Emit the value expression.
                        // Statement-level Let children: [name_var, value, ty?], RHS at index 1.
                        // Direct allocations (unit tests): [value], RHS at index 0.
                        let let_children = arena.children(child_id);
                        let rhs_idx = if let_children.len() > 1 { 1 } else { 0 };
                        if let Some(&rhs_id) = let_children.get(rhs_idx) {
                            if let Some(rhs_node) = arena.get(rhs_id) {
                                // Assign next scratch register if available.
                                if self.state.scratch_count() >= scratch_regs.len() {
                                    // Register pressure exceeded.
                                    self.push_typed_diag(
                                        t0527_code(),
                                        format!(
                                            "register pressure exceeded in Phase 7 Let-literal bindings: more than {} in-flight bindings",
                                            scratch_regs.len()
                                        ),
                                    );
                                    return;
                                }

                                let scratch_reg = scratch_regs[self.state.scratch_count()];
                                self.state.assign_scratch(scratch_reg);

                                // Get binding name from arena.binding_names()
                                // After Phase 6 m2-004b, all local let bindings have entries in binding_names table
                                let binding_name = arena
                                    .binding_names()
                                    .get(child_id)
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| format!("_let_{}", child_id.get()));

                                // Edit A: Handle Literal RHS
                                if rhs_node.kind == IrKind::Literal {
                                    if let Some(value) = arena.literal_values().get(rhs_id) {
                                        // Allocate scratch register and emit mov instruction
                                        self.state
                                            .local_bindings
                                            .insert(binding_name.clone(), scratch_reg);

                                        // PA8-m3-001: this is a (Reg, Imm64) move — the one
                                        // operand shape MovSized accepts — and `child_id` is the
                                        // Let node, so its declared width is recoverable from the
                                        // let-meta table. Resolve it and width-route exactly as
                                        // visit_let_literal does; untyped bindings (no typer, no
                                        // recorded type, or W64) keep the generic 64-bit path.
                                        let width = typer.and_then(|typer| {
                                            Self::resolve_let_width(arena, child_id, typer)
                                        });

                                        // Emit: mov scratch_reg, imm64 (or MovSized for sub-64-bit).
                                        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                        operands.push(Operand::Reg(scratch_reg));
                                        operands.push(Operand::Imm64(value));

                                        let (mnemonic, _inst_size) = match width {
                                            Some(
                                                w @ (IntWidth::W8 | IntWidth::W16 | IntWidth::W32),
                                            ) => (
                                                Mnemonic::MovSized { width: w },
                                                w.estimated_size(),
                                            ),
                                            _ => {
                                                // Generic 64-bit Mov: i32 → 7 bytes, i64 → 10.
                                                let size = if value >= i32::MIN as i64
                                                    && value <= i32::MAX as i64
                                                {
                                                    7
                                                } else {
                                                    10
                                                };
                                                (Mnemonic::Mov, size)
                                            }
                                        };

                                        let inst = Instruction {
                                            mnemonic,
                                            operands,
                                            encoding_hint: None,
                                            byte_offset_in_text: None,
                                            mode: self.current_mode(),
                                        emission_order: 0,
                                        };

                                        // Use virtual ID: child_id * 3 + offset to ensure proper sorting
                                        let inst_id = IrNodeId::new(child_id.get() * 3)
                                            .expect("let literal instr id");
                                        self.emit_inst(inst_id, inst);
                                    }
                                }
                                // Edit B: Handle Unsafe RHS
                                else if matches!(rhs_node.kind, IrKind::Unsafe { .. }) {
                                    // Record binding in local_bindings but don't emit instruction
                                    // UnsafeWalker will handle the body via existing pending queue
                                    self.state
                                        .local_bindings
                                        .insert(binding_name.clone(), scratch_reg);
                                }
                                // Edit C: Handle RawInstruction RHS (future lowering placeholder)
                                else if rhs_node.kind == IrKind::RawInstruction {
                                    if let Some(inst) = arena.instructions().get(rhs_id) {
                                        // Check if this is a value-producing Mov instruction
                                        if inst.mnemonic == Mnemonic::Mov {
                                            // PA8-m3-001 (not width-routed): this Mov is *cloned*
                                            // from a pre-lowered RawInstruction whose mnemonic and
                                            // operand shape are fixed upstream; we only rewrite its
                                            // destination register. The original operand shape is
                                            // unknown here (it may be reg-reg or a memory form that
                                            // MovSized cannot encode), so the generic mnemonic is
                                            // preserved verbatim.
                                            let mut cloned = inst.clone();
                                            if let Some(first_op) = cloned.operands.get_mut(0) {
                                                *first_op = Operand::Reg(scratch_reg);
                                            }

                                            self.state
                                                .local_bindings
                                                .insert(binding_name.clone(), scratch_reg);

                                            self.emit_inst(rhs_id, cloned);
                                        }
                                    }
                                }
                                // Edit D: Handle App RHS (function calls and operators) - #1152 / #1191
                                else if rhs_node.kind == IrKind::App {
                                    if let Some(meta) = arena.call_sites().get(rhs_id) {
                                        // #1191 corrective: check if this is an operator or function call
                                        if is_operator_callee(&meta.callee_name) {
                                            // Operator App at let-RHS: emit the binary operation into scratch_reg
                                            // Register binding FIRST so emit has access
                                            self.state
                                                .local_bindings
                                                .insert(binding_name.clone(), scratch_reg);
                                            // Emit operator into scratch_reg (mirrors tail-App dispatch from lines 571-619)
                                            let _ = self.emit_var_assign_expr_to_reg(rhs_id, arena, scratch_reg, 0);
                                        } else {
                                            // Real function call (callee is not an operator)
                                            let app_children = arena.children(rhs_id);
                                            let arg_ids: Vec<IrNodeId> = app_children[1..].to_vec();
                                            // Use state.current_function (the enclosing lambda's id),
                                            // NOT child_id (the Let node id).
                                            let lambda_id = IrNodeId::new(self.state.current_function)
                                                .expect("current_function set by walker");
                                            self.emit_call_expr(lambda_id, meta.callee_name.clone(), &arg_ids, arena);
                                            if scratch_reg != abi::RAX {
                                                // mov scratch_reg, rax — materialize the CALL result.
                                                let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                                ops.push(Operand::Reg(scratch_reg));
                                                ops.push(Operand::Reg(abi::RAX));
                                                let inst = Instruction {
                                                    mnemonic: Mnemonic::Mov,
                                                    operands: ops,
                                                    encoding_hint: None,
                                                    byte_offset_in_text: None,
                                                    mode: self.current_mode(),
                                                    emission_order: 0,
                                                };
                                                let inst_id = IrNodeId::new(1_200_000 + child_id.get())
                                                    .expect("let-app materialize id");
                                                self.emit_inst(inst_id, inst);
                                            }
                                            self.state.local_bindings.insert(binding_name.clone(), scratch_reg);
                                        }
                                    }
                                }
                                else if rhs_node.kind == IrKind::BitNot {
                                    // #1194: Handle BitNot RHS (~expr) — route through #1181 lowerer to emit
                                    // mov dest, operand ; not dest. Without this, the catch-all at #1138
                                    // records the binding but never emits the operation.
                                    self.state
                                        .local_bindings
                                        .insert(binding_name.clone(), scratch_reg);
                                    let _ = self.emit_var_assign_expr_to_reg(rhs_id, arena, scratch_reg, 0);
                                }
                                else if rhs_node.kind == IrKind::FieldAccess {
                                    // #1187: module-qualified field-read Let-RHS `let x = M.f`.
                                    // Emit RIP-relative load into scratch_reg INSIDE the enclosing lambda's
                                    // Action arm — pending_first_instr_lambda captures this instruction as
                                    // lambda_first_instr[L], keeping the load bytes inside the function
                                    // symbol range. Struct-typed FA RHS falls to the #1138 else fallback,
                                    // where the flat walker's visit_let_field_access already emitted the load
                                    // (existing pattern; unchanged for #1187 scope).
                                    if let Some(field_name) = arena.module_field_refs().get(rhs_id) {
                                        let name_owned = field_name.to_string();
                                        self.state
                                            .local_bindings
                                            .insert(binding_name.clone(), scratch_reg);
                                        self.emit_module_field_read(rhs_id, scratch_reg, name_owned);
                                    } else {
                                        // Struct-typed FA RHS: flat walker's visit_let_field_access emitted
                                        // the load (existing pattern). Just record the binding here so the
                                        // tail Var arm can resolve it.
                                        self.state
                                            .local_bindings
                                            .insert(binding_name.clone(), scratch_reg);
                                    }
                                }
                                // #1138: Handle other RHS kinds (e.g., Var) by just recording binding
                                // without emitting instructions. Instruction emission is deferred or N/A.
                                else {
                                    self.state
                                        .local_bindings
                                        .insert(binding_name.clone(), scratch_reg);
                                }

                                if cfg!(debug_assertions) {
                                    eprintln!(
                                        "[emit_block_body] Let binding {} uses scratch reg {:?}",
                                        binding_name, scratch_reg
                                    );
                                }
                            }
                        }
                    }
                    IrKind::Action => {
                        // This is a StmtExpr (statement expression). Emit it and discard result.
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body] StmtExpr at index {}", i);
                        }
                        self.emit_action_stmt(child_id, arena, typer);
                    }
                    IrKind::RawInstruction => {
                        // Phase 7 m2-001 (PA7C-m2-001): RawInstruction child of Action.
                        // Look up the instruction payload in the side-table.
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body] RawInstruction at index {}", i);
                        }
                        if let Some(inst) = arena.instructions().get(child_id) {
                            self.emit_inst(child_id, inst.clone());
                        } else {
                            // #1147 A3: invariant violation — RawInstruction lacks side-table payload.
                            self.push_typed_diag(
                                u1642_code(),
                                format!(
                                    "Instruction payload not found in side-table for RawInstruction node {} (internal compiler error)",
                                    child_id.get()
                                ),
                            );
                        }
                    }
                    IrKind::Var => {
                        // Phase 7 m2-003: Bare identifier in statement or final-expression position.
                        // If this is the final expression (last child), move its value to RAX for return.
                        // Otherwise it's a statement-form variable reference with no side effects.
                        if i == block_children.len() - 1 {
                            // Final expression: move variable's value to RAX
                            if cfg!(debug_assertions) {
                                eprintln!(
                                    "[emit_block_body] Var (final expression) at index {} — moving to RAX",
                                    i
                                );
                            }

                            // Look up the variable's current register
                            if let Some(var_name) = arena.binding_names().get(child_id) {
                                if let Some(src_reg) = self.state.local_bindings.get(var_name) {
                                    if src_reg != abi::RAX {
                                        // Emit: mov rax, src_reg
                                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                        ops.push(Operand::Reg(abi::RAX));
                                        ops.push(Operand::Reg(src_reg));
                                        let inst = Instruction {
                                            mnemonic: Mnemonic::Mov,
                                            operands: ops,
                                            encoding_hint: None,
                                            byte_offset_in_text: None,
                                            mode: self.current_mode(),
                                            emission_order: 0,
                                        };
                                        let inst_id = IrNodeId::new(child_id.get() * 3 + 2)
                                            .expect("final var mov id");
                                        self.emit_inst(inst_id, inst);
                                    }
                                }
                            }
                        } else {
                            // Statement-form variable reference with no side effects
                            if cfg!(debug_assertions) {
                                eprintln!(
                                    "[emit_block_body] Var (bare identifier) at index {} — skipped",
                                    i
                                );
                            }
                        }
                    }
                    IrKind::Branch => {
                        // PA8-m2-001: Branch as the final expression of a unit-typed block.
                        // When a Branch appears in emit_block_body, it's the value-returning expression.
                        // We need to emit the test, conditional jumps, and arm bodies WITHOUT emitting ret.
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body] Branch at index {} (final expression)", i);
                        }

                        let branch_children = arena.children(child_id);
                        if branch_children.len() < 2 {
                            self.push_typed_diag(
                                u1621_code(),
                                format!(
                                    "Branch node {} has {} children; expected at least 2",
                                    child_id.get(),
                                    branch_children.len()
                                ),
                            );
                            return;
                        }

                        let _cond_id = branch_children[0];
                        let _then_id = branch_children[1];
                        let else_id = if branch_children.len() > 2 {
                            Some(branch_children[2])
                        } else {
                            None
                        };

                        // Generate unique label names per branch node.
                        let then_label = format!("if_then_{}", child_id.get());
                        let else_label = format!("if_else_{}", child_id.get());
                        let end_label = format!("if_end_{}", child_id.get());

                        // Emit TEST instruction: test rax, rax (3 bytes)
                        // Assume condition result is in RAX from prior expression evaluation.
                        let test_id =
                            IrNodeId::new(child_id.get() * 3).expect("branch test instr id");
                        let mut test_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        test_operands.push(Operand::Reg(abi::RAX)); // rax
                        test_operands.push(Operand::Reg(abi::RAX)); // rax

                        let test_inst = Instruction {
                            mnemonic: Mnemonic::Test,
                            operands: test_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                        emission_order: 0,
                        };

                        self.emit_inst(test_id, test_inst);

                        // Emit conditional jump (jz): jump to else-label or end-label if condition is zero
                        let target_label = if else_id.is_some() {
                            &else_label
                        } else {
                            &end_label
                        };
                        let jz_id =
                            IrNodeId::new(child_id.get() * 3 + 1).expect("branch jz instr id");
                        let mut jz_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        jz_operands.push(Operand::LabelRef {
                            name: target_label.clone(),
                            addend: 0,
                        });

                        let jz_inst = Instruction {
                            mnemonic: Mnemonic::Jcc(Cond::Zero),
                            operands: jz_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                        emission_order: 0,
                        };

                        self.emit_inst(jz_id, jz_inst);

                        // Register then_label at current offset.
                        self.state.register_label(then_label);

                        // Emit then_body: recursively process children without emitting ret.
                        // The then_id is an Action or Block node containing statements/expressions.
                        if let Some(then_node) = arena.get(_then_id) {
                            match then_node.kind {
                                IrKind::Action => {
                                    // Then body is an Action block: emit its children recursively
                                    // (without the final ret from emit_block_body).
                                    self.emit_block_body_arm(_then_id, arena, typer);
                                }
                                _ => {
                                    // Single expression in then arm: emit it directly.
                                    if cfg!(debug_assertions) {
                                        eprintln!(
                                            "[emit_block_body] Branch then arm is non-Action: {:?}",
                                            then_node.kind
                                        );
                                    }
                                }
                            }
                        }

                        // If else branch exists, emit jmp to end_label
                        if else_id.is_some() {
                            let jmp_id =
                                IrNodeId::new(child_id.get() * 3 + 2).expect("branch jmp instr id");
                            let mut jmp_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                            jmp_operands.push(Operand::LabelRef {
                                name: end_label.clone(),
                                addend: 0,
                            });

                            let jmp_inst = Instruction {
                                mnemonic: Mnemonic::Jmp,
                                operands: jmp_operands,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                            emission_order: 0,
                            };

                            self.emit_inst(jmp_id, jmp_inst);

                            // Register else_label at current offset.
                            self.state.register_label(else_label);

                            // Emit else_body: recursively process children without emitting ret.
                            if let Some(else_node) = arena.get(else_id.unwrap()) {
                                match else_node.kind {
                                    IrKind::Action => {
                                        // Else body is an Action block: emit its children recursively
                                        // (without the final ret from emit_block_body).
                                        self.emit_block_body_arm(else_id.unwrap(), arena, typer);
                                    }
                                    _ => {
                                        // Single expression in else arm: emit it directly.
                                        if cfg!(debug_assertions) {
                                            eprintln!(
                                                "[emit_block_body] Branch else arm is non-Action: {:?}",
                                                else_node.kind
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        // Register end_label at current offset.
                        self.state.register_label(end_label);

                        // Note: Branch result is expected in RAX from whichever arm executed.
                        // No ret instruction is emitted here — the enclosing function's ret
                        // will consume the value in RAX.
                        // We return early to skip the ret emission below.
                        return;
                    }
                    IrKind::Store => {
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body] Store at index {}", i);
                        }
                        self.dispatch_store(child_id, arena);
                    }
                    IrKind::Match => {
                        // #1129: route Match through visit_match. Statement-position
                        // matches discard their result; a trailing (tail) match should
                        // leave its result in RAX for the enclosing lambda's ret.
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body] Match at index {}", i);
                        }
                        self.visit_match(child_id, arena, typer, TailContext::Discard);
                    }
                    IrKind::App => {
                        // #1191 corrective: dispatch on call_sites metadata FIRST — operator callees
                        // are IrKind::Placeholder, not IrKind::Var, so the older Var-callee guard
                        // dead-branched around them. Mirrors Let-RHS operator gate at lines 255-260.
                        let is_operator = arena.call_sites().get(child_id)
                            .map(|m| is_operator_callee(&m.callee_name))
                            .unwrap_or(false);

                        if is_operator {
                            if i == block_children.len() - 1 {
                                // Tail-position operator App (a + b, x + 1, ...): lower into RAX
                                // via #1181's context-neutral BinOp lowerer.
                                if cfg!(debug_assertions) {
                                    eprintln!("[emit_block_body] operator App (tail) at index {}", i);
                                }
                                let _ = self.emit_var_assign_expr_to_rax(child_id, arena);
                            } else if cfg!(debug_assertions) {
                                eprintln!("[emit_block_body] operator App (statement, discarded) at index {}", i);
                            }
                        } else {
                            // Real function call (callee is IrKind::Var with a binding name).
                            // Existing #1183 path preserved.
                            let app_children = arena.children(child_id);
                            if app_children.len() > 0 {
                                let callee_id = app_children[0];
                                if let Some(callee_node) = arena.get(callee_id) {
                                    if callee_node.kind == IrKind::Var {
                                        if let Some(target_name) = arena.binding_names().get(callee_id) {
                                            let lambda_id = IrNodeId::new(self.state.current_function)
                                                .expect("current_function set by walker");
                                            if i == block_children.len() - 1 {
                                                if cfg!(debug_assertions) {
                                                    eprintln!("[emit_block_body] App (tail call) at index {}", i);
                                                }
                                                self.emit_call_expr(lambda_id, target_name.to_string(),
                                                    &app_children[1..], arena);
                                            } else {
                                                if cfg!(debug_assertions) {
                                                    eprintln!("[emit_block_body] App (statement call) at index {}", i);
                                                }
                                                self.emit_call_stmt(lambda_id, target_name.to_string(),
                                                    &app_children[1..], arena);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    IrKind::FieldAccess => {
                        // #1187: module-qualified field-read tail-in-braces `{ M.f }`.
                        // Emit RIP-relative load into RAX at tail position INSIDE the
                        // enclosing lambda's Action arm — pending_first_instr_lambda captures
                        // this instruction as lambda_first_instr[L], keeping the load bytes
                        // inside the function symbol range. Statement-position FA is inert
                        // (mirrors emit_action_stmt's IrKind::FieldAccess arm).
                        if i == block_children.len() - 1 {
                            if let Some(field_name) = arena.module_field_refs().get(child_id) {
                                let name_owned = field_name.to_string();
                                self.emit_module_field_read(child_id, abi::RAX, name_owned);
                            }
                            // Struct-typed FA at tail: deferred (no fixture exercises it today).
                        } else if cfg!(debug_assertions) {
                            eprintln!(
                                "[emit_block_body] FieldAccess at index {} (statement position, skipped)",
                                i
                            );
                        }
                    }
                    _ => {
                        // Unexpected statement kind.
                        if cfg!(debug_assertions) {
                            eprintln!(
                                "[emit_block_body] Unexpected child kind: {:?}",
                                child_node.kind
                            );
                        }
                    }
                }
            }
        }

        // For now, emit a simple ret instruction at the end.
        // The final expression should be in RAX before this.
        let ret_inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        emission_order: 0,
        };
        let ret_id = IrNodeId::new(block_id.get() * 2).expect("ret virtual id");
        self.emit_inst(ret_id, ret_inst);
    }

    /// PA8-m2-001: Emit block body for branch arm (same as emit_block_body but WITHOUT final ret).
    ///
    /// Used when a Branch node appears as the final expression in a block.
    /// This helper emits the arm's statements/expressions but suppresses the final ret,
    /// allowing the enclosing block's ret to consume the arm's result in RAX.
    pub(crate) fn emit_block_body_arm(
        &mut self,
        block_id: IrNodeId,
        arena: &IrArena,
        typer: Option<&paideia_as_types::TypeInterner>,
    ) {
        // PA10-005 §3.2: Push scope on entry to nested block arm.
        self.state.local_bindings.push_scope();

        let block_children = arena.children(block_id);
        if cfg!(debug_assertions) {
            eprintln!(
                "[emit_block_body_arm] Block {} has {} children",
                block_id.get(),
                block_children.len()
            );
        }

        // Scratch register sequence for in-block let bindings.
        // Exclude RAX since it's clobbered by function calls (used for return values).
        // Use RCX, RDX, R8, R9 instead to avoid conflicts with call results.
        let scratch_regs = [abi::RCX, abi::RDX, abi::R8, abi::R9]; // RCX, RDX, R8, R9

        // Walk all children: statements + optional tail.
        for (i, &child_id) in block_children.iter().enumerate() {
            if let Some(child_node) = arena.get(child_id) {
                match child_node.kind {
                    IrKind::Let => {
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body_arm] Let statement at index {}", i);
                        }
                        // This is a let binding. Emit the value expression.
                        // Statement-level Let children: [name_var, value, ty?], RHS at index 1.
                        // Direct allocations (unit tests): [value], RHS at index 0.
                        let let_children = arena.children(child_id);
                        let rhs_idx = if let_children.len() > 1 { 1 } else { 0 };
                        if let Some(&rhs_id) = let_children.get(rhs_idx) {
                            if let Some(rhs_node) = arena.get(rhs_id) {
                                // Assign next scratch register if available.
                                if self.state.scratch_count() >= scratch_regs.len() {
                                    // Register pressure exceeded.
                                    self.push_typed_diag(
                                        t0527_code(),
                                        format!(
                                            "register pressure exceeded in Phase 7 Let-literal bindings: more than {} in-flight bindings",
                                            scratch_regs.len()
                                        ),
                                    );
                                    // PA10-005 §3.2: Pop scope before early return
                                    self.state.local_bindings.pop_scope();
                                    return;
                                }

                                let scratch_reg = scratch_regs[self.state.scratch_count()];
                                self.state.assign_scratch(scratch_reg);

                                // Get binding name from arena.binding_names()
                                let binding_name = arena
                                    .binding_names()
                                    .get(child_id)
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| format!("_let_{}", child_id.get()));

                                // Edit A: Handle Literal RHS
                                if rhs_node.kind == IrKind::Literal {
                                    if let Some(value) = arena.literal_values().get(rhs_id) {
                                        // Allocate scratch register and emit mov instruction
                                        self.state
                                            .local_bindings
                                            .insert(binding_name.clone(), scratch_reg);

                                        // PA8-m3-001: (Reg, Imm64) move with a recoverable Let
                                        // width — width-route to MovSized exactly as the main
                                        // block-body path does.
                                        let width = typer.and_then(|typer| {
                                            Self::resolve_let_width(arena, child_id, typer)
                                        });

                                        // Emit: mov scratch_reg, imm64 (or MovSized for sub-64-bit).
                                        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                        operands.push(Operand::Reg(scratch_reg));
                                        operands.push(Operand::Imm64(value));

                                        let (mnemonic, _inst_size) = match width {
                                            Some(
                                                w @ (IntWidth::W8 | IntWidth::W16 | IntWidth::W32),
                                            ) => (
                                                Mnemonic::MovSized { width: w },
                                                w.estimated_size(),
                                            ),
                                            _ => {
                                                // Generic 64-bit Mov: i32 → 7 bytes, i64 → 10.
                                                let size = if value >= i32::MIN as i64
                                                    && value <= i32::MAX as i64
                                                {
                                                    7
                                                } else {
                                                    10
                                                };
                                                (Mnemonic::Mov, size)
                                            }
                                        };

                                        let inst = Instruction {
                                            mnemonic,
                                            operands,
                                            encoding_hint: None,
                                            byte_offset_in_text: None,
                                            mode: self.current_mode(),
                                        emission_order: 0,
                                        };

                                        // Use virtual ID: child_id * 3 + offset to ensure proper sorting
                                        let inst_id = IrNodeId::new(child_id.get() * 3)
                                            .expect("let literal instr id");
                                        self.emit_inst(inst_id, inst);
                                    }
                                }
                                // Edit B: Handle Unsafe RHS
                                else if matches!(rhs_node.kind, IrKind::Unsafe { .. }) {
                                    // Record binding in local_bindings but don't emit instruction
                                    // UnsafeWalker will handle the body via existing pending queue
                                    self.state
                                        .local_bindings
                                        .insert(binding_name.clone(), scratch_reg);
                                }
                                // Edit C: Handle RawInstruction RHS (future lowering placeholder)
                                else if rhs_node.kind == IrKind::RawInstruction {
                                    if let Some(inst) = arena.instructions().get(rhs_id) {
                                        // Check if this is a value-producing Mov instruction
                                        if inst.mnemonic == Mnemonic::Mov {
                                            // PA8-m3-001 (not width-routed): cloned from a
                                            // pre-lowered RawInstruction; only the destination is
                                            // rewritten. Operand shape is fixed upstream and may
                                            // not be MovSized-encodable, so the mnemonic is kept.
                                            let mut cloned = inst.clone();
                                            if let Some(first_op) = cloned.operands.get_mut(0) {
                                                *first_op = Operand::Reg(scratch_reg);
                                            }

                                            self.state
                                                .local_bindings
                                                .insert(binding_name.clone(), scratch_reg);

                                            self.emit_inst(rhs_id, cloned);
                                        }
                                    }
                                }
                                // Edit D: Handle App RHS (function calls and operators) - #1162/#1191 (mirror of emit_block_body)
                                else if rhs_node.kind == IrKind::App {
                                    if let Some(meta) = arena.call_sites().get(rhs_id) {
                                        // #1191 corrective: check if this is an operator or function call
                                        if is_operator_callee(&meta.callee_name) {
                                            // Operator App at let-RHS in match arm: emit the binary operation into scratch_reg
                                            // Register binding FIRST so emit has access
                                            self.state
                                                .local_bindings
                                                .insert(binding_name.clone(), scratch_reg);
                                            // Emit operator into scratch_reg (mirrors tail-App dispatch)
                                            let _ = self.emit_var_assign_expr_to_reg(rhs_id, arena, scratch_reg, 0);
                                        } else {
                                            // Real function call (callee is not an operator)
                                            let app_children = arena.children(rhs_id);
                                            let arg_ids: Vec<IrNodeId> = app_children[1..].to_vec();
                                            // Use state.current_function (the enclosing lambda's id),
                                            // NOT child_id (the Let node id).
                                            let lambda_id = IrNodeId::new(self.state.current_function)
                                                .expect("current_function set by walker");
                                            self.emit_call_expr(lambda_id, meta.callee_name.clone(), &arg_ids, arena);
                                            if scratch_reg != abi::RAX {
                                                // mov scratch_reg, rax — materialize the CALL result.
                                                let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                                ops.push(Operand::Reg(scratch_reg));
                                                ops.push(Operand::Reg(abi::RAX));
                                                let inst = Instruction {
                                                    mnemonic: Mnemonic::Mov,
                                                    operands: ops,
                                                    encoding_hint: None,
                                                    byte_offset_in_text: None,
                                                    mode: self.current_mode(),
                                                    emission_order: 0,
                                                };
                                                let inst_id = IrNodeId::new(1_200_000 + child_id.get())
                                                    .expect("let-app materialize id");
                                                self.emit_inst(inst_id, inst);
                                            }
                                            self.state.local_bindings.insert(binding_name.clone(), scratch_reg);
                                        }
                                    }
                                }
                                else if rhs_node.kind == IrKind::BitNot {
                                    // #1194: Handle BitNot RHS (~expr) — route through #1181 lowerer to emit
                                    // mov dest, operand ; not dest. Without this, the catch-all at #1138
                                    // records the binding but never emits the operation.
                                    self.state
                                        .local_bindings
                                        .insert(binding_name.clone(), scratch_reg);
                                    let _ = self.emit_var_assign_expr_to_reg(rhs_id, arena, scratch_reg, 0);
                                }
                                // #1138: Handle other RHS kinds (e.g., Var) by just recording binding
                                // without emitting instructions. Instruction emission is deferred or N/A.
                                else {
                                    self.state
                                        .local_bindings
                                        .insert(binding_name.clone(), scratch_reg);
                                }

                                if cfg!(debug_assertions) {
                                    eprintln!(
                                        "[emit_block_body_arm] Let binding {} uses scratch reg {:?}",
                                        binding_name, scratch_reg
                                    );
                                }
                            }
                        }
                    }
                    IrKind::Action => {
                        // This is a StmtExpr (statement expression). Emit it and discard result.
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body_arm] StmtExpr at index {}", i);
                        }
                        self.emit_action_stmt(child_id, arena, typer);
                    }
                    IrKind::RawInstruction => {
                        // Phase 7 m2-001 (PA7C-m2-001): RawInstruction child of Action.
                        // Look up the instruction payload in the side-table.
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body_arm] RawInstruction at index {}", i);
                        }
                        if let Some(inst) = arena.instructions().get(child_id) {
                            self.emit_inst(child_id, inst.clone());
                        } else {
                            // #1147 A3: invariant violation — RawInstruction lacks side-table payload (arm variant).
                            self.push_typed_diag(
                                u1642_code(),
                                format!(
                                    "Instruction payload not found in side-table for RawInstruction node {} (internal compiler error)",
                                    child_id.get()
                                ),
                            );
                        }
                    }
                    IrKind::Var => {
                        // Phase 7 m2-003: Bare identifier in statement or final-expression position.
                        // #1188: If this is the final expression (last child), move its value to RAX.
                        // Otherwise it's a statement-form variable reference with no side effects.
                        if i == block_children.len() - 1 {
                            // Tail position in match arm: value in RAX becomes the arm's value.
                            if cfg!(debug_assertions) {
                                eprintln!(
                                    "[emit_block_body_arm] Var (tail position) at index {} — moving to RAX",
                                    i
                                );
                            }

                            // Look up the variable's current register
                            if let Some(var_name) = arena.binding_names().get(child_id) {
                                if let Some(src_reg) = self.state.local_bindings.get(var_name) {
                                    if src_reg != abi::RAX {
                                        // Emit: mov rax, src_reg
                                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                        ops.push(Operand::Reg(abi::RAX));
                                        ops.push(Operand::Reg(src_reg));
                                        let inst = Instruction {
                                            mnemonic: Mnemonic::Mov,
                                            operands: ops,
                                            encoding_hint: None,
                                            byte_offset_in_text: None,
                                            mode: self.current_mode(),
                                            emission_order: 0,
                                        };
                                        let inst_id = IrNodeId::new(child_id.get() * 3 + 2)
                                            .expect("arm tail var mov id");
                                        self.emit_inst(inst_id, inst);
                                    }
                                }
                            }
                        } else {
                            // Statement-form variable reference with no side effects
                            if cfg!(debug_assertions) {
                                eprintln!(
                                    "[emit_block_body_arm] Var (bare identifier) at index {} — skipped",
                                    i
                                );
                            }
                        }
                    }
                    IrKind::Store => {
                        // #1115: mirror emit_block_body's Store dispatch so `match x { A => y.f = z }`
                        // doesn't ghost-drop the write. Uses the same three-way helper.
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body_arm] Store at index {}", i);
                        }
                        self.dispatch_store(child_id, arena);
                    }
                    IrKind::Match => {
                        // #1129: mirror emit_block_body's Match dispatch so nested matches
                        // (match arm containing another match) don't ghost-drop.
                        if cfg!(debug_assertions) {
                            eprintln!("[emit_block_body_arm] Match at index {}", i);
                        }
                        self.visit_match(child_id, arena, typer, TailContext::Discard);
                    }
                    IrKind::App => {
                        // #1191 corrective: dispatch on call_sites metadata FIRST — operator callees
                        // are IrKind::Placeholder, not IrKind::Var, so the older Var-callee guard
                        // dead-branched around them. Mirrors Let-RHS operator gate at lines 255-260.
                        let is_operator = arena.call_sites().get(child_id)
                            .map(|m| is_operator_callee(&m.callee_name))
                            .unwrap_or(false);

                        if is_operator {
                            if i == block_children.len() - 1 {
                                // Tail-position operator App (a + b, x + 1, ...): lower into RAX
                                // via #1181's context-neutral BinOp lowerer.
                                if cfg!(debug_assertions) {
                                    eprintln!("[emit_block_body_arm] operator App (tail) at index {}", i);
                                }
                                let _ = self.emit_var_assign_expr_to_rax(child_id, arena);
                            } else if cfg!(debug_assertions) {
                                eprintln!("[emit_block_body_arm] operator App (statement, discarded) at index {}", i);
                            }
                        } else {
                            // Real function call (callee is IrKind::Var with a binding name).
                            // Existing #1183 path preserved.
                            let app_children = arena.children(child_id);
                            if app_children.len() > 0 {
                                let callee_id = app_children[0];
                                if let Some(callee_node) = arena.get(callee_id) {
                                    if callee_node.kind == IrKind::Var {
                                        if let Some(target_name) = arena.binding_names().get(callee_id) {
                                            let lambda_id = IrNodeId::new(self.state.current_function)
                                                .expect("current_function set by walker");
                                            if i == block_children.len() - 1 {
                                                if cfg!(debug_assertions) {
                                                    eprintln!("[emit_block_body_arm] App (tail call) at index {}", i);
                                                }
                                                self.emit_call_expr(lambda_id, target_name.to_string(),
                                                    &app_children[1..], arena);
                                            } else {
                                                if cfg!(debug_assertions) {
                                                    eprintln!("[emit_block_body_arm] App (statement call) at index {}", i);
                                                }
                                                self.emit_call_stmt(lambda_id, target_name.to_string(),
                                                    &app_children[1..], arena);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    IrKind::FieldAccess => {
                        // #1189: module-qualified field-read tail-in-braces inside a match arm.
                        // Mirror of the #1187 arm in emit_block_body: at tail position, RIP-relative
                        // load into RAX via emit_module_field_read. Statement-position FA is inert
                        // (matches emit_action_stmt::IrKind::FieldAccess).
                        if i == block_children.len() - 1 {
                            if let Some(field_name) = arena.module_field_refs().get(child_id) {
                                let name_owned = field_name.to_string();
                                self.emit_module_field_read(child_id, abi::RAX, name_owned);
                            }
                            // Struct-typed FA at tail: deferred (no fixture exercises it today).
                        } else if cfg!(debug_assertions) {
                            eprintln!(
                                "[emit_block_body_arm] FieldAccess at index {} (statement position, skipped)",
                                i
                            );
                        }
                    }
                    _ => {
                        // Unexpected statement kind.
                        if cfg!(debug_assertions) {
                            eprintln!(
                                "[emit_block_body_arm] Unexpected child kind: {:?}",
                                child_node.kind
                            );
                        }
                    }
                }
            }
        }

        // PA10-005 §3.2: Pop scope on exit from nested block arm.
        // Debug-assert to verify scope depth is correctly maintained.
        if cfg!(debug_assertions) {
            // Scope depth should be >= 2 at exit (root + current arm)
            eprintln!(
                "[emit_block_body_arm] Scope depth before pop: {}",
                self.state.local_bindings.scopes_len()
            );
        }
        self.state.local_bindings.pop_scope();

        // Note: NO ret instruction is emitted here — that's left to the caller.
    }

    /// PA-r17-013 (#991): Emit a tail expression in the proper context.
    ///
    /// For trailing expressions in match arms and lambda bodies, this ensures
    /// the result lands in RAX (or RAX:RDX / [RDI+disp] per the tail context).
    ///
    /// Handles:
    /// - Literal: emit Mov [tail_reg], Imm64(v)
    /// - Var: emit Mov RAX, <var_reg>
    /// - EnumCons: recurse via visit_enum_cons (writes RAX/RDX or [RDI+disp])
    /// - Match: recurse via visit_match with tail context
    /// - Branch: recurse into arms with tail propagation
    #[allow(dead_code)]
    pub(crate) fn emit_tail_expr(
        &mut self,
        tail_id: IrNodeId,
        arena: &IrArena,
        typer: Option<&paideia_as_types::TypeInterner>,
        tail: TailContext,
    ) {
        if let Some(node) = arena.get(tail_id) {
            match node.kind {
                IrKind::Literal => {
                    if let Some(value) = arena.literal_values().get(tail_id) {
                        match tail {
                            TailContext::ReturnRax => {
                                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                operands.push(Operand::Reg(abi::RAX));
                                operands.push(Operand::Imm64(value));
                                let inst = Instruction {
                                    mnemonic: Mnemonic::Mov,
                                    operands,
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                emission_order: 0,
                                };
                                self.emit_inst(tail_id, inst);
                            }
                            TailContext::ReturnRaxRdx => {
                                // For small enum (≤16 bytes), put discriminant in RAX
                                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                operands.push(Operand::Reg(abi::RAX));
                                operands.push(Operand::Imm64(value));
                                let inst = Instruction {
                                    mnemonic: Mnemonic::Mov,
                                    operands,
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                emission_order: 0,
                                };
                                self.emit_inst(tail_id, inst);
                            }
                            TailContext::ReturnIndirect { disc_size: _ } => {
                                // For large enum (>16 bytes), write to [RDI+0] for discriminant
                                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                operands.push(Operand::MemSib {
                                    base: abi::RDI,
                                    index: None,
                                    scale: paideia_as_ir::instruction::Scale::X1,
                                    disp: 0,
                                });
                                operands.push(Operand::Imm64(value));
                                let inst = Instruction {
                                    mnemonic: Mnemonic::Mov,
                                    operands,
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                emission_order: 0,
                                };
                                self.emit_inst(tail_id, inst);
                            }
                            TailContext::Discard => {
                                // Discarded: emit mov rax, imm (standard path)
                                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                operands.push(Operand::Reg(abi::RAX));
                                operands.push(Operand::Imm64(value));
                                let inst = Instruction {
                                    mnemonic: Mnemonic::Mov,
                                    operands,
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                emission_order: 0,
                                };
                                self.emit_inst(tail_id, inst);
                            }
                        }
                    }
                }
                IrKind::Var => {
                    // Load variable from its binding and move to RAX
                    if let Some(var_name) = arena.binding_names().get(tail_id) {
                        if let Some(var_reg) = self.state.local_bindings.get(var_name) {
                            // Move from var_reg to RAX
                            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                            operands.push(Operand::Reg(abi::RAX));
                            operands.push(Operand::Reg(var_reg));
                            let inst = Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                            emission_order: 0,
                            };
                            self.emit_inst(tail_id, inst);
                        }
                    }
                }
                IrKind::EnumCons => {
                    // Delegate to visit_enum_cons which already handles RAX/RDX or [RDI+disp]
                    self.visit_enum_cons(tail_id, arena);
                }
                IrKind::Match => {
                    // Recurse via visit_match with tail context
                    self.visit_match(tail_id, arena, typer, tail);
                    self.state.mark_match_emitted(tail_id.get());
                }
                IrKind::Branch => {
                    // Recurse into branch with tail context (not implemented yet, deferred)
                    // For now, just skip to avoid panics
                }
                _ => {
                    // Other expression kinds deferred
                }
            }
        }
    }

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
