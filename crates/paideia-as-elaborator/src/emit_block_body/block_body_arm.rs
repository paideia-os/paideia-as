//! `emit_block_body_arm`: mirror of `emit_block_body` used inside a match
//! arm body. Same statement-walk shape as the outer walker, but pushes a
//! local scope on entry and pops on exit (or on any early return), and
//! emits no trailing `ret` — the arm's value is left in RAX for the
//! enclosing lambda's return.
//!
//! Extracted verbatim from the pre-split `emit_block_body.rs`
//! (issue #1410). No behavior change.

use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi, PassingConvention};

use crate::emit_walker::EmitWalker;
use crate::emit_store_record::is_operator_callee;

use super::TailContext;
use super::diagnostics::{t0527_code, t0559_code, u1642_code, u1659_code};

impl EmitWalker {
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
                                            "register pressure exceeded in Let-literal bindings: more than {} in-flight bindings",
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
                                                    self.state.local_bindings.pop_scope();
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
                                                    let payload_inst_id = IrNodeId::new(1_211_000 + child_id.get())
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
                                // Part A: #1209 — Handle Var RHS: `let y = x` copies source register to scratch
                                else if rhs_node.kind == IrKind::Var {
                                    self.state
                                        .local_bindings
                                        .insert(binding_name.clone(), scratch_reg);
                                    let _ = self.emit_var_assign_expr_to_reg(rhs_id, arena, scratch_reg, 0);
                                }
                                // Part B: #1207 — Handle Match RHS: `let x = match ... { ... }` in match arms
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
                                            self.state.local_bindings.pop_scope();
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
                                                self.state.local_bindings.pop_scope();
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

                                            // #995: Check for closure-typed binding BEFORE scalar function pointer.
                                            use crate::local_binding_table::BindingHome;
                                            if let Some(BindingHome::Closure(closure_reg)) = self.state.local_bindings.get_home(target_name) {
                                                // Closure call - handle both tail and statement positions in arm
                                                if cfg!(debug_assertions) {
                                                    eprintln!("[emit_block_body_arm] App (closure {} call) at index {}",
                                                        if i == block_children.len() - 1 { "tail" } else { "statement" }, i);
                                                }
                                                self.emit_closure_call(lambda_id, closure_reg, &app_children[1..], arena);
                                                // Tail-position closure in arm returns value for outer block
                                                // Statement-position closure discards result
                                            } else if i == block_children.len() - 1 {
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
}
