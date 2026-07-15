//! Store + record-constructor lowerers.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Covers
//! `visit_store` (l-value assignment via `MemSib`) and `visit_record_cons`
//! (Phase 6 m3-004 cap-mint record constructor for the 4-field all-u64
//! capability descriptor shape).

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi};
use paideia_as_ir::symbol::SymbolKind;
use paideia_as_diagnostics::{DiagnosticCode, Category, Severity};

use crate::emit_walker::EmitWalker;

/// Helper to construct T0518 diagnostic code.
fn t0518_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 518)
        .expect("T0518 is within valid T range")
}

/// Helper to construct T0540 diagnostic code.
fn t0540_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 540)
        .expect("T0540 is within valid T range")
}

/// Helper to construct U1623 diagnostic code.
fn u1623_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1623)
        .expect("U1623 is within valid U range")
}

/// #1181: arg1-scratch pool for nested BinOp lowering.
/// Indexed by recursion depth. Index 0 → outer arg1 scratch (dest is RAX).
/// Index 1 → level-1 arg1 scratch (dest is R10). Length caps the supported
/// nesting depth; going beyond fires T0540.
///
/// R10 and R11 are both SysV caller-saved and appear in PATTERN_SCRATCH
/// (see abi.rs). They are not SysV arg registers, so live parameter values
/// in RDI/RSI/RDX/RCX/R8/R9 are not disturbed. R12+ would be callee-saved
/// and require prologue plumbing this MVP does not have.
const ARG1_SCRATCHES: [RegId; 2] = [
    paideia_as_ir::abi::R10,
    paideia_as_ir::abi::R11,
];

impl EmitWalker {
    /// Phase 6 m3-004: Emit record constructor lowering for cap-mint shape.
    ///
    /// Accepts only the 4-field all-u64 capability descriptor shape:
    /// - Field 0: u64 at offset 0 (from RSI = arg 2)
    /// - Field 1: u64 at offset 8 (from RDX = arg 3)
    /// - Field 2: u64 at offset 16 (from RCX = arg 4)
    /// - Field 3: u64 at offset 24 (from R8 = arg 5)
    /// Buffer pointer is in RDI (arg 0).
    ///
    /// For literal-valued fields, emits `mov [rdi + offset], 0` via
    /// imm32-sign-extended form: `48 C7 47 18 00 00 00 00` (8 bytes).
    ///
    /// Fires T0518 for unsupported shapes.
    pub(crate) fn visit_store(&mut self, store_id: IrNodeId, arena: &IrArena) {
        // Phase 7 m5-001 & m5-002: l-value assignment emission.
        // Store has three children: [addr, index_or_unused, value].
        // m5-001: a[i] = value → [base, index, value]
        // m5-002: *p = value → [pointer, unused, value]
        // m5-002: (*p).f = value → [pointer, unused, value] (offset handled later)
        let children = arena.children(store_id);
        if children.len() != 3 {
            self.push_typed_diag(
                u1623_code(),
                format!("Store node {} has {} children; expected 3", store_id.get(), children.len()),
            );
            return;
        }

        let addr_id = children[0];
        let _index_or_unused_id = children[1];
        let value_id = children[2];

        let addr_node = arena.get(addr_id);
        let value_node = arena.get(value_id);

        if addr_node.map(|n| n.kind) != Some(IrKind::Var) {
            self.push_typed_diag(
                u1623_code(),
                format!("Store addr must be Var; got {:?}", addr_node.map(|n| n.kind)),
            );
            return;
        }

        if value_node.map(|n| n.kind) != Some(IrKind::Var) {
            self.push_typed_diag(
                u1623_code(),
                format!("Store value must be Var; got {:?}", value_node.map(|n| n.kind)),
            );
            return;
        }

        // Determine if this is m5-001 (array index) or m5-002 (deref).
        // If the second child is a Var, it's m5-001 (index).
        // If the second child is not a Var (e.g., Placeholder from operator), it's m5-002.
        let is_array_store = arena
            .get(_index_or_unused_id)
            .map(|n| n.kind == IrKind::Var)
            .unwrap_or(false);

        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();

        if is_array_store {
            // m5-001: a[i] = value
            // Operands: [base, index, value] = [rdi, rsi, rdx]
            // Emit: mov [rdi + rsi*8], rdx
            operands.push(Operand::MemSib {
                base: abi::RDI,        // rdi (base)
                index: Some(abi::RSI), // rsi (index)
                scale: paideia_as_ir::instruction::Scale::X8,
                disp: 0,
            });
            operands.push(Operand::Reg(abi::RDX)); // rdx (value, source)
        } else {
            // m5-002: *p = value or (*p).f = value
            // Operands: [pointer, value] = [rdi, rdx]
            // Emit: mov [rdi], rdx (use MemSib with no index for [base] addressing)
            operands.push(Operand::MemSib {
                base: abi::RDI,                               // rdi (pointer)
                index: None,                                  // no index
                scale: paideia_as_ir::instruction::Scale::X1, // ignored when no index
                disp: 0,
            });
            operands.push(Operand::Reg(abi::RDX)); // rdx (value, source)
        }

        // PA8-m3-001 (generic Mov retained): memory-*store* move (`mov [rdi], rdx`).
        // The destination is memory, not a register, so MovSized (which encodes a
        // register-destination immediate move) does not apply; store width is the
        // encoder's concern.
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };

        self.emit_inst(store_id, inst);
    }

    /// Pattern 5 (#1116): Emit variable assignment for module-level `let mut` via lambda body.
    ///
    /// Handles: `counter = v` where counter is a module-level symbol and v is a register parameter.
    ///
    /// Children layout (from Edit 1b): `[Var(lhs_name), op, Var(rhs_name)]`
    /// - child[0]: Var node for LHS (module symbol name)
    /// - child[1]: op (=) node — unused
    /// - child[2]: Var node for RHS (source register parameter)
    ///
    /// Resolution:
    /// - LHS: lookup via `arena.symbols().lookup_by_name` → must be a module symbol, not in `local_bindings`
    /// - RHS: lookup in `local_bindings` → must resolve to a register (function parameter)
    ///
    /// Fallback behavior:
    /// - If LHS is shadowed by local binding: emit T0518 diagnostic (do NOT silently shadow module symbol)
    /// - If RHS is not in local_bindings: emit T0518 (non-register sources not yet supported)
    /// - If RHS is literal: fall back to generic Store or emit T0518
    ///
    /// Emission (MVP):
    /// - Hardcoded u64 (8-byte) width: `mov [rip+counter], rdi` (source in RDI per SysV ABI)
    /// - Uses `emit_mem_write_via_rip_sym(store_id, src_reg, "counter", 0, 8, false)`
    /// - T0529 for non-u64 widths (mirrors `emit_mem_write_via_rip_sym` gap)
    pub(crate) fn visit_var_assign(&mut self, store_id: IrNodeId, arena: &IrArena) {
        let children = arena.children(store_id);
        if children.len() != 3 {
            self.push_typed_diag(
                t0540_code(),
                format!(
                    "Store node {} has {} children; expected 3",
                    store_id.get(),
                    children.len()
                ),
            );
            return;
        }

        let lhs_id = children[0];
        let _op_id = children[1];
        let rhs_id = children[2];

        // Verify both children are Var nodes
        let lhs_node = arena.get(lhs_id);
        let rhs_node = arena.get(rhs_id);

        if lhs_node.map(|n| n.kind) != Some(IrKind::Var) {
            self.push_typed_diag(
                t0540_code(),
                format!(
                    "Store (var_assign) LHS must be Var; got {:?}",
                    lhs_node.map(|n| n.kind)
                ),
            );
            return;
        }

        // Resolve LHS name
        let lhs_name = match arena.binding_names().get(lhs_id) {
            Some(name) => name.to_string(),
            None => {
                self.push_typed_diag(
                    t0540_code(),
                    format!(
                        "Store (var_assign) LHS Var {} has no binding name",
                        lhs_id.get()
                    ),
                );
                return;
            }
        };

        // #1138: LHS is a function-local let-mut → rewrite the target register.
        if let Some(dest_reg) = self.state.local_bindings.get(&lhs_name) {
            let rhs_kind = rhs_node.map(|n| n.kind);
            let src_operand = match rhs_kind {
                Some(IrKind::Literal) => {
                    let imm = arena.literal_values().get(rhs_id).unwrap_or(0);
                    Operand::Imm64(imm)
                }
                Some(IrKind::Var) => {
                    let name = match arena.binding_names().get(rhs_id) {
                        Some(n) => n.to_string(),
                        None => {
                            self.push_typed_diag(t0540_code(),
                                format!("local var_assign RHS Var {} has no binding name", rhs_id.get()));
                            return;
                        }
                    };
                    match self.state.local_bindings.get(&name) {
                        Some(src_reg) => Operand::Reg(src_reg),
                        None => {
                            self.push_typed_diag(t0540_code(),
                                format!("local var_assign RHS {} not found in local bindings", name));
                            return;
                        }
                    }
                }
                _ => {
                    self.push_typed_diag(t0540_code(),
                        format!("local var_assign RHS must be Literal or Var; got {:?}", rhs_kind));
                    return;
                }
            };
            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
            operands.push(Operand::Reg(dest_reg));
            operands.push(src_operand);
            self.emit_inst(store_id, Instruction {
                mnemonic: Mnemonic::Mov,
                operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            });
            return;
        }

        // Check if LHS is shadowed by a local binding (error case)
        if self.state.local_bindings.contains(&lhs_name) {
            self.push_typed_diag(
                t0540_code(),
                format!(
                    "variable {} is shadowed by a local binding; cannot assign to module symbol",
                    lhs_name
                ),
            );
            return;
        }

        // Verify LHS is a module symbol (not a function parameter)
        if arena.symbols().lookup_by_name(&lhs_name).is_none() {
            self.push_typed_diag(
                t0540_code(),
                format!(
                    "variable {} is not a module symbol; var_assign requires module-level let mut",
                    lhs_name
                ),
            );
            return;
        }

        // Resolve the RHS source register, dispatching on RHS shape.
        let src_reg = match rhs_node.map(|n| n.kind) {
            Some(IrKind::Var) => {
                let rhs_name = match arena.binding_names().get(rhs_id) {
                    Some(name) => name.to_string(),
                    None => {
                        self.push_typed_diag(
                            t0540_code(),
                            format!(
                                "Store (var_assign) RHS Var {} has no binding name",
                                rhs_id.get()
                            ),
                        );
                        return;
                    }
                };
                match self.state.local_bindings.get(&rhs_name) {
                    Some(reg) => reg,
                    None => {
                        // Issue #1179: RHS is a module-level Object (const or let-mut),
                        // not a local binding — the same blindness fixed in #1176 for
                        // emit_call.rs. Mirror the App-RHS branch below: materialise
                        // the value into RAX via a RIP-relative load, then use RAX as
                        // the store source. u64/8-byte hardcode carried forward from
                        // #1176 (documented gap; separate follow-up for width dispatch).
                        let is_module_object = arena
                            .symbols()
                            .lookup_by_name(&rhs_name)
                            .map(|s| matches!(s.kind, SymbolKind::Object))
                            .unwrap_or(false);
                        if is_module_object {
                            let load_id = self.alloc_synthetic_id();
                            self.emit_mem_read_via_rip_sym(
                                load_id,
                                paideia_as_ir::abi::RAX,
                                rhs_name.clone(),
                                0,
                                8,
                                false,
                            );
                            paideia_as_ir::abi::RAX
                        } else {
                            self.push_typed_diag(
                                t0540_code(),
                                format!(
                                    "var_assign RHS {} not found in local bindings; non-register sources not yet supported",
                                    rhs_name
                                ),
                            );
                            return;
                        }
                    }
                }
            }
            Some(IrKind::App) => {
                // #1181: dispatch on operator vs function call based on call_sites metadata
                if let Some(meta) = arena.call_sites().get(rhs_id).cloned() {
                    if is_operator_callee(&meta.callee_name) {
                        if !self.emit_var_assign_expr_to_rax(rhs_id, arena) {
                            return; // helper pushed a T0540 with detail
                        }
                        paideia_as_ir::abi::RAX
                    } else {
                        self.emit_call_expr_for_var_assign_rhs(rhs_id, &meta.callee_name, arena)
                    }
                } else {
                    // Legacy fallback: no call_sites entry — preserve pre-#1181 behaviour.
                    let app_children = arena.children(rhs_id);
                    if app_children.is_empty() {
                        self.push_typed_diag(
                            t0540_code(),
                            format!(
                                "Store (var_assign) App RHS {} has no children",
                                rhs_id.get()
                            ),
                        );
                        return;
                    }
                    let callee_id = app_children[0];
                    let target_name = match arena.binding_names().get(callee_id) {
                        Some(name) => name.to_string(),
                        None => {
                            self.push_typed_diag(
                                t0540_code(),
                                format!(
                                    "Store (var_assign) App RHS callee {} has no binding name",
                                    callee_id.get()
                                ),
                            );
                            return;
                        }
                    };
                    let arg_ids: Vec<IrNodeId> = app_children[1..].to_vec();
                    let lambda_id = IrNodeId::new(self.state.current_function)
                        .expect("current_function set by walker");
                    self.emit_call_expr(lambda_id, target_name, &arg_ids, arena);
                    paideia_as_ir::abi::RAX
                }
            }
            Some(IrKind::BitNot) => {
                if !self.emit_var_assign_expr_to_rax(rhs_id, arena) {
                    return;
                }
                paideia_as_ir::abi::RAX
            }
            _ => {
                self.push_typed_diag(
                    t0540_code(),
                    format!(
                        "Store (var_assign) RHS must be Var, App, or BitNot; got {:?}",
                        rhs_node.map(|n| n.kind)
                    ),
                );
                return;
            }
        };

        // MVP: hardcoded u64 width (8 bytes)
        // T0529 for non-u64 widths
        let size = 8;
        let signed = false;
        let addend = 0;

        // Emit store via rip-relative symbol reference
        self.emit_mem_write_via_rip_sym(store_id, src_reg, lhs_name, addend, size, signed);
    }

    pub(crate) fn visit_record_cons(&mut self, record_cons_id: IrNodeId, arena: &IrArena) {
        // Look up the RecordTypeId for this RecordCons node.
        let type_id = match arena.record_layout_table().get(record_cons_id) {
            Some(&tid) => tid,
            None => {
                // No layout entry → unsupported shape → T0518
                self.push_typed_diag(
                    t0518_code(),
                    format!(
                        "RecordCons node {} has no layout entry (unsupported shape in Phase 6)",
                        record_cons_id.get()
                    ),
                );
                return;
            }
        };

        // Look up the finalised layout for this type.
        let layout = match self.state.record_layout(type_id) {
            Some(l) => l,
            None => {
                // Layout not finalised → unsupported
                self.push_typed_diag(
                    t0518_code(),
                    format!(
                        "RecordCons node {} type {} not finalised (unsupported shape in Phase 6)",
                        record_cons_id.get(),
                        type_id.0
                    ),
                );
                return;
            }
        };

        // Phase 6 m3-004: Accept only the cap-mint shape:
        // - Exactly 4 fields
        // - All u64 (size 8 each)
        // - Offsets [0, 8, 16, 24], total size 32, align 8
        if layout.fields.len() != 4 {
            self.push_typed_diag(
                t0518_code(),
                format!(
                    "RecordCons node {} has {} fields; cap-mint requires 4 (unsupported shape in Phase 6)",
                    record_cons_id.get(),
                    layout.fields.len()
                ),
            );
            return;
        }

        for (i, field) in layout.fields.iter().enumerate() {
            if field.size != 8 {
                self.push_typed_diag(
                    t0518_code(),
                    format!(
                        "RecordCons node {} field {} has size {}; cap-mint requires u64 (size 8) (unsupported shape in Phase 6)",
                        record_cons_id.get(),
                        i,
                        field.size
                    ),
                );
                return;
            }
            let expected_offset = (i as u64) * 8;
            if field.offset != expected_offset {
                self.push_typed_diag(
                    t0518_code(),
                    format!(
                        "RecordCons node {} field {} has offset {}; cap-mint requires offset {} (unsupported shape in Phase 6)",
                        record_cons_id.get(),
                        i,
                        field.offset,
                        expected_offset
                    ),
                );
                return;
            }
        }

        // Shape is valid cap-mint. Get field values from children.
        let children = arena.children(record_cons_id);
        if children.len() != 4 {
            self.push_typed_diag(
                t0518_code(),
                format!(
                    "RecordCons node {} has {} children; cap-mint requires 4 (unsupported shape in Phase 6)",
                    record_cons_id.get(),
                    children.len()
                ),
            );
            return;
        }

        // Argument register assignment: RSI, RDX, RCX, R8 for args 2..5
        // In RegId terms: RSI=6, RDX=2, RCX=1, R8=8
        let arg_regs = [abi::RSI, abi::RDX, abi::RCX, abi::R8];

        // Emit 4 store instructions in field-declaration order.
        for (field_idx, &arg_reg) in arg_regs.iter().enumerate() {
            let field_offset = (field_idx as i32) * 8;

            // Check if this field is a literal (0).
            let is_literal = if let Some(child_node) = arena.get(children[field_idx]) {
                child_node.kind == IrKind::Literal
            } else {
                false
            };

            if is_literal {
                // Emit: mov [rdi + offset], 0 via imm32-sign-extended form.
                // Encoding: 48 C7 47 NN 00 00 00 00 (8 bytes for small offsets)
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::MemSib {
                    base: abi::RDI, // rdi = buffer pointer
                    index: None,
                    scale: paideia_as_ir::instruction::Scale::X1,
                    disp: field_offset,
                });
                operands.push(Operand::Imm64(0));

                // PA8-m3-001 (generic Mov retained): memory-store immediate
                // (`mov [rdi+off], 0`). Destination is memory; MovSized encodes a
                // register-destination immediate move only, so it does not apply.
                let mut inst = Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };

                // Virtual ID: record_cons_id * 10 + field_idx to sort in order.
                let inst_id = IrNodeId::new(record_cons_id.get() * 10 + field_idx as u32)
                    .expect("virtual id");
                // TODO(step5-encoder): encode_mov does not yet handle
                // [MemSib, Imm64] (only MovSized does), so estimated_bytes
                // would return 0 here. Keep the hardcoded literal until
                // encode_mov gains this arm. Bytes: 48 C7 47 NN 00 00 00 00
                // = 8 bytes for small offsets.
                // #1140: Set emission_order before direct insert to match emit_inst behavior.
                inst.emission_order = self.state.next_emission_order;
                self.state.next_emission_order += 1;
                self.state.instructions.insert(inst_id, inst);
                self.state.estimated_offset += 8;
            } else {
                // Emit: mov [rdi + offset], arg_reg via MemSib.
                // Encoding: 48 89 47 NN (4 bytes for small offsets)
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::MemSib {
                    base: abi::RDI, // rdi = buffer pointer
                    index: None,
                    scale: paideia_as_ir::instruction::Scale::X1,
                    disp: field_offset,
                });
                operands.push(Operand::Reg(arg_reg));

                // PA8-m3-001 (generic Mov retained): memory-store reg move
                // (`mov [rdi+off], reg`). Destination is memory; not MovSized-encodable.
                let inst = Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };

                // Virtual ID: record_cons_id * 10 + field_idx to sort in order.
                let inst_id = IrNodeId::new(record_cons_id.get() * 10 + field_idx as u32)
                    .expect("virtual id");
                self.emit_inst(inst_id, inst);
            }
        }
    }

    /// #1181: lower a var_assign RHS expression tree into RAX.
    ///
    /// Supports operator nesting up to depth 2 (kernel-actual shape:
    /// `bitmap | (1 << p)`). Arg1 subtrees at depth 0 land in R10; at depth 1
    /// they land in R11 (see `ARG1_SCRATCHES`). Depth ≥ 2 fires T0540 (explicit
    /// failure > silent miscompile). BitNot is unary and does not consume a
    /// scratch slot. Clobbers RAX + R10 + R11 + RCX — all SysV caller-saved;
    /// store-bodied lambda always ends in RET.
    pub(crate) fn emit_var_assign_expr_to_rax(
        &mut self,
        expr_id: IrNodeId,
        arena: &IrArena,
    ) -> bool {
        self.emit_var_assign_expr_to_reg(expr_id, arena, paideia_as_ir::abi::RAX, 0)
    }

    /// #1181: lower a var_assign RHS expression tree into a given register.
    /// Returns true on success; false if any push_typed_diag fired.
    pub(crate) fn emit_var_assign_expr_to_reg(
        &mut self,
        expr_id: IrNodeId,
        arena: &IrArena,
        dest: RegId,
        depth: usize,
    ) -> bool {
        match arena.get(expr_id).map(|n| n.kind) {
            Some(IrKind::Literal) => {
                let imm = arena.literal_values().get(expr_id).unwrap_or(0);
                let inst = Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: {
                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        ops.push(Operand::Reg(dest));
                        ops.push(Operand::Imm64(imm));
                        ops
                    },
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };
                let inst_id = self.alloc_synthetic_id();
                self.emit_inst(inst_id, inst);
                true
            }
            Some(IrKind::Var) => {
                let name = match arena.binding_names().get(expr_id) {
                    Some(n) => n.to_string(),
                    None => {
                        self.push_typed_diag(
                            t0540_code(),
                            format!("expr Var {} has no binding name", expr_id.get()),
                        );
                        return false;
                    }
                };
                // Try local binding first
                if let Some(src_reg) = self.state.local_bindings.get(&name) {
                    let inst = Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: {
                            let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                            ops.push(Operand::Reg(dest));
                            ops.push(Operand::Reg(src_reg));
                            ops
                        },
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    };
                    let inst_id = self.alloc_synthetic_id();
                    self.emit_inst(inst_id, inst);
                    true
                } else {
                    // Module object: load via RIP-relative
                    let is_module_object = arena
                        .symbols()
                        .lookup_by_name(&name)
                        .map(|s| matches!(s.kind, SymbolKind::Object))
                        .unwrap_or(false);
                    if is_module_object {
                        let load_id = self.alloc_synthetic_id();
                        self.emit_mem_read_via_rip_sym(
                            load_id,
                            dest,
                            name.clone(),
                            0,
                            8,
                            false,
                        );
                        true
                    } else {
                        self.push_typed_diag(
                            t0540_code(),
                            format!("expr Var {} not found in bindings; unsupported", name),
                        );
                        false
                    }
                }
            }
            Some(IrKind::BitNot) => {
                let children = arena.children(expr_id);
                if children.len() != 1 {
                    self.push_typed_diag(
                        t0540_code(),
                        format!("BitNot {} has {} children; expected 1", expr_id.get(), children.len()),
                    );
                    return false;
                }
                if !self.emit_var_assign_expr_to_reg(children[0], arena, dest, depth) {
                    return false;
                }
                let inst = Instruction {
                    mnemonic: Mnemonic::Not,
                    operands: {
                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        ops.push(Operand::Reg(dest));
                        ops
                    },
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };
                let inst_id = self.alloc_synthetic_id();
                self.emit_inst(inst_id, inst);
                true
            }
            Some(IrKind::App) => {
                // Binary operator application
                if let Some(meta) = arena.call_sites().get(expr_id).cloned() {
                    if !is_operator_callee(&meta.callee_name) {
                        self.push_typed_diag(
                            t0540_code(),
                            format!("nested function calls not supported in BinOp RHS"),
                        );
                        return false;
                    }
                    let children = arena.children(expr_id);
                    if children.len() != 3 {
                        self.push_typed_diag(
                            t0540_code(),
                            format!("App {} has {} children; expected 3 (callee + 2 args)", expr_id.get(), children.len()),
                        );
                        return false;
                    }

                    // #1181 corrective: depth-indexed arg1 scratch (was hardcoded R10).
                    let arg1_dest = match ARG1_SCRATCHES.get(depth) {
                        Some(&r) => r,
                        None => {
                            self.push_typed_diag(
                                t0540_code(),
                                format!(
                                    "BinOp RHS nesting depth {} exceeds supported limit {} \
                                     (no scratch register available for arg1 — refactor \
                                     deeply-nested expression into let-bindings)",
                                    depth + 1,
                                    ARG1_SCRATCHES.len(),
                                ),
                            );
                            return false;
                        }
                    };

                    // arg0 into dest at SAME depth (arg0 does not consume a scratch slot).
                    if !self.emit_var_assign_expr_to_reg(children[1], arena, dest, depth) {
                        return false;
                    }
                    // arg1 into fresh scratch at depth+1.
                    if !self.emit_var_assign_expr_to_reg(children[2], arena, arg1_dest, depth + 1) {
                        return false;
                    }

                    // Dispatch on operator
                    let mnemonic = match meta.callee_name.as_str() {
                        "|" => Mnemonic::Or,
                        "&" => Mnemonic::And,
                        "^" => Mnemonic::Xor,
                        "+" => Mnemonic::Add,
                        "-" => Mnemonic::Sub,
                        "*" => Mnemonic::Imul,
                        "/" | "%" => {
                            let result_reg = if meta.callee_name == "/" { abi::RAX } else { abi::RDX };
                            self.emit_div_unsigned(dest, arg1_dest, result_reg);
                            return true;
                        }
                        "<<" | ">>" => {
                            // Shift operators require RCX for the count.
                            // If RCX is in use by a prior binding (when dest != RCX), we must save/restore it.
                            // Check if RCX is in local_bindings (i.e., holds a binding value).
                            let rcx_in_use = if dest != paideia_as_ir::abi::RCX {
                                self.state.local_bindings.iter().any(|(_, reg)| reg == paideia_as_ir::abi::RCX)
                            } else {
                                false // If dest is RCX, RCX will be overwritten anyway
                            };

                            if rcx_in_use {
                                // Save RCX before moving shift count there
                                let save_inst = Instruction {
                                    mnemonic: Mnemonic::Push,
                                    operands: {
                                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                        ops.push(Operand::Reg(paideia_as_ir::abi::RCX));
                                        ops
                                    },
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                    emission_order: 0,
                                };
                                let save_id = self.alloc_synthetic_id();
                                self.emit_inst(save_id, save_inst);
                            }

                            // Move shift count to RCX
                            let mov_inst = Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: {
                                    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                    ops.push(Operand::Reg(paideia_as_ir::abi::RCX));
                                    ops.push(Operand::Reg(arg1_dest));
                                    ops
                                },
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
                            };
                            let mov_id = self.alloc_synthetic_id();
                            self.emit_inst(mov_id, mov_inst);

                            let mnemonic = if meta.callee_name == "<<" {
                                Mnemonic::Shl
                            } else {
                                Mnemonic::Shr
                            };

                            // Perform the shift operation
                            let shift_operands = {
                                let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                ops.push(Operand::Reg(dest));
                                ops.push(Operand::Reg(paideia_as_ir::abi::RCX));
                                ops
                            };
                            let shift_inst = Instruction {
                                mnemonic,
                                operands: shift_operands,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
                            };
                            let shift_id = self.alloc_synthetic_id();
                            self.emit_inst(shift_id, shift_inst);

                            if rcx_in_use {
                                // Restore RCX after the shift
                                let restore_inst = Instruction {
                                    mnemonic: Mnemonic::Pop,
                                    operands: {
                                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                                        ops.push(Operand::Reg(paideia_as_ir::abi::RCX));
                                        ops
                                    },
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                    emission_order: 0,
                                };
                                let restore_id = self.alloc_synthetic_id();
                                self.emit_inst(restore_id, restore_inst);
                            }

                            return true; // Already emitted all instructions
                        }
                        other => {
                            self.push_typed_diag(
                                t0540_code(),
                                format!("unknown operator {}", other),
                            );
                            return false;
                        }
                    };

                    // Emit the binary operation
                    let operands = if matches!(meta.callee_name.as_str(), "<<" | ">>") {
                        // Shift: dest, cl (implicit RCX)
                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        ops.push(Operand::Reg(dest));
                        ops.push(Operand::Reg(paideia_as_ir::abi::RCX));
                        ops
                    } else {
                        // Non-shift: dest, arg1_dest
                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        ops.push(Operand::Reg(dest));
                        ops.push(Operand::Reg(arg1_dest));
                        ops
                    };

                    let inst = Instruction {
                        mnemonic,
                        operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    };
                    let inst_id = self.alloc_synthetic_id();
                    self.emit_inst(inst_id, inst);
                    true
                } else {
                    self.push_typed_diag(
                        t0540_code(),
                        format!("App {} has no call_sites entry", expr_id.get()),
                    );
                    false
                }
            }
            other => {
                self.push_typed_diag(
                    t0540_code(),
                    format!("unsupported RHS kind {:?}", other),
                );
                false
            }
        }
    }

    /// #1181: extract the call-RHS helper for function calls (non-operator).
    fn emit_call_expr_for_var_assign_rhs(
        &mut self,
        rhs_id: IrNodeId,
        target_name: &str,
        arena: &IrArena,
    ) -> RegId {
        let app_children = arena.children(rhs_id);
        let arg_ids: Vec<IrNodeId> = app_children[1..].to_vec();
        let lambda_id = IrNodeId::new(self.state.current_function)
            .expect("current_function set by walker");
        self.emit_call_expr(lambda_id, target_name.to_string(), &arg_ids, arena);
        paideia_as_ir::abi::RAX
    }

    /// #1200: Emit unsigned DIV/MOD instruction sequence.
    ///
    /// Unsigned division requires the RDX:RAX pair:
    /// - RAX holds the dividend (numerator)
    /// - RDX is the high half (must be zeroed for unsigned)
    /// - Result goes to RAX (for /) or RDX (for %)
    ///
    /// This method:
    /// 1. Saves RDX if it's live (bound in local_bindings and not our dest/divisor)
    /// 2. Saves RAX if it's live (bound in local_bindings and not our dest/divisor)
    /// 3. Spills divisor if it aliases RAX or RDX
    /// 4. Emits: mov rax, dest; xor rdx, rdx; div divisor_reg; mov dest, result_reg
    /// 5. Restores in reverse order
    fn emit_div_unsigned(&mut self, dest: RegId, divisor: RegId, result_reg: RegId) {
        // Check if RDX is live and must be saved.
        let rdx_live = if dest != abi::RDX && divisor != abi::RDX {
            self.state.local_bindings.iter().any(|(_, reg)| reg == abi::RDX)
        } else {
            false
        };

        // Check if RAX is live and must be saved.
        let rax_live = if dest != abi::RAX && divisor != abi::RAX {
            self.state.local_bindings.iter().any(|(_, reg)| reg == abi::RAX)
        } else {
            false
        };

        // Determine if divisor aliases RAX or RDX and needs spilling.
        let divisor_clobbered = divisor == abi::RAX || divisor == abi::RDX;
        let divisor_spill_reg = if divisor_clobbered {
            // Pick a scratch register (R10 or R11).
            if dest != abi::R10 && divisor != abi::R10 {
                Some(abi::R10)
            } else {
                Some(abi::R11)
            }
        } else {
            None
        };

        // Save RDX if live.
        if rdx_live {
            let save_inst = Instruction {
                mnemonic: Mnemonic::Push,
                operands: {
                    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                    ops.push(Operand::Reg(abi::RDX));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let save_id = self.alloc_synthetic_id();
            self.emit_inst(save_id, save_inst);
        }

        // Save RAX if live.
        if rax_live {
            let save_inst = Instruction {
                mnemonic: Mnemonic::Push,
                operands: {
                    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                    ops.push(Operand::Reg(abi::RAX));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let save_id = self.alloc_synthetic_id();
            self.emit_inst(save_id, save_inst);
        }

        // Spill divisor if it aliases RAX or RDX.
        if let Some(spill_reg) = divisor_spill_reg {
            let spill_inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: {
                    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                    ops.push(Operand::Reg(spill_reg));
                    ops.push(Operand::Reg(divisor));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let spill_id = self.alloc_synthetic_id();
            self.emit_inst(spill_id, spill_inst);
        }

        // Move dividend to RAX (skip if dest is already RAX).
        if dest != abi::RAX {
            let mov_inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: {
                    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                    ops.push(Operand::Reg(abi::RAX));
                    ops.push(Operand::Reg(dest));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let mov_id = self.alloc_synthetic_id();
            self.emit_inst(mov_id, mov_inst);
        }

        // Zero RDX (required for unsigned division).
        let xor_inst = Instruction {
            mnemonic: Mnemonic::Xor,
            operands: {
                let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                ops.push(Operand::Reg(abi::RDX));
                ops.push(Operand::Reg(abi::RDX));
                ops
            },
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        let xor_id = self.alloc_synthetic_id();
        self.emit_inst(xor_id, xor_inst);

        // Emit DIV with the (possibly spilled) divisor.
        let actual_divisor = divisor_spill_reg.unwrap_or(divisor);
        let div_inst = Instruction {
            mnemonic: Mnemonic::Div,
            operands: {
                let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                ops.push(Operand::Reg(actual_divisor));
                ops
            },
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        let div_id = self.alloc_synthetic_id();
        self.emit_inst(div_id, div_inst);

        // Move result to dest (skip if dest is already result_reg).
        if dest != result_reg {
            let mov_inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: {
                    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                    ops.push(Operand::Reg(dest));
                    ops.push(Operand::Reg(result_reg));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let mov_id = self.alloc_synthetic_id();
            self.emit_inst(mov_id, mov_inst);
        }

        // Restore RAX if it was saved.
        if rax_live {
            let restore_inst = Instruction {
                mnemonic: Mnemonic::Pop,
                operands: {
                    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                    ops.push(Operand::Reg(abi::RAX));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let restore_id = self.alloc_synthetic_id();
            self.emit_inst(restore_id, restore_inst);
        }

        // Restore RDX if it was saved.
        if rdx_live {
            let restore_inst = Instruction {
                mnemonic: Mnemonic::Pop,
                operands: {
                    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                    ops.push(Operand::Reg(abi::RDX));
                    ops
                },
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let restore_id = self.alloc_synthetic_id();
            self.emit_inst(restore_id, restore_inst);
        }
    }
}

/// #1181: operator lexemes for dispatch in emit_var_assign_expr_to_reg
pub(crate) fn is_operator_callee(s: &str) -> bool {
    matches!(s,
        "|" | "&" | "^" | "<<" | ">>" |
        "+" | "-" | "*" | "/" | "%" |
        "~" | "!"
    )
}

/// #1196: authoritative operator-lexeme lookup for an App IR node.
/// Returns the lexeme string from call_sites() if the App has an entry and
/// the lexeme is a known operator.
pub(crate) fn operator_lexeme_of<'a>(
    arena: &'a IrArena, app_id: IrNodeId,
) -> Option<&'a str> {
    let lexeme = arena.call_sites().get(app_id)?.callee_name.as_str();
    is_operator_callee(lexeme).then_some(lexeme)
}
