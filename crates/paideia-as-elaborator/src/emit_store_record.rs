//! Store + record-constructor lowerers.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Covers
//! `visit_store` (l-value assignment via `MemSib`) and `visit_record_cons`
//! (Phase 6 m3-004 cap-mint record constructor for the 4-field all-u64
//! capability descriptor shape).

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi};

use crate::emit_walker::EmitWalker;

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
            self.diagnostics.push(format!(
                "Store node {} has {} children; expected 3",
                store_id.get(),
                children.len()
            ));
            return;
        }

        let addr_id = children[0];
        let _index_or_unused_id = children[1];
        let value_id = children[2];

        let addr_node = arena.get(addr_id);
        let value_node = arena.get(value_id);

        if addr_node.map(|n| n.kind) != Some(IrKind::Var) {
            self.diagnostics.push(format!(
                "Store addr must be Var; got {:?}",
                addr_node.map(|n| n.kind)
            ));
            return;
        }

        if value_node.map(|n| n.kind) != Some(IrKind::Var) {
            self.diagnostics.push(format!(
                "Store value must be Var; got {:?}",
                value_node.map(|n| n.kind)
            ));
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
            self.diagnostics.push(format!(
                "Store node {} has {} children; expected 3",
                store_id.get(),
                children.len()
            ));
            return;
        }

        let lhs_id = children[0];
        let _op_id = children[1];
        let rhs_id = children[2];

        // Verify both children are Var nodes
        let lhs_node = arena.get(lhs_id);
        let rhs_node = arena.get(rhs_id);

        if lhs_node.map(|n| n.kind) != Some(IrKind::Var) {
            self.diagnostics.push(format!(
                "Store (var_assign) LHS must be Var; got {:?}",
                lhs_node.map(|n| n.kind)
            ));
            return;
        }

        if rhs_node.map(|n| n.kind) != Some(IrKind::Var) {
            self.diagnostics.push(format!(
                "Store (var_assign) RHS must be Var; got {:?}",
                rhs_node.map(|n| n.kind)
            ));
            return;
        }

        // Resolve LHS name
        let lhs_name = match arena.binding_names().get(lhs_id) {
            Some(name) => name.to_string(),
            None => {
                self.diagnostics.push(format!(
                    "Store (var_assign) LHS Var {} has no binding name",
                    lhs_id.get()
                ));
                return;
            }
        };

        // Check if LHS is shadowed by a local binding (error case)
        if self.state.local_bindings.contains(&lhs_name) {
            self.diagnostics.push(format!(
                "T0518: variable {} is shadowed by a local binding; cannot assign to module symbol",
                lhs_name
            ));
            return;
        }

        // Verify LHS is a module symbol (not a function parameter)
        if arena.symbols().lookup_by_name(&lhs_name).is_none() {
            self.diagnostics.push(format!(
                "T0518: variable {} is not a module symbol; var_assign requires module-level let mut",
                lhs_name
            ));
            return;
        }

        // Resolve RHS name and register
        let rhs_name = match arena.binding_names().get(rhs_id) {
            Some(name) => name.to_string(),
            None => {
                self.diagnostics.push(format!(
                    "Store (var_assign) RHS Var {} has no binding name",
                    rhs_id.get()
                ));
                return;
            }
        };

        let src_reg = match self.state.local_bindings.get(&rhs_name) {
            Some(reg) => reg,
            None => {
                self.diagnostics.push(format!(
                    "T0518: var_assign RHS {} not found in local bindings; non-register sources not yet supported",
                    rhs_name
                ));
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
                self.diagnostics.push(format!(
                    "T0518: RecordCons node {} has no layout entry (unsupported shape in Phase 6)",
                    record_cons_id.get()
                ));
                return;
            }
        };

        // Look up the finalised layout for this type.
        let layout = match self.state.record_layout(type_id) {
            Some(l) => l,
            None => {
                // Layout not finalised → unsupported
                self.diagnostics.push(format!(
                    "T0518: RecordCons node {} type {} not finalised (unsupported shape in Phase 6)",
                    record_cons_id.get(),
                    type_id.0
                ));
                return;
            }
        };

        // Phase 6 m3-004: Accept only the cap-mint shape:
        // - Exactly 4 fields
        // - All u64 (size 8 each)
        // - Offsets [0, 8, 16, 24], total size 32, align 8
        if layout.fields.len() != 4 {
            self.diagnostics.push(format!(
                "T0518: RecordCons node {} has {} fields; cap-mint requires 4 (unsupported shape in Phase 6)",
                record_cons_id.get(),
                layout.fields.len()
            ));
            return;
        }

        for (i, field) in layout.fields.iter().enumerate() {
            if field.size != 8 {
                self.diagnostics.push(format!(
                    "T0518: RecordCons node {} field {} has size {}; cap-mint requires u64 (size 8) (unsupported shape in Phase 6)",
                    record_cons_id.get(),
                    i,
                    field.size
                ));
                return;
            }
            let expected_offset = (i as u64) * 8;
            if field.offset != expected_offset {
                self.diagnostics.push(format!(
                    "T0518: RecordCons node {} field {} has offset {}; cap-mint requires offset {} (unsupported shape in Phase 6)",
                    record_cons_id.get(),
                    i,
                    field.offset,
                    expected_offset
                ));
                return;
            }
        }

        // Shape is valid cap-mint. Get field values from children.
        let children = arena.children(record_cons_id);
        if children.len() != 4 {
            self.diagnostics.push(format!(
                "T0518: RecordCons node {} has {} children; cap-mint requires 4 (unsupported shape in Phase 6)",
                record_cons_id.get(),
                children.len()
            ));
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
                let inst = Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                };

                // Virtual ID: record_cons_id * 10 + field_idx to sort in order.
                let inst_id = IrNodeId::new(record_cons_id.get() * 10 + field_idx as u32)
                    .expect("virtual id");
                // TODO(step5-encoder): encode_mov does not yet handle
                // [MemSib, Imm64] (only MovSized does), so estimated_bytes
                // would return 0 here. Keep the hardcoded literal until
                // encode_mov gains this arm. Bytes: 48 C7 47 NN 00 00 00 00
                // = 8 bytes for small offsets.
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
                };

                // Virtual ID: record_cons_id * 10 + field_idx to sort in order.
                let inst_id = IrNodeId::new(record_cons_id.get() * 10 + field_idx as u32)
                    .expect("virtual id");
                self.emit_inst(inst_id, inst);
            }
        }
    }
}
