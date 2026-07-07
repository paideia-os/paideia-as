//! Field-access + field-assign emit family.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Hosts every
//! path that walks `FieldAccess(Deref(...))` shapes and the corresponding
//! store side (field-assign). Also owns the `emit_field_access_*_reg`
//! helpers and the `emit_widening_load` dispatcher used by their callers.
//!
//! All methods run as `impl EmitWalker` and share walker state via
//! `pub(crate)` visibility on the walker's fields and helper methods.

use paideia_as_ir::instruction::{
    EncodingHint, Instruction, IntWidth, Mnemonic, Operand, RegId,
};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi};

use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// Phase 6 m3-002: Emit field access lowering for (*p).field shape.
    ///
    /// Handles pattern: FieldAccess(Deref(Var(p))) where p is the function's first argument.
    /// Determines field offset and size from the record layout, then emits:
    /// - mov rax, [rdi + offset] for u64/*T fields (3 bytes: 48 8b 47 NN or 48 8b 87 NNNNNNNN)
    /// - mov eax, [rdi + offset] for u32 fields (3-6 bytes)
    /// - movzx rax, byte [rdi + offset] for u8 fields (4-7 bytes)
    ///
    /// If the pattern is not Deref(Var(arg0)), emits T0516 diagnostic and skips emission.
    pub(crate) fn visit_field_access(&mut self, field_access_id: IrNodeId, arena: &IrArena) {
        // Get the field access info from the side-table.
        let field_info = match arena.field_access_info().get(field_access_id) {
            Some(info) => info,
            None => {
                // No field access info registered; skip (may happen before elaboration).
                return;
            }
        };

        // Get the FieldAccess node's single child (the record value).
        let children = arena.children(field_access_id);
        let record_value_id = match children.first() {
            Some(&id) => id,
            None => {
                // No child; malformed FieldAccess node.
                self.diagnostics.push(format!(
                    "FieldAccess node {} has no child",
                    field_access_id.get()
                ));
                return;
            }
        };

        // Check that the record value is a Deref.
        let record_value_node = match arena.get(record_value_id) {
            Some(node) => node,
            None => return,
        };

        if record_value_node.kind != IrKind::Deref {
            // Not a dereference; pattern not supported yet.
            self.diagnostics.push(format!(
                "T0516: field access on non-Deref shape (kind={:?})",
                record_value_node.kind
            ));
            return;
        }

        // Get the child of Deref (the pointer being dereferenced).
        let deref_children = arena.children(record_value_id);
        let ptr_id = match deref_children.first() {
            Some(&id) => id,
            None => {
                self.diagnostics
                    .push(format!("Deref node {} has no child", record_value_id.get()));
                return;
            }
        };

        // Check that the pointer is a Var.
        let ptr_node = match arena.get(ptr_id) {
            Some(node) => node,
            None => return,
        };

        if ptr_node.kind != IrKind::Var {
            // Not a variable; pattern not supported yet.
            self.diagnostics.push(format!(
                "T0516: field access on non-Var shape (kind={:?})",
                ptr_node.kind
            ));
            return;
        }

        // For now, we only support first argument (rdi).
        // Ideally, we'd track which argument this Var refers to, but we don't have that info yet.
        // As a simplification for this phase, we assume all Vars are the first argument.

        // Look up the record layout to get field offset and size.
        let record_layout = match self.state.record_layout(field_info.type_id) {
            Some(layout) => layout,
            None => {
                self.diagnostics.push(format!(
                    "No record layout found for type {}",
                    field_info.type_id.0
                ));
                return;
            }
        };

        // Get the field layout.
        let field_index = field_info.field_index as usize;
        let field_layout = match record_layout.fields.get(field_index) {
            Some(layout) => layout,
            None => {
                self.diagnostics.push(format!(
                    "Field index {} out of bounds for record type {}",
                    field_index, field_info.type_id.0
                ));
                return;
            }
        };

        // Route through the unified width dispatch. RAX is the fixed
        // destination for the original visit_field_access path.
        self.emit_widening_load(
            field_access_id,
            field_layout.offset as i32,
            abi::RAX, // rax
            field_layout.size,
            field_layout.signed,
        );
    }

    // The three original RAX/RDI-hardcoded field-access helpers
    // (emit_field_access_mov_sized, emit_field_access_movzx,
    // emit_field_access_movsx) were retired by Step 4 of the emit-side
    // refactor: their sole callers now route through emit_widening_load,
    // which dispatches on (size, signed) once and delegates to the
    // dest-register-parametric _reg variants below.

    /// pa-r17-006 (#984): Emit field assignment lowering for (*p).field = value shape.
    ///
    /// Expects Store IR children:
    /// - children[0] = IrKind::FieldAccess node
    /// - children[2] = value var (or literal)
    ///
    /// Extracts field offset and size from record_layouts, then emits
    /// mov [base + offset], src with width-appropriate opcode.
    pub(crate) fn visit_field_assign(&mut self, store_id: IrNodeId, arena: &IrArena) {
        let children = arena.children(store_id);
        if children.len() != 3 {
            self.diagnostics.push(format!(
                "Store node {} has {} children; expected 3",
                store_id.get(),
                children.len()
            ));
            return;
        }

        let field_access_id = children[0];
        let _index_or_unused_id = children[1];
        let _value_id = children[2];

        // Get the field access info from the side-table.
        let field_info = match arena.field_access_info().get(field_access_id) {
            Some(info) => info,
            None => {
                self.diagnostics.push(format!(
                    "Store field_access node {} has no FieldAccessInfo",
                    field_access_id.get()
                ));
                return;
            }
        };

        // Get the record layout to extract field offset and size.
        let record_layout = match self.state.record_layout(field_info.type_id) {
            Some(layout) => layout,
            None => {
                self.diagnostics.push(format!(
                    "No record layout found for type {}",
                    field_info.type_id.0
                ));
                return;
            }
        };

        // Get the field layout.
        let field_index = field_info.field_index as usize;
        let field_layout = match record_layout.fields.get(field_index) {
            Some(layout) => layout,
            None => {
                self.diagnostics.push(format!(
                    "Field index {} out of bounds for record type {}",
                    field_index, field_info.type_id.0
                ));
                return;
            }
        };

        // Dispatch on field size to emit the appropriate width.
        // Signedness is IGNORED for stores (we write N bytes regardless).
        let width = match field_layout.size {
            1 => IntWidth::W8,
            2 => IntWidth::W16,
            4 => IntWidth::W32,
            8 => IntWidth::W64,
            _ => {
                self.diagnostics.push(format!(
                    "Unsupported field size {} for field store at offset {}",
                    field_layout.size, field_layout.offset
                ));
                return;
            }
        };

        // Emit MovSized with operands [MemSib{base: RDI, disp: offset}, Reg(RDX)]
        // Following the same convention as visit_store: base=RDI (abi::RDI), source=RDX (abi::RDX)
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::MemSib {
            base: abi::RDI,                               // rdi (pointer)
            index: None,                                  // no index
            scale: paideia_as_ir::instruction::Scale::X1, // ignored when no index
            disp: field_layout.offset as i32,             // field offset
        });
        operands.push(Operand::Reg(abi::RDX)); // rdx (value, source)

        let inst = Instruction {
            mnemonic: Mnemonic::MovSized { width },
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(store_id, inst);
    }

    /// Phase 6 m3-003: Emit field access with a specified scratch register.
    ///
    /// Generalizes visit_field_access to support arbitrary destination registers.
    /// Used by visit_let_field_access to emit field bindings to RAX, RCX, RDX, R8
    /// in sequence.
    pub(crate) fn visit_field_access_with_reg(
        &mut self,
        field_access_id: IrNodeId,
        dest_reg: RegId,
        arena: &IrArena,
    ) {
        // Get the field access info from the side-table.
        let field_info = match arena.field_access_info().get(field_access_id) {
            Some(info) => info,
            None => {
                // No field access info registered; skip (may happen before elaboration).
                return;
            }
        };

        // Get the FieldAccess node's single child (the record value).
        let children = arena.children(field_access_id);
        let record_value_id = match children.first() {
            Some(&id) => id,
            None => {
                // No child; malformed FieldAccess node.
                self.diagnostics.push(format!(
                    "FieldAccess node {} has no child",
                    field_access_id.get()
                ));
                return;
            }
        };

        // Check that the record value is a Deref.
        let record_value_node = match arena.get(record_value_id) {
            Some(node) => node,
            None => return,
        };

        if record_value_node.kind != IrKind::Deref {
            // Not a dereference; pattern not supported yet.
            self.diagnostics.push(format!(
                "T0516: field access on non-Deref shape (kind={:?})",
                record_value_node.kind
            ));
            return;
        }

        // Get the child of Deref (the pointer being dereferenced).
        let deref_children = arena.children(record_value_id);
        let ptr_id = match deref_children.first() {
            Some(&id) => id,
            None => {
                self.diagnostics
                    .push(format!("Deref node {} has no child", record_value_id.get()));
                return;
            }
        };

        // Check that the pointer is a Var.
        let ptr_node = match arena.get(ptr_id) {
            Some(node) => node,
            None => return,
        };

        if ptr_node.kind != IrKind::Var {
            // Not a variable; pattern not supported yet.
            self.diagnostics.push(format!(
                "T0516: field access on non-Var shape (kind={:?})",
                ptr_node.kind
            ));
            return;
        }

        // Look up the record layout to get field offset and size.
        let record_layout = match self.state.record_layout(field_info.type_id) {
            Some(layout) => layout,
            None => {
                self.diagnostics.push(format!(
                    "No record layout found for type {}",
                    field_info.type_id.0
                ));
                return;
            }
        };

        // Get the field layout.
        let field_index = field_info.field_index as usize;
        let field_layout = match record_layout.fields.get(field_index) {
            Some(layout) => layout,
            None => {
                self.diagnostics.push(format!(
                    "Field index {} out of bounds for record type {}",
                    field_index, field_info.type_id.0
                ));
                return;
            }
        };

        // Route through the unified width dispatch.
        self.emit_widening_load(
            field_access_id,
            field_layout.offset as i32,
            dest_reg,
            field_layout.size,
            field_layout.signed,
        );
    }

    /// Emit a mov instruction with sized load to a specified register: mov r64/r32, [rdi + offset]
    ///
    /// Phase 13 m6-001: Handles u32, u64, and i64 field loads to an arbitrary register.
    /// Unified `(size, signed)` width-dispatch for field-shaped memory loads.
    /// Every emitter that reads a value from `[rdi + offset]` and dispatches on
    /// its declared size (u8/u16/u32/u64 or i8/i16/i32/i64) routes through
    /// this method.
    ///
    /// Retires three copy-pasted 8-arm matches previously duplicated in
    /// `visit_field_access`, `visit_field_access_with_reg`, and
    /// `lower_pattern`'s `Simple` leaf.
    ///
    /// Semantics:
    /// * u8/u16     → `emit_field_access_movzx_reg` (zero-extend)
    /// * u32        → `emit_field_access_mov_sized_reg` (W32, no REX.W)
    /// * u64 / *T   → `emit_field_access_mov_sized_reg` (W64, REX.W)
    /// * i8/i16/i32 → `emit_field_access_movsx_reg` (sign-extend / MOVSXD)
    /// * i64        → `emit_field_access_mov_sized_reg` (W64)
    ///
    /// Unsupported sizes push a diagnostic string and emit nothing.
    ///
    /// Note: base register is still RDI in the underlying primitives (a
    /// separate refactoring step will thread `base_reg` through).
    pub(crate) fn emit_widening_load(
        &mut self,
        node_id: IrNodeId,
        offset: i32,
        dest_reg: RegId,
        size: u8,
        signed: bool,
    ) {
        match (size, signed) {
            (1, false) => self.emit_field_access_movzx_reg(node_id, offset, dest_reg, 1),
            (2, false) => self.emit_field_access_movzx_reg(node_id, offset, dest_reg, 2),
            (4, false) => {
                self.emit_field_access_mov_sized_reg(node_id, offset, dest_reg, IntWidth::W32)
            }
            (8, false) => {
                self.emit_field_access_mov_sized_reg(node_id, offset, dest_reg, IntWidth::W64)
            }
            (1, true) => self.emit_field_access_movsx_reg(node_id, offset, dest_reg, 1),
            (2, true) => self.emit_field_access_movsx_reg(node_id, offset, dest_reg, 2),
            (4, true) => self.emit_field_access_movsx_reg(node_id, offset, dest_reg, 4),
            (8, true) => {
                self.emit_field_access_mov_sized_reg(node_id, offset, dest_reg, IntWidth::W64)
            }
            _ => {
                self.diagnostics.push(format!(
                    "Unsupported field: size={}, signed={} at node {}",
                    size, signed, node_id.get()
                ));
            }
        }
    }

    pub(crate) fn emit_field_access_mov_sized_reg(
        &mut self,
        field_access_id: IrNodeId,
        offset: i32,
        dest_reg: RegId,
        width: IntWidth,
    ) {
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::Reg(dest_reg)); // destination register
        operands.push(Operand::MemSib {
            base: abi::RDI, // rdi (first argument)
            index: None,
            scale: paideia_as_ir::instruction::Scale::X1,
            disp: offset,
        });

        let inst = Instruction {
            mnemonic: Mnemonic::MovSized { width },
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(field_access_id, inst);
    }

    /// Emit a movzx instruction to a specified register: movzx <reg>, [rdi + offset]
    ///
    /// Phase 13 m6-001: Handles u8 and u16 field loads to an arbitrary register.
    pub(crate) fn emit_field_access_movzx_reg(
        &mut self,
        field_access_id: IrNodeId,
        offset: i32,
        dest_reg: RegId,
        src_width: u8,
    ) {
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::Reg(dest_reg)); // destination register
        operands.push(Operand::MemSib {
            base: abi::RDI, // rdi
            index: None,
            scale: paideia_as_ir::instruction::Scale::X1,
            disp: offset,
        });

        let inst = Instruction {
            mnemonic: Mnemonic::Movzx,
            operands,
            encoding_hint: Some(EncodingHint { opcode: 0x0F, operand_size: src_width }),
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(field_access_id, inst);
    }

    /// Emit a movsx instruction to a specified register: movsx <reg>, [rdi + offset]
    ///
    /// Phase 13 m6-001: Handles i8, i16, and i32 field loads to an arbitrary register.
    pub(crate) fn emit_field_access_movsx_reg(
        &mut self,
        field_access_id: IrNodeId,
        offset: i32,
        dest_reg: RegId,
        src_width: u8,
    ) {
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::Reg(dest_reg)); // destination register
        operands.push(Operand::MemSib {
            base: abi::RDI, // rdi
            index: None,
            scale: paideia_as_ir::instruction::Scale::X1,
            disp: offset,
        });

        // Opcode varies by source width: 0x0F for 1/2-byte, 0x63 for 4-byte
        let opcode = if src_width == 4 { 0x63 } else { 0x0F };

        let inst = Instruction {
            mnemonic: Mnemonic::Movsx,
            operands,
            encoding_hint: Some(EncodingHint { opcode, operand_size: src_width }),
            byte_offset_in_text: None,
            mode: self.current_mode(),
        };

        self.emit_inst(field_access_id, inst);
    }

    /// pa-r17-005-e: Emit global record-field read via RIP-relative symbol.
    ///
    /// Emits `mov <dest_reg>, [rip + sym + addend]` for module-level records.
    /// Handles u32 (W32) and u64 (W64) sized loads; other sizes push a diagnostic.
    ///
    /// For u8/u16/i8/i16/i32, the fixture does not exercise these paths, so
    /// a diagnostic is emitted as a placeholder. This method is called from
    /// the visit_lambda arm for FieldAccess(Var(...)) tail-position reads.
    pub(crate) fn emit_mem_read_via_rip_sym(
        &mut self,
        node_id: IrNodeId,
        dest_reg: RegId,
        sym_name: String,
        addend: i32,
        size: u8,
        signed: bool,
    ) {
        match (size, signed) {
            (4, false) => {
                // u32: emit movl (W32 without REX.W)
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::Reg(dest_reg));
                operands.push(Operand::MemRipRelSym { name: sym_name, addend });

                let inst = Instruction {
                    mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                };

                self.emit_inst(node_id, inst);
            }
            (8, false) => {
                // u64: emit movq (W64 with REX.W)
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::Reg(dest_reg));
                operands.push(Operand::MemRipRelSym { name: sym_name, addend });

                let inst = Instruction {
                    mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                };

                self.emit_inst(node_id, inst);
            }
            (8, true) => {
                // i64: same as u64 (W64 with REX.W)
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::Reg(dest_reg));
                operands.push(Operand::MemRipRelSym { name: sym_name, addend });

                let inst = Instruction {
                    mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                };

                self.emit_inst(node_id, inst);
            }
            _ => {
                // u8/u16/i8/i16/i32: not exercised by the fixture; emit diagnostic.
                self.diagnostics.push(format!(
                    "T0517: emit_mem_read_via_rip_sym with size={}, signed={} not supported; deferred to future phase",
                    size, signed
                ));
            }
        }
    }
}
