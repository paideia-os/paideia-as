//! `emit_block_body`: statement-walk driver for the Action body of the
//! enclosing Lambda (function level). Handles `Let` / `StmtExpr` /
//! `RawInstruction` children, tail-position `Var` / `App` / `Branch` /
//! `FieldAccess`, and emits a trailing `ret`.
//!
//! Extracted verbatim from the pre-split `emit_block_body.rs`
//! (issue #1410). No behavior change.

use paideia_as_ir::instruction::{Cond, Instruction, IntWidth, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi, PassingConvention};

use crate::emit_walker::EmitWalker;
use crate::emit_store_record::is_operator_callee;

use super::TailContext;
use super::diagnostics::{t0527_code, t0559_code, u1621_code, u1642_code, u1659_code};

impl EmitWalker {
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
                                            "register pressure exceeded in Let-literal bindings: more than {} in-flight bindings",
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
                                            // #1230: Fail loud if an operator somehow bypassed filter.
                                            debug_assert!(
                                                !paideia_as_ir::is_operator(&meta.callee_name),
                                                "operator {} fell through to function-call path at site P",
                                                meta.callee_name
                                            );
                                            // #1178: Check if return value is a register-form enum with payload.
                                            let pair_layout = Self::resolve_let_enum_layout(arena, child_id)
                                                .filter(|l| l.passing_convention() == PassingConvention::RegisterPair
                                                            && l.payload_size > 0);

                                            // Reserve payload_reg from the SAME [RCX,RDX,R8,R9] scratch pool
                                            // (do NOT introduce an R10 sub-pool — softarch scope rule).
                                            // Only when pair_layout is Some.
                                            let payload_reg = if pair_layout.is_some() {
                                                // Same allocator as scratch_reg — increment scratch_count.
                                                let idx = self.state.scratch_count();
                                                if idx >= scratch_regs.len() {
                                                    self.push_typed_diag(t0527_code(), format!(
                                                        "let-App RHS scratch-pool exhausted (enum pair) for '{}'",
                                                        binding_name
                                                    ));
                                                    return;
                                                }
                                                let reg = scratch_regs[idx];
                                                self.state.assign_scratch(reg);
                                                Some(reg)
                                            } else {
                                                None
                                            };

                                            let app_children = arena.children(rhs_id);
                                            let arg_ids: Vec<IrNodeId> = app_children[1..].to_vec();
                                            // Use state.current_function (the enclosing lambda's id),
                                            // NOT child_id (the Let node id).
                                            let lambda_id = IrNodeId::new(self.state.current_function)
                                                .expect("current_function set by walker");
                                            self.emit_call_expr(lambda_id, meta.callee_name.clone(), &arg_ids, arena);

                                            // #1178 CRITICAL: emit payload capture FIRST (only if pair path AND payload_reg != RDX).
                                            if let Some(preg) = payload_reg {
                                                if preg != abi::RDX {
                                                    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                                    ops.push(Operand::Reg(preg));
                                                    ops.push(Operand::Reg(abi::RDX));
                                                    let inst = Instruction {
                                                        mnemonic: Mnemonic::Mov,
                                                        operands: ops,
                                                        encoding_hint: None,
                                                        byte_offset_in_text: None,
                                                        mode: self.current_mode(),
                                                        emission_order: 0,
                                                    };
                                                    let payload_inst_id = IrNodeId::new(1_210_000 + child_id.get())
                                                        .expect("let-app payload capture id");
                                                    self.emit_inst(payload_inst_id, inst);
                                                }
                                            }

                                            // Existing mov scratch_reg, rax block (unchanged).
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

                                            // Registration.
                                            if let Some(preg) = payload_reg {
                                                self.state.local_bindings.insert_pair(binding_name.clone(), scratch_reg, preg);
                                            } else {
                                                self.state.local_bindings.insert(binding_name.clone(), scratch_reg);
                                            }
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
                                // Part A: #1209 — Handle Var RHS: `let y = x` copies source register to scratch
                                else if rhs_node.kind == IrKind::Var {
                                    self.state
                                        .local_bindings
                                        .insert(binding_name.clone(), scratch_reg);
                                    let _ = self.emit_var_assign_expr_to_reg(rhs_id, arena, scratch_reg, 0);
                                }
                                // Part B: #1207 — Handle Match RHS: `let x = match ... { ... }`
                                else if rhs_node.kind == IrKind::Match {
                                    self.state
                                        .local_bindings
                                        .insert(binding_name.clone(), scratch_reg);
                                    self.visit_match(rhs_id, arena, typer, TailContext::ReturnRax);
                                    self.state.mark_match_emitted(rhs_id.get());
                                    if scratch_reg != abi::RAX {
                                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                        ops.push(Operand::Reg(scratch_reg));
                                        ops.push(Operand::Reg(abi::RAX));
                                        self.emit_inst(
                                            IrNodeId::new(1_300_000 + child_id.get()).expect("let-match materialize id"),
                                            Instruction {
                                                mnemonic: Mnemonic::Mov,
                                                operands: ops,
                                                encoding_hint: None,
                                                byte_offset_in_text: None,
                                                mode: self.current_mode(),
                                                emission_order: 0,
                                            },
                                        );
                                    }
                                }
                                // Part D: #1206 / #1209 / #1213 — Handle EnumCons RHS
                                else if rhs_node.kind == IrKind::EnumCons {
                                    // `let x : Enum = Enum::Variant` — load variant discriminant (and payload if present).
                                    let info = arena.enum_cons_info().get(rhs_id)
                                        .expect("EnumCons node must have EnumConsInfo");
                                    if !arena.children(rhs_id).is_empty() {
                                        // #1213: payload-carrying variant — allocate paired scratch register.
                                        if self.state.scratch_count() >= scratch_regs.len() {
                                            self.push_typed_diag(t0527_code(), format!(
                                                "register pressure exceeded: payload enum needs 2nd scratch (variant {})",
                                                info.variant_index));
                                            return;
                                        }
                                        let payload_reg = scratch_regs[self.state.scratch_count()];
                                        self.state.assign_scratch(payload_reg);

                                        // Rewire insert → insert_pair (primary reg stays scratch_reg for parity).
                                        self.state.local_bindings.insert_pair(
                                            binding_name.clone(), scratch_reg, payload_reg);

                                        // Emit discriminant load.
                                        self.emit_mov_literal_to_reg_with_id(
                                            IrNodeId::new(1_310_000 + child_id.get()).expect("let-enum disc id"),
                                            scratch_reg,
                                            info.variant_index as i64,
                                        );

                                        // Emit payload load. Support Literal + Var children (mirror visit_enum_cons
                                        // producer path at emit_enum_match.rs:385-406).
                                        let payload_child_id = arena.children(rhs_id)[0];
                                        match arena.get(payload_child_id).map(|n| n.kind) {
                                            Some(IrKind::Literal) => {
                                                let val = arena.literal_values().get(payload_child_id).unwrap_or(0);
                                                self.emit_mov_literal_to_reg_with_id(
                                                    IrNodeId::new(1_320_000 + child_id.get()).expect("let-enum payload id"),
                                                    payload_reg,
                                                    val,
                                                );
                                            }
                                            Some(IrKind::Var) => {
                                                let _ = self.emit_var_assign_expr_to_reg(payload_child_id, arena, payload_reg, 0);
                                            }
                                            _ => {
                                                self.push_typed_diag(t0559_code(), format!(
                                                    "payload child kind {:?} not supported at let-RHS (only Literal/Var)",
                                                    arena.get(payload_child_id).map(|n| n.kind)));
                                                return;
                                            }
                                        }
                                        self.state.mark_enum_cons_handled(rhs_id.get());
                                    } else {
                                        // Unit variant — emit mov scratch_reg, discriminant_value
                                        self.state
                                            .local_bindings
                                            .insert(binding_name.clone(), scratch_reg);
                                        self.emit_mov_literal_to_reg_with_id(
                                            IrNodeId::new(1_310_000 + child_id.get()).expect("let-enum materialize id"),
                                            scratch_reg,
                                            info.variant_index as i64,
                                        );
                                        self.state.mark_enum_cons_handled(rhs_id.get());
                                    }
                                }
                                // Part E: #1233 — Handle ClosureCons RHS (closure literal)
                                else if rhs_node.kind == IrKind::ClosureCons {
                                    // `let f = <closure>` — emit fat pointer to stack.
                                    // emit_closure_cons always leaves the fat-pointer address
                                    // in RAX (`lea rax, [rsp + fat_off]`); if this binding's
                                    // assigned scratch register is something else, move it over
                                    // (mirrors the analogous fixup in the Match-RHS arm above).
                                    self.emit_closure_cons(rhs_id, arena);
                                    // #995: Register as Closure binding (not scalar) for proper dispatch.
                                    self.state.local_bindings.insert_closure(binding_name.clone(), scratch_reg);
                                    if scratch_reg != abi::RAX {
                                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                        ops.push(Operand::Reg(scratch_reg));
                                        ops.push(Operand::Reg(abi::RAX));
                                        self.emit_inst(
                                            IrNodeId::new(1_330_000 + child_id.get())
                                                .expect("let-closure-cons materialize id"),
                                            Instruction {
                                                mnemonic: Mnemonic::Mov,
                                                operands: ops,
                                                encoding_hint: None,
                                                byte_offset_in_text: None,
                                                mode: self.current_mode(),
                                                emission_order: 0,
                                            },
                                        );
                                    }
                                }
                                // Part C: #1209/#1207 hardening — convert catch-all to typed diagnostic
                                else {
                                    self.state
                                        .local_bindings
                                        .insert(binding_name.clone(), scratch_reg);
                                    self.push_typed_diag(
                                        u1659_code(),
                                        format!(
                                            "unhandled Let-RHS kind {:?} at node {} — binding {} would be uninitialized",
                                            rhs_node.kind, rhs_id.get(), binding_name
                                        ),
                                    );
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

                                            // #995: Check for closure-typed binding BEFORE scalar function pointer.
                                            use crate::local_binding_table::BindingHome;
                                            if let Some(BindingHome::Closure(closure_reg)) = self.state.local_bindings.get_home(target_name) {
                                                // Closure call - handle both tail and statement positions
                                                if cfg!(debug_assertions) {
                                                    eprintln!("[emit_block_body] App (closure {} call) at index {}",
                                                        if i == block_children.len() - 1 { "tail" } else { "statement" }, i);
                                                }
                                                self.emit_closure_call(lambda_id, closure_reg, &app_children[1..], arena);
                                                // Tail-position closure returns the value in RAX
                                                // Statement-position closure discards the result
                                            } else if i == block_children.len() - 1 {
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
        let ret_id = IrNodeId::new(block_id.get() * 2).expect("ret virtual id");
        self.emit_ret(ret_id, arena);
    }
}
