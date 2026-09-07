//! Cmp/jne cascade match dispatch (fallback path) + guard expression
//! evaluation.
//!
//! - `visit_match` (Phase 7 m1-004 / PA7-007): the main match entry point.
//!   Routes integer scrutinees to `visit_int_match`, dense enum scrutinees
//!   to `visit_match_jump_table`, and everything else through the cmp/jne
//!   cascade lowered here. Handles single-alt and multi-alt (or-pattern)
//!   arm dispatch, guards (#1000), and both stack- and register-form
//!   pattern binding.
//! - `emit_guard_expression` (#1000): evaluates a guard expression into
//!   RAX for the subsequent `test rax, rax` / `jz next_label` sequence.
//!
//! Extracted verbatim from the pre-split `emit_enum_match.rs` (issue #1409).

use paideia_as_ir::instruction::{Cond, Instruction, Mnemonic, Operand};
use paideia_as_ir::{abi, IrArena, IrKind, IrNodeId, SmallVec};

use crate::emit_block_body::TailContext;
use crate::emit_walker::EmitWalker;

use super::diagnostics::{t0556_code, t0560_code, u1649_code, u1650_code, u1651_code};

impl EmitWalker {
    /// Phase 7 m1-004 (PA7-007): Emit match-expression lowering for enum variant dispatch.
    ///
    /// Lowers `match value { Ok(x) => ..., Err(y) => ..., _ => ... }` to:
    /// - discriminant load (if stack form)
    /// - cmp rax, variant_0; jne arm_1_label
    /// - payload load for arm_0 (if needed)
    /// - arm_0 body; jmp end
    /// - arm_1_label: cmp rax, variant_1; jne default_label
    /// - payload load for arm_1 (if needed)
    /// - arm_1 body; jmp end
    /// - default_label: default body
    /// - end_label:
    ///
    /// Register convention (mirrors visit_enum_cons):
    /// - Register form (≤16 bytes): discriminant in RAX, payload in RDX
    /// - Stack form (>16 bytes): scrutinee pointer in RDI, load disc from [rdi+0]
    ///
    /// Structure: Match has children [scrutinee, arm0, arm1, ...].
    ///
    /// PA-r17-013 (#991): tail parameter specifies where arm body results should land.
    /// When in ReturnRax/ReturnRaxRdx/ReturnIndirect, arm bodies propagate this context
    /// so their expressions land in the proper return location without intermediate RAX.
    pub(crate) fn visit_match(
        &mut self,
        match_node_id: IrNodeId,
        arena: &IrArena,
        typer: Option<&paideia_as_types::TypeInterner>,
        _tail: TailContext,
    ) {
        // Mark this match as emitted in tail position
        self.state.mark_match_emitted(match_node_id.get());

        // Issue #1210: Intercept integer-scrutinee matches and route to visit_int_match
        if let Some(int_meta) = arena.int_match_scrutinee().get(match_node_id).copied() {
            return self.visit_int_match(match_node_id, arena, typer, _tail, int_meta);
        }

        let children = arena.children(match_node_id);
        if children.is_empty() {
            self.push_typed_diag(u1650_code(), format!(
                "Match node {} has no children; expected scrutinee + arms",
                match_node_id.get()
            ));
            return;
        }

        // PA-r15-009b (#1032): Check for jump-table dispatch
        if let Some(dispatch_meta) = arena.match_dispatch_meta().get(match_node_id) {
            if dispatch_meta.jump_table && dispatch_meta.density_ok {
                return self.visit_match_jump_table(match_node_id, arena, dispatch_meta);
            }
        }

        let scrutinee_id = children[0];
        let arm_ids: Vec<IrNodeId> = children[1..].to_vec();

        if arm_ids.is_empty() {
            self.push_typed_diag(u1650_code(), format!(
                "Match node {} has scrutinee but no arms",
                match_node_id.get()
            ));
            return;
        }

        // Early check: if scrutinee is not a Var, fire T0556
        if let Some(scrutinee_node) = arena.get(scrutinee_id) {
            if scrutinee_node.kind != IrKind::Var {
                self.push_typed_diag(
                    t0556_code(),
                    format!(
                        "match scrutinee must be a Var binding — bind the value to a `let` first (e.g., `let x = {}; match x {{ ... }}`)",
                        match scrutinee_node.kind {
                            IrKind::App => "f(y)",
                            _ => "expr",
                        }
                    ),
                );
                return;
            }
        }

        // Read enum type from scrutinee table
        let enum_type_id = match arena.match_scrutinee_table().get(match_node_id) {
            Some(tid) => *tid,
            None => {
                self.push_typed_diag(u1650_code(), format!(
                    "Match node {} has no scrutinee type",
                    match_node_id.get()
                ));
                return;
            }
        };

        // Look up layout and extract needed fields
        let layout = match self.state.enum_layout(enum_type_id) {
            Some(l) => l.clone(),
            None => {
                self.push_typed_diag(u1649_code(), format!(
                    "No enum layout found for match type {}",
                    enum_type_id.0
                ));
                return;
            }
        };
        let layout_size = layout.size;
        let layout_payload_size = layout.payload_size;

        // #1084: Emit scrutinee load from module symbol or local binding
        self.emit_scrutinee_load(match_node_id, scrutinee_id, &layout, arena);

        // Emit discriminant load for stack form
        if layout_size > 16 {
            let disc_load_id = IrNodeId::new(match_node_id.get() * 100 + 900)
                .expect("disc load id");
            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
            operands.push(Operand::Reg(abi::RAX)); // RAX
            operands.push(Operand::MemSib {
                base: abi::RDI, // RDI
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });

            self.emit_inst(disc_load_id, Instruction {
                mnemonic: Mnemonic::Mov,
                operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
            emission_order: 0,
            });
        }

        // Label names
        let default_label = format!("match_default_{}", match_node_id.get());
        let end_label = format!("match_end_{}", match_node_id.get());

        // Track whether we saw a default arm to properly register the default_label
        let mut default_arm_registered = false;

        // Emit arms with cmp/jne cascade
        for (idx, &arm_id) in arm_ids.iter().enumerate() {
            let arm_meta = match arena.match_arm_meta().get(arm_id) {
                Some(m) => m,
                None => {
                    self.push_typed_diag(u1651_code(), format!(
                        "Match arm {} has no MatchArmMeta",
                        arm_id.get()
                    ));
                    return;
                }
            };

            let arm_label = format!("match_arm_{}_{}", match_node_id.get(), idx);

            // Hoist next_label computation for use in both pattern-check-jne and guard-jz
            let next_label = arm_ids.get(idx + 1)
                .and_then(|&next_id| arena.match_arm_meta().get(next_id))
                .map(|next_meta| {
                    if next_meta.is_default {
                        default_label.clone()
                    } else {
                        format!("match_arm_{}_{}", match_node_id.get(), idx + 1)
                    }
                })
                .unwrap_or_else(|| default_label.clone());

            // If default arm, skip comparisons and emit body directly
            if arm_meta.is_default {
                // #1241: Route default_label through label_to_instr; register_label captured a
                // pre-Unsafe-body estimated_offset that drifts once UnsafeWalker prepends
                // deferred bodies at encode time. Mirrors #1120's fix for end_label.
                let default_nop_id = IrNodeId::new(match_node_id.get() * 100 + 996)
                    .expect("default anchor NOP virtual id");
                self.emit_inst(default_nop_id, Instruction {
                    mnemonic: Mnemonic::Nop,
                    operands: SmallVec::new(),
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });
                self.state.insert_label(default_label.clone(), default_nop_id);
                default_arm_registered = true;
                if let Some(arm_node) = arena.get(arm_id) {
                    match arm_node.kind {
                        IrKind::Action => self.emit_block_body_arm(arm_id, arena, typer),
                        IrKind::Literal => {
                            // Literal arm (e.g., `0u64`): move value to RAX at a match-scoped
                            // IrNodeId so the MOV sorts between dispatch and arm-end anchor.
                            if let Some(value) = arena.literal_values().get(arm_id) {
                                let mov_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 4)
                                    .expect("literal arm mov id");
                                self.emit_mov_literal_to_reg_with_id(mov_id, abi::RAX, value);
                            }
                        }
                        IrKind::Var => {
                            // Var arm (e.g., `x`): move variable value to RAX
                            // Get binding name from binding_names table
                            if let Some(var_name) = arena.binding_names().get(arm_id) {
                                if let Some(var_reg) = self.state.local_bindings.get(var_name) {
                                    // Variable is already in a register; move it to RAX if needed
                                    if var_reg != abi::RAX {
                                        let mov_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 5)
                                            .expect("var arm mov id");
                                        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                        operands.push(Operand::Reg(abi::RAX));
                                        operands.push(Operand::Reg(var_reg));
                                        self.emit_inst(mov_id, Instruction {
                                            mnemonic: Mnemonic::Mov,
                                            operands,
                                            encoding_hint: None,
                                            byte_offset_in_text: None,
                                            mode: self.current_mode(),
                                            emission_order: 0,
                                        });
                                    }
                                }
                            }
                        }
                        IrKind::App => self.emit_arm_body_app(arm_id, arena, match_node_id, idx as u32),
                        _ => {
                            // Other IR kinds: emit diagnostic (arm body lowered to unsupported kind)
                            self.push_typed_diag(t0560_code(), format!("unsupported match arm body IR kind: {:?}", arm_node.kind));
                        }
                    }
                }
                continue;
            }

            // #1199: Register arm label at the START of the discriminator check (before cmp)
            // so that the previous arm's jne correctly targets this arm's cmp, not its body.
            // #1241: Route arm_label through label_to_instr; register_label captured a
            // pre-Unsafe-body estimated_offset that drifts once UnsafeWalker prepends
            // deferred bodies at encode time. Mirrors #1120's fix for end_label.
            let arm_nop_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 8)
                .expect("arm anchor NOP virtual id");
            self.emit_inst(arm_nop_id, Instruction {
                mnemonic: Mnemonic::Nop,
                operands: SmallVec::new(),
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            });
            self.state.insert_label(arm_label, arm_nop_id);

            // #1001: multi-alt cascade for or-patterns.
            let alts: Vec<u32> = if !arm_meta.alt_variant_indices.is_empty() {
                arm_meta.alt_variant_indices.clone()
            } else if let Some(v) = arm_meta.variant_index {
                vec![v]
            } else {
                Vec::new()  // shouldn't happen for non-default arms
            };

            // Track if we need to register body_label for multi-alt
            let mut body_label_to_register: Option<String> = None;

            if alts.is_empty() {
                // no discriminant check — fall through to guard/body (default arm case)
            } else if alts.len() == 1 {
                // SINGLE-ALT PATH: preserve exact original emission (single cmp/jne).
                // This keeps identical byte-for-byte output for existing non-or fixtures.
                let vi = alts[0];
                let cmp_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10)
                    .expect("cmp id");
                let mut cmp_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                cmp_operands.push(Operand::Reg(abi::RAX)); // RAX
                cmp_operands.push(Operand::Imm64(vi as i64));

                self.emit_inst(cmp_id, Instruction {
                    mnemonic: Mnemonic::Cmp,
                    operands: cmp_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });

                // Emit jne to next arm or default
                let jne_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 1)
                    .expect("jne id");
                let mut jne_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                jne_operands.push(Operand::LabelRef {
                    name: next_label.clone(),
                    addend: 0,
                });

                self.emit_inst(jne_id, Instruction {
                    mnemonic: Mnemonic::Jcc(Cond::Ne),
                    operands: jne_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });
            } else {
                // MULTI-ALT PATH: emit N-1 cmp/je to body_label, then last cmp/jne to next_label.
                let body_label = format!("match_arm_{}_{}_body", match_node_id.get(), idx);
                let last_idx = alts.len() - 1;
                for (a_idx, &vi) in alts.iter().enumerate() {
                    // cmp rax, imm(vi) — IrNodeId: match_id*1000 + idx*100 + a_idx*2 + 0
                    let cmp_id = IrNodeId::new(match_node_id.get() * 1000 + idx as u32 * 100 + a_idx as u32 * 2)
                        .expect("multi-alt cmp id");
                    let mut cmp_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                    cmp_operands.push(Operand::Reg(abi::RAX));
                    cmp_operands.push(Operand::Imm64(vi as i64));
                    self.emit_inst(cmp_id, Instruction {
                        mnemonic: Mnemonic::Cmp,
                        operands: cmp_operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    });

                    // if a_idx == last_idx: jne next_label — IrNodeId: match_id*1000 + idx*100 + a_idx*2 + 1
                    // else: je body_label — IrNodeId: match_id*1000 + idx*100 + a_idx*2 + 1
                    let jcc_id = IrNodeId::new(match_node_id.get() * 1000 + idx as u32 * 100 + a_idx as u32 * 2 + 1)
                        .expect("multi-alt jcc id");
                    let mut jcc_operands: SmallVec<[Operand; 3]> = SmallVec::new();

                    if a_idx == last_idx {
                        jcc_operands.push(Operand::LabelRef {
                            name: next_label.clone(),
                            addend: 0,
                        });
                        self.emit_inst(jcc_id, Instruction {
                            mnemonic: Mnemonic::Jcc(Cond::Ne),
                            operands: jcc_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    } else {
                        jcc_operands.push(Operand::LabelRef {
                            name: body_label.clone(),
                            addend: 0,
                        });
                        self.emit_inst(jcc_id, Instruction {
                            mnemonic: Mnemonic::Jcc(Cond::Eq),
                            operands: jcc_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                }
                debug_assert!(alts.len() * 2 < 100, "or-pattern alt count exceeds IrNodeId slots");
                // register body_label at current emission position
                body_label_to_register = Some(body_label);
            }

            // Register body_label if needed (multi-alt case)
            if let Some(body_label) = body_label_to_register {
                // #1241: Route body_label through label_to_instr; register_label captured a
                // pre-Unsafe-body estimated_offset that drifts once UnsafeWalker prepends
                // deferred bodies at encode time. Mirrors #1120's fix for end_label.
                let body_nop_id = IrNodeId::new(match_node_id.get() * 1000 + idx as u32 * 100 + 98)
                    .expect("body anchor NOP virtual id");
                self.emit_inst(body_nop_id, Instruction {
                    mnemonic: Mnemonic::Nop,
                    operands: SmallVec::new(),
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });
                self.state.insert_label(body_label, body_nop_id);
            }

            // Issue #1002: Handle outer binder (bind-and-match)
            // Must be done before pattern_binding so the discriminant is still in RAX
            let should_pop_outer_binder_scope = if let Some(ref binder_name) = arm_meta.outer_binder {
                if layout_size > 16 {
                    // Stack form: emit T-code diagnostic (MVP forbids this)
                    self.push_typed_diag(
                        paideia_as_diagnostics::DiagnosticCode::new(
                            paideia_as_diagnostics::Category::T,
                            paideia_as_diagnostics::Severity::Error,
                            1003,
                        ).unwrap_or_else(|_| unreachable!()),
                        "bind-and-match on stack-form enum not supported (MVP)".to_string(),
                    );
                    false
                } else {
                    // Register form: bind discriminant (RAX) to the binder name
                    self.state.local_bindings.push_scope();
                    self.state.local_bindings.insert(binder_name.clone(), abi::RAX);
                    true
                }
            } else {
                false
            };

            // Phase 17 m9-009: Nested pattern binding
            // #1084 (follow-up): Split on layout.size for register vs stack form
            if let Some(ref pattern_binding) = arm_meta.pattern_binding {
                self.state.local_bindings.push_scope();
                let mut slot = 0u32;

                if layout_size <= 16 {
                    // Register form: extract from RDX (payload already loaded)
                    self.lower_pattern_from_reg(
                        pattern_binding,
                        abi::RDX,   // Source register (payload)
                        0,           // bit_offset = 0
                        arm_id,
                        &mut slot,
                        arena,
                        (8, false),  // default: u64 unsigned
                    );
                } else {
                    // Stack form: extract from memory via RDI (scrutinee pointer)
                    self.lower_pattern(
                        pattern_binding,
                        abi::RDI,    // RDI = base register (scrutinee pointer)
                        0,           // base_offset
                        arm_id,
                        &mut slot,
                        arena,
                        (8, false),  // default: u64 unsigned
                    );
                }
                // Note: pop_scope happens after emit_block_body_arm below
            } else if let Some(ref binder) = arm_meta.payload_binder {
                // Legacy single-payload binder (from #986)
                // #1084 (follow-up): Split on layout.size for register vs stack form
                if layout_payload_size > 0 {
                    if layout_size <= 16 {
                        // Register form (size <= 16): payload already in RDX after scrutinee load
                        // No memory load needed — direct rebind
                        self.state.local_bindings.insert(binder.clone(), abi::RDX);
                    } else {
                        // Stack form (size > 16): emit memory load from [RDI+8]
                        let payload_load_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 2)
                            .expect("payload load id");
                        let mut payload_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        payload_operands.push(Operand::Reg(abi::RDX)); // RDX
                        payload_operands.push(Operand::MemSib {
                            base: abi::RDI, // RDI
                            index: None,
                            scale: paideia_as_ir::instruction::Scale::X1,
                            disp: 8,
                        });

                        self.emit_inst(payload_load_id, Instruction {
                            mnemonic: Mnemonic::Mov,
                            operands: payload_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                        emission_order: 0,
                        });

                        // Set up local binding for the payload binder variable
                        self.state.local_bindings.insert(binder.clone(), abi::RDX);
                    }
                }
            }

            // Emit guard if present (#1000)
            if let Some(guard_ir_id) = arm_meta.guard {
                // Emit guard expression evaluation into RAX
                self.emit_guard_expression(guard_ir_id, arena);

                // test rax, rax
                let test_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 6)
                    .expect("guard test id");
                let mut test_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                test_ops.push(Operand::Reg(abi::RAX));
                test_ops.push(Operand::Reg(abi::RAX));
                self.emit_inst(test_id, Instruction {
                    mnemonic: Mnemonic::Test,
                    operands: test_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });

                // jz next_label (if guard is false, jump to next arm)
                let jz_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 7)
                    .expect("guard jz id");
                let mut jz_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                jz_ops.push(Operand::LabelRef {
                    name: next_label.clone(),
                    addend: 0,
                });
                self.emit_inst(jz_id, Instruction {
                    mnemonic: Mnemonic::Jcc(Cond::Zero),
                    operands: jz_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });
            }

            // Emit arm body based on its IR kind
            if let Some(arm_node) = arena.get(arm_id) {
                match arm_node.kind {
                    IrKind::Action => {
                        // Action (Block) arm: emit all statements + tail
                        self.emit_block_body_arm(arm_id, arena, typer)
                    }
                    IrKind::Literal => {
                        // Literal arm (e.g., `0u64`): move value to RAX at a match-scoped
                        // IrNodeId so the MOV sorts between dispatch and arm-end anchor.
                        // #1128: use _with_id variant; emit_mov_literal_to_reg self-allocates
                        // via alloc_synthetic_id(), which is fine for call args but wrong here.
                        if let Some(value) = arena.literal_values().get(arm_id) {
                            let mov_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 4)
                                .expect("literal arm mov id");
                            self.emit_mov_literal_to_reg_with_id(mov_id, abi::RAX, value);
                        }
                    }
                    IrKind::Var => {
                        // Var arm (e.g., `x`): move variable value to RAX
                        // Get binding name from binding_names table
                        if let Some(var_name) = arena.binding_names().get(arm_id) {
                            if let Some(var_reg) = self.state.local_bindings.get(var_name) {
                                // Variable is already in a register; move it to RAX if needed
                                if var_reg != abi::RAX {
                                    let mov_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 5)
                                        .expect("var arm mov id");
                                    let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                    operands.push(Operand::Reg(abi::RAX));
                                    operands.push(Operand::Reg(var_reg));
                                    self.emit_inst(mov_id, Instruction {
                                        mnemonic: Mnemonic::Mov,
                                        operands,
                                        encoding_hint: None,
                                        byte_offset_in_text: None,
                                        mode: self.current_mode(),
                                    emission_order: 0,
                                    });
                                }
                            }
                        }
                    }
                    IrKind::App => self.emit_arm_body_app(arm_id, arena, match_node_id, idx as u32),
                    _ => {
                        // Other IR kinds: emit diagnostic (arm body lowered to unsupported kind)
                        self.push_typed_diag(t0560_code(), format!("unsupported match arm body IR kind: {:?}", arm_node.kind));
                    }
                }
            }

            // Pop nested pattern scope if we pushed one
            if arm_meta.pattern_binding.is_some() {
                self.state.local_bindings.pop_scope();
            }

            // Issue #1002: Pop outer binder scope if we pushed one
            if should_pop_outer_binder_scope {
                self.state.local_bindings.pop_scope();
            }

            // Emit jmp end
            let jmp_end_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 3)
                .expect("jmp end id");
            let mut jmp_end_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            jmp_end_operands.push(Operand::LabelRef {
                name: end_label.clone(),
                addend: 0,
            });

            self.emit_inst(jmp_end_id, Instruction {
                mnemonic: Mnemonic::Jmp,
                operands: jmp_end_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
            emission_order: 0,
            });
        }

        // Register default label if no explicit default arm was present
        // (when all arms are non-default enum variants, the last arm's jne jumps here)
        // #1120: use label_to_instr via NOP anchor for post-sort resolution
        if !default_arm_registered {
            let default_nop_id = IrNodeId::new(match_node_id.get() * 100 + 997)
                .expect("match default anchor NOP virtual id");
            let default_nop_inst = Instruction {
                mnemonic: Mnemonic::Nop,
                operands: SmallVec::new(),
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
};
            self.emit_inst(default_nop_id, default_nop_inst);
            self.state.insert_label(default_label, default_nop_id);
        }

        // #1120: register end_label via label_to_instr (offset resolved post-sort)
        // instead of register_label (which captures walker-time estimated_offset).
        // Anchor the label to a 1-byte NOP whose IrNodeId is high enough to sort
        // after every arm-end jmp (M*100 + idx*10 + 3).
        let end_nop_id = IrNodeId::new(match_node_id.get() * 100 + 999)
            .expect("match end anchor NOP virtual id");
        let end_nop_inst = Instruction {
            mnemonic: Mnemonic::Nop,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
};
        self.emit_inst(end_nop_id, end_nop_inst);
        self.state.insert_label(end_label, end_nop_id);
    }

    /// Emit guard expression evaluation into RAX (#1000).
    ///
    /// Evaluates the guard expression (which should be a comparison or variable)
    /// and places the result (boolean: 0 or non-zero) into RAX for testing.
    /// Uses the existing emit_var_assign_expr_to_reg machinery to handle
    /// Var (load variable), Literal (load constant), and comparison operators.
    pub(crate) fn emit_guard_expression(&mut self, guard_ir_id: IrNodeId, arena: &IrArena) {
        let _ = self.emit_var_assign_expr_to_reg(guard_ir_id, arena, abi::RAX, 0);
    }
}
