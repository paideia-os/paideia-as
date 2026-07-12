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
use paideia_as_diagnostics::{DiagnosticCode, Category, Severity};

use crate::emit_walker::EmitWalker;

/// Helper to construct T0516 diagnostic code.
fn t0516_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 516)
        .expect("T0516 is within valid T range")
}

/// Helper to construct T0529 diagnostic code.
fn t0529_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 529)
        .expect("T0529 is within valid T range")
}

/// Helper to construct T0530 diagnostic code.
fn t0530_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 530)
        .expect("T0530 is within valid T range")
}

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
    ///
    /// Refactor 2026-07-07 Step 6: this is now a thin wrapper over
    /// `visit_field_access_with_reg` with `dest_reg = abi::RAX`. Retires the
    /// ~100-line near-clone that had drifted vs the parametric version and
    /// was a classic "update one, forget the other" hazard.
    pub(crate) fn visit_field_access(&mut self, field_access_id: IrNodeId, arena: &IrArena) {
        self.visit_field_access_with_reg(field_access_id, abi::RAX, arena);
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
        let value_id = children[2];

        // Mark this FieldAccess as handled so emit_walker.walk_inner doesn't double-visit it.
        self.state.mark_field_access_handled(field_access_id.get());

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

        // Extract field layout data and drop the record_layout reference to avoid borrow conflicts.
        let field_offset = field_layout.offset;
        let field_size = field_layout.size;
        let field_signed = field_layout.signed;

        // Dispatch on field size to emit the appropriate width.
        // Signedness is IGNORED for stores (we write N bytes regardless).
        let width = match field_size {
            1 => IntWidth::W8,
            2 => IntWidth::W16,
            4 => IntWidth::W32,
            8 => IntWidth::W64,
            _ => {
                self.diagnostics.push(format!(
                    "Unsupported field size {} for field store at offset {}",
                    field_size, field_offset
                ));
                return;
            }
        };

        // Phase 17 m6-b: Check if this is a module-level record field write.
        // Get the FieldAccess's receiver to check if it's a module-level symbol.
        let fa_children = arena.children(field_access_id);
        let receiver_id = match fa_children.first() {
            Some(&id) => id,
            None => {
                self.diagnostics.push(format!(
                    "FieldAccess node {} has no receiver child",
                    field_access_id.get()
                ));
                return;
            }
        };

        let receiver_node = match arena.get(receiver_id) {
            Some(node) => node,
            None => return,
        };

        // Check if receiver is a Var and it's a module-level symbol (not local)
        if receiver_node.kind == IrKind::Var {
            if let Some(name) = arena.binding_names().get(receiver_id) {
                if !self.state.local_bindings.contains(name)
                    && arena.symbols().lookup_by_name(name).is_some()
                {
                    // This is a module-level record: emit RIP-relative write
                    // Materialize the RHS value into RAX
                    if let Some(value_node) = arena.get(value_id) {
                        if value_node.kind == IrKind::Literal {
                            // Extract immediate value from the literal
                            if let Some(imm_val) = arena.literal_values().get(value_id) {
                                // Emit width-appropriate immediate load into RAX
                                // For W32 fields, use MovSized{W32} to emit 5-byte mov eax, imm32
                                // For W64 fields, use generic Mov to emit 10-byte movabs rax, imm64
                                let mov_id = IrNodeId::new(store_id.get() * 2).expect("mov instr virtual id");
                                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                operands.push(Operand::Reg(abi::RAX));
                                operands.push(Operand::Imm64(imm_val));

                                let (mnemonic, _est_size) = match field_size {
                                    4 => (Mnemonic::MovSized { width: IntWidth::W32 }, 5),
                                    8 => (Mnemonic::Mov, 10),
                                    _ => {
                                        self.push_typed_diag(
                                            t0530_code(),
                                            format!(
                                                "unsupported field size {} for module-level field write",
                                                field_size
                                            ),
                                        );
                                        return;
                                    }
                                };

                                let mov_inst = Instruction {
                                    mnemonic,
                                    operands,
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                            emission_order: 0,
        };

                                self.emit_inst(mov_id, mov_inst);

                                // Emit RIP-relative write using a different virtual ID
                                let write_id = IrNodeId::new(store_id.get() * 2 + 1).expect("write instr virtual id");
                                self.emit_mem_write_via_rip_sym(
                                    write_id,
                                    abi::RAX,
                                    name.to_string(),
                                    field_offset as i32,
                                    field_size,
                                    field_signed,
                                );
                                return;
                            }
                        }
                    }
                }
            }
        }

        // Fallthrough: unsafe deref case, e.g. `(*p).field = value`.
        //
        // #1146 follow-up: base and value operands used to be hardcoded to
        // RDI/RDX. That only produced correct code by coincidence — when
        // the pointer receiver happened to be the function's first
        // parameter (RDI) and the value happened to be its third (RDX).
        // For the common two-argument shape `fn(p: *T, v: U) -> (*p).f = v`
        // the value lives in RSI, not RDX, so the hardcoded operand wrote
        // the wrong register's contents into the field. Resolve both
        // registers from local_bindings instead, mirroring
        // visit_var_assign's RHS resolution.
        let base_reg = match receiver_node.kind {
            IrKind::Deref => arena
                .children(receiver_id)
                .first()
                .and_then(|&ptr_id| arena.binding_names().get(ptr_id))
                .and_then(|name| self.state.local_bindings.get(name)),
            _ => None,
        };
        let base_reg = match base_reg {
            Some(reg) => reg,
            None => {
                self.diagnostics.push(format!(
                    "field-assign pointer receiver for Store {} not found in local bindings",
                    store_id.get()
                ));
                return;
            }
        };

        let value_reg = match arena.get(value_id).map(|n| n.kind) {
            Some(IrKind::Var) => {
                let resolved = arena
                    .binding_names()
                    .get(value_id)
                    .and_then(|name| self.state.local_bindings.get(name));
                match resolved {
                    Some(reg) => reg,
                    None => {
                        self.diagnostics.push(format!(
                            "field-assign value Var for Store {} not found in local bindings",
                            store_id.get()
                        ));
                        return;
                    }
                }
            }
            // Preserve the prior fallback for non-Var RHS shapes (e.g. an
            // already-materialized scratch result landed in RDX upstream).
            _ => abi::RDX,
        };

        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::MemSib {
            base: base_reg,                               // resolved pointer register
            index: None,                                  // no index
            scale: paideia_as_ir::instruction::Scale::X1, // ignored when no index
            disp: field_offset as i32,                    // field offset
        });
        operands.push(Operand::Reg(value_reg)); // resolved value register

        let inst = Instruction {
            mnemonic: Mnemonic::MovSized { width },
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
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
            // Not a dereference. Check if it's a Var (which might be owned by visit_field_assign).
            // For now, silently skip FieldAccess(Var) patterns — they're handled by visit_field_assign
            // in Store contexts. Other non-Deref shapes are truly unsupported.
            if record_value_node.kind == IrKind::Var {
                // This is FieldAccess(Var), which may be owned by a Store node (handled by
                // visit_field_assign). Silently skip to avoid double-processing.
                return;
            }
            // Other non-Deref patterns are not yet supported.
            self.push_typed_diag(
                t0516_code(),
                format!(
                    "field access on non-Deref shape (kind={:?})",
                    record_value_node.kind
                ),
            );
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
            self.push_typed_diag(
                t0516_code(),
                format!(
                    "field access on non-Var shape (kind={:?})",
                    ptr_node.kind
                ),
            );
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
                    emission_order: 0,
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
                    emission_order: 0,
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
                    emission_order: 0,
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
                            emission_order: 0,
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
                            emission_order: 0,
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
                            emission_order: 0,
        };

                self.emit_inst(node_id, inst);
            }
            _ => {
                // u8/u16/i8/i16/i32: not exercised by the fixture; emit T0529 typed diagnostic.
                self.push_typed_diag(
                    t0529_code(),
                    format!(
                        "field read with size={}, signed={} not yet lowered",
                        size, signed
                    ),
                );
            }
        }
    }

    /// Phase 17 m6-b: Emit field store via RIP-relative symbol reference (write-side mirror).
    ///
    /// KNOWN LIMITATION: The encoder currently only supports [MemRipRelSym, Reg] patterns
    /// with generic Mov mnemonic, and ONLY for W64 (hardcoded REX.W in encoder).
    /// For W32 fields, this still emits the W64 form (48 89 ...) which is a silent data-corruption bug.
    ///
    /// Proper fix requires encoder enhancement to support width-aware [MemRipRelSym, Reg]
    /// with MovSized mnemonic. Encoder gap: crates/paideia-as-encoder/src/encode_instruction.rs
    /// encode_mov_sized() function lacks [MemRipRelSym, Reg] operand pattern.
    ///
    /// Workaround: use generic Mov (which compiles but produces W64 encoding for all sizes).
    pub(crate) fn emit_mem_write_via_rip_sym(
        &mut self,
        node_id: IrNodeId,
        src_reg: RegId,
        sym_name: String,
        addend: i32,
        size: u8,
        _signed: bool,
    ) {
        // PA-R17-006b: Dispatch on size and emit width-appropriate MovSized.
        // - W32 (4 bytes): MovSized{W32} emits 6-byte 89 05 + disp32 (no REX.W)
        // - W64 (8 bytes): MovSized{W64} emits 7-byte 48 89 05 + disp32 (with REX.W)
        match size {
            4 => {
                // u32: use MovSized{W32} to emit 6-byte mov [rip+...], eax (no REX.W)
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::MemRipRelSym { name: sym_name, addend });
                operands.push(Operand::Reg(src_reg));

                let inst = Instruction {
                    mnemonic: Mnemonic::MovSized { width: IntWidth::W32 },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                            emission_order: 0,
        };

                self.emit_inst(node_id, inst);
            }
            8 => {
                // u64: use MovSized{W64} to emit 7-byte mov [rip+...], rax (with REX.W)
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::MemRipRelSym { name: sym_name, addend });
                operands.push(Operand::Reg(src_reg));

                let inst = Instruction {
                    mnemonic: Mnemonic::MovSized { width: IntWidth::W64 },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                            emission_order: 0,
        };

                self.emit_inst(node_id, inst);
            }
            _ => {
                // u8/u16/i8/i16/i32: not exercised by the fixture; emit diagnostic.
                self.push_typed_diag(
                    t0529_code(),
                    format!(
                        "field write with size={} not yet lowered",
                        size
                    ),
                );
            }
        }
    }
}
