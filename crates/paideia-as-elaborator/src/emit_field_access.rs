//! Field-access + field-assign emit family.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Hosts every
//! path that walks `FieldAccess(Deref(...))` shapes and the corresponding
//! store side (field-assign). Also owns the `emit_field_access_*_reg`
//! helpers and the `emit_widening_load` dispatcher used by their callers.
//!
//! All methods run as `impl EmitWalker` and share walker state via
//! `pub(crate)` visibility on the walker's fields and helper methods.

use paideia_as_ast::Endianness;
use paideia_as_ir::instruction::{
    EncodingHint, Instruction, IntWidth, Mnemonic, Operand, RegId,
};
use paideia_as_ir::record_layout::RecordTypeId;
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

/// Helper to construct T0564 diagnostic code.
fn t0564_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 564)
        .expect("T0564 is within valid T range")
}

/// Helper to construct T0565 diagnostic code.
fn t0565_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 565)
        .expect("T0565 is within valid T range")
}

/// paideia-as#1508 / paideia-as#1560 (PAS-DEBT-B2-015): sign-extended
/// narrow scalar (i8/i16/i32) annotated with `@endian(be)` needs a
/// byte-swap on the pre-sign-extension value AND a re-sign-extend of
/// the swapped low bytes. Wave 56 (v0.36.80) scoped the byte-swap
/// emission to unsigned widths (u8/u16/u32/u64) plus i64 (whose native
/// two's-complement representation doesn't require post-swap re-
/// extension) and diagnosed signed narrow (i16/i32) with T0567 to
/// prevent silent miscompile. Wave 57 (v0.36.81, paideia-as#1560)
/// lands the three-instruction `mov-low; bswap; movsx-widen` recipe
/// for the load side and a matching `mov r11, v; bswap-low r11;
/// narrow-store` recipe for the store side, retiring T0567 for the
/// signed-narrow case. The diagnostic remains as a defensive arm on
/// truly unsupported widths (sizes other than 1/2/4/8) in
/// `emit_bswap_low_bits`.
fn t0567_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 567)
        .expect("T0567 is within valid T range")
}

/// Helper to construct U1643 diagnostic code (Malformed field-shape IR node).
/// Slice A4 mint — covers Store/FieldAccess/Deref shape violations.
fn u1643_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1643)
        .expect("U1643 is within valid U range")
}

/// Helper to construct U1644 diagnostic code (FieldAccessInfo side-table miss).
fn u1644_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1644)
        .expect("U1644 is within valid U range")
}

/// Helper to construct U1645 diagnostic code (Record layout missing).
fn u1645_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1645)
        .expect("U1645 is within valid U range")
}

/// Helper to construct U1646 diagnostic code (Field index OOB).
fn u1646_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1646)
        .expect("U1646 is within valid U range")
}

/// Helper to construct U1647 diagnostic code (Field-assign operand missing from local_bindings).
fn u1647_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1647)
        .expect("U1647 is within valid U range")
}

/// PAS-DEBT-B3-006 debugger follow-up: Field-access pointer receiver is
/// bound but in a non-register home (Stack / Env / Closure / RegPair).
/// The current emit path only knows how to load from `[base_reg +
/// offset]`; non-register homes need a distinct emission (spill-thaw
/// through a scratch reg, or a dedicated `[rbp + rbp_off]` load).
/// Refusing here beats silently emitting `[rdi + offset]` and storing
/// through the wrong pointer.
fn u1648_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1648)
        .expect("U1648 is within valid U range")
}

/// Helper to construct T0540 diagnostic code (MVP scope guard for module-field writes).
fn t0540_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 540)
        .expect("T0540 is within valid T range")
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
            self.push_typed_diag(
                u1643_code(),
                format!(
                    "Store node {} has {} children; expected 3",
                    store_id.get(),
                    children.len()
                ),
            );
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
                // Issue #1182: fall back to module-field write path if the
                // FieldAccess was recorded as a module-qualified reference.
                if let Some(field_name) = arena.module_field_refs().get(field_access_id) {
                    let name_owned = field_name.to_string();
                    self.emit_module_field_write(store_id, field_access_id, name_owned, arena);
                    return;
                }
                self.push_typed_diag(
                    u1644_code(),
                    format!(
                        "Store field_access node {} has no FieldAccessInfo",
                        field_access_id.get()
                    ),
                );
                return;
            }
        };

        // Get the record layout to extract field offset and size.
        let record_layout = match self.state.record_layout(field_info.type_id) {
            Some(layout) => layout,
            None => {
                self.push_typed_diag(
                    u1645_code(),
                    format!(
                        "No record layout found for type {}",
                        field_info.type_id.0
                    ),
                );
                return;
            }
        };

        // Get the field layout.
        let field_index = field_info.field_index as usize;
        let field_layout = match record_layout.fields.get(field_index) {
            Some(layout) => layout,
            None => {
                self.push_typed_diag(
                    u1646_code(),
                    format!(
                        "Field index {} out of bounds for record type {}",
                        field_index, field_info.type_id.0
                    ),
                );
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
                self.push_typed_diag(
                    t0564_code(),
                    format!(
                        "Unsupported field size {} for field store at offset {}",
                        field_size, field_offset
                    ),
                );
                return;
            }
        };

        // Phase 17 m6-b: Check if this is a module-level record field write.
        // Get the FieldAccess's receiver to check if it's a module-level symbol.
        let fa_children = arena.children(field_access_id);
        let receiver_id = match fa_children.first() {
            Some(&id) => id,
            None => {
                self.push_typed_diag(
                    u1643_code(),
                    format!(
                        "FieldAccess node {} has no receiver child",
                        field_access_id.get()
                    ),
                );
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
                self.push_typed_diag(
                    u1647_code(),
                    format!(
                        "field-assign pointer receiver for Store {} not found in local bindings",
                        store_id.get()
                    ),
                );
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
                        self.push_typed_diag(
                            u1647_code(),
                            format!(
                                "field-assign value Var for Store {} not found in local bindings",
                                store_id.get()
                            ),
                        );
                        return;
                    }
                }
            }
            // Preserve the prior fallback for non-Var RHS shapes (e.g. an
            // already-materialized scratch result landed in RDX upstream).
            _ => abi::RDX,
        };

        // paideia-as#1508 (PAS-DEBT-B2-015): if the parser stashed
        // `@endian(be)` on this field, byte-swap the value BEFORE
        // storing. The helper copies `value_reg` into R11, byte-swaps
        // R11 in place, and returns R11 as the new source register;
        // when unannotated (or `Le` on x86_64) it returns `value_reg`
        // unchanged and emits nothing. Callers must not touch
        // `value_reg` afterwards on the byte-swap path — R11 becomes
        // the store source, so the caller's binding table entry for
        // the original value stays intact.
        let src_reg = self.emit_endian_store_swap_if_needed(
            field_info.type_id,
            field_info.field_index,
            field_size,
            field_signed,
            value_reg,
        );

        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::MemSib {
            base: base_reg,                               // resolved pointer register
            index: None,                                  // no index
            scale: paideia_as_ir::instruction::Scale::X1, // ignored when no index
            disp: field_offset as i32,                    // field offset
        });
        operands.push(Operand::Reg(src_reg)); // resolved value register (R11 on BE swap)

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

    /// Issue #1182: emit `Module.field = expr` as a RIP-relative store against
    /// the bare exported symbol `field`. Assumes the paideia-as bare-name symbol
    /// convention (see emit_walker.rs::visit_let, preempt.pdx unsafe asm).
    ///
    /// MVP scope: RHS = Var only; width = u64 hardcoded (module lets in kernel
    /// are u64 today; matches #1176/#1179/#1181 precedent). Non-Var RHS is
    /// scope-guarded via T0540 to avoid silent miscompile — follow-up will
    /// extend parity with visit_var_assign's full RHS matrix.
    fn emit_module_field_write(
        &mut self,
        store_id: IrNodeId,
        field_access_id: IrNodeId,
        field_name: String,
        arena: &IrArena,
    ) {
        let children = arena.children(store_id);
        if children.len() != 3 {
            self.push_typed_diag(u1643_code(), format!(
                "Store node {} has {} children; expected 3",
                store_id.get(), children.len()));
            return;
        }
        let value_id = children[2];

        // Prevent double-visit under flat dispatch.
        self.state.mark_field_access_handled(field_access_id.get());

        let value_reg = match arena.get(value_id).map(|n| n.kind) {
            Some(IrKind::Var) => {
                let rhs_name = match arena.binding_names().get(value_id) {
                    Some(n) => n.to_string(),
                    None => {
                        self.push_typed_diag(t0540_code(), format!(
                            "module-field write RHS Var {} has no binding name",
                            value_id.get()));
                        return;
                    }
                };
                match self.state.local_bindings.get(&rhs_name) {
                    Some(reg) => reg,
                    None => {
                        self.push_typed_diag(t0540_code(), format!(
                            "module-field write RHS {} not found in local bindings; \
                             non-register sources not yet supported", rhs_name));
                        return;
                    }
                }
            }
            other => {
                self.push_typed_diag(t0540_code(), format!(
                    "module-field write RHS must be Var; got {:?}", other));
                return;
            }
        };

        self.emit_mem_write_via_rip_sym(
            store_id,
            value_reg,
            field_name,
            0,     // no field-offset addend (bare-symbol reference)
            8,     // u64 hardcode; width dispatch is follow-up
            false, // unsigned
        );
    }

    /// Issue #1184: emit `Module.field` (READ) as a RIP-relative load from the
    /// bare exported symbol `field`. Mirror of `emit_module_field_write` added
    /// in #1182 (f4df57a).
    ///
    /// MVP scope: width = u64 hardcoded (module lets in kernel are u64 today;
    /// matches #1176/#1179/#1181/#1182 precedent). Non-u64 module fields are
    /// deferred — follow-up will thread declared type through module_field_refs.
    ///
    /// `node_id` is the emission ID under which the load is registered; callers
    /// pass either the real FieldAccess node id (from visit_field_access_with_reg)
    /// or a virtual lambda-scoped id (from the visit_lambda FieldAccess arm).
    pub(crate) fn emit_module_field_read(
        &mut self,
        node_id: IrNodeId,
        dest_reg: RegId,
        field_name: String,
    ) {
        self.emit_mem_read_via_rip_sym(
            node_id,
            dest_reg,
            field_name,
            0,
            8,
            false,
        );
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
                // #1187: module-qualified FA reads are owned by three consumers that
                // run INSIDE the enclosing lambda's function scope, so the load bytes
                // land inside the function symbol range:
                //   - emit_visit_lambda.rs FieldAccess arm  — no-braces `fn () -> M.f`
                //   - emit_block_body.rs Let-arm FA branch  — let-RHS `let x = M.f; x`
                //   - emit_block_body.rs block-child FA arm — tail-in-braces `{ M.f }`
                // The fallback previously here (#1184-corr) emitted at the flat-walker's
                // arena preorder, before `pending_first_instr_lambda = Some(L)` fired,
                // so the load sorted to a byte offset BEFORE the function symbol
                // (issue #1187: orphan load). Silent-return preserves pre-#1184 behavior
                // for shapes we haven't enumerated.
                return;
            }
        };

        // Get the FieldAccess node's single child (the record value).
        let children = arena.children(field_access_id);
        let record_value_id = match children.first() {
            Some(&id) => id,
            None => {
                // No child; malformed FieldAccess node.
                self.push_typed_diag(
                    u1643_code(),
                    format!(
                        "FieldAccess node {} has no child",
                        field_access_id.get()
                    ),
                );
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
                self.push_typed_diag(
                    u1643_code(),
                    format!("Deref node {} has no child", record_value_id.get()),
                );
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
                self.push_typed_diag(
                    u1645_code(),
                    format!(
                        "No record layout found for type {}",
                        field_info.type_id.0
                    ),
                );
                return;
            }
        };

        // Get the field layout.
        let field_index = field_info.field_index as usize;
        let field_layout = match record_layout.fields.get(field_index) {
            Some(layout) => layout,
            None => {
                self.push_typed_diag(
                    u1646_code(),
                    format!(
                        "Field index {} out of bounds for record type {}",
                        field_index, field_info.type_id.0
                    ),
                );
                return;
            }
        };

        // PAS-DEBT-B3-006 (#1519): resolve the pointer receiver's home register
        // from LocalBindingTable instead of hardcoding RDI.
        //
        // Fallback taxonomy (debugger follow-up to the naive
        // `unwrap_or(abi::RDI)`): three distinct cases must not collapse
        // to one silent default:
        //   1. No binding name at all (`binding_names().get(ptr_id)` = None)
        //      → pre-#1519 shape, bare synthetic Var; keep RDI fallback for
        //      test-corpus byte-identity.
        //   2. Name bound to a `Reg` home → use that register.
        //   3. Name bound to a NON-`Reg` home (Stack / Env / Closure /
        //      RegPair) → refuse with U1648. Emitting `[rdi + offset]`
        //      here would silently generate wrong code for stack-spilled
        //      or captured pointer receivers.
        let base_reg = match arena.binding_names().get(ptr_id) {
            None => abi::RDI,
            Some(name) => match self.state.local_bindings.get(&name) {
                Some(reg) => reg,
                None => {
                    // Name is bound but not to a Reg home. Do not silently
                    // encode through RDI — that would corrupt the address
                    // for any lambda arg spilled to the stack, any closure
                    // capture, or any RegPair binding.
                    if self.state.local_bindings.get_home(&name).is_some() {
                        self.push_typed_diag(
                            u1648_code(),
                            format!(
                                "field access on pointer '{}' bound to a non-register home; \
                                 spill / env / closure loads are not yet emitted for field access",
                                name
                            ),
                        );
                        return;
                    }
                    // Truly unbound name — same shape as case 1.
                    abi::RDI
                }
            },
        };

        // Route through the unified width dispatch.
        let field_size = field_layout.size;
        let field_signed = field_layout.signed;
        self.emit_widening_load(
            field_access_id,
            field_layout.offset as i32,
            base_reg,
            dest_reg,
            field_size,
            field_signed,
        );

        // paideia-as#1508 (PAS-DEBT-B2-015): if the parser stashed
        // `@endian(be)` on this field, byte-swap the loaded value in
        // place on `dest_reg`. Unannotated fields (the common case)
        // hit a HashMap-miss inside the helper and emit no extra
        // instructions — pre-Wave-55 byte-identity is preserved for
        // every existing fixture.
        self.emit_endian_load_swap_if_needed(
            field_info.type_id,
            field_info.field_index,
            field_size,
            field_signed,
            dest_reg,
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
    /// PAS-DEBT-B3-006 (#1519): `base_reg` is now threaded through the three
    /// underlying primitives so callers that resolve the receiver via
    /// LocalBindingTable (e.g. `(*p).f` where `p` lives in RCX) get
    /// `[base_reg + offset]` instead of the pre-#1519 hardcoded `[rdi + offset]`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_widening_load(
        &mut self,
        node_id: IrNodeId,
        offset: i32,
        base_reg: RegId,
        dest_reg: RegId,
        size: u8,
        signed: bool,
    ) {
        match (size, signed) {
            (1, false) => self.emit_field_access_movzx_reg(node_id, offset, base_reg, dest_reg, 1),
            (2, false) => self.emit_field_access_movzx_reg(node_id, offset, base_reg, dest_reg, 2),
            (4, false) => self.emit_field_access_mov_sized_reg(
                node_id, offset, base_reg, dest_reg, IntWidth::W32,
            ),
            (8, false) => self.emit_field_access_mov_sized_reg(
                node_id, offset, base_reg, dest_reg, IntWidth::W64,
            ),
            (1, true) => self.emit_field_access_movsx_reg(node_id, offset, base_reg, dest_reg, 1),
            (2, true) => self.emit_field_access_movsx_reg(node_id, offset, base_reg, dest_reg, 2),
            (4, true) => self.emit_field_access_movsx_reg(node_id, offset, base_reg, dest_reg, 4),
            (8, true) => self.emit_field_access_mov_sized_reg(
                node_id, offset, base_reg, dest_reg, IntWidth::W64,
            ),
            _ => {
                self.push_typed_diag(
                    t0565_code(),
                    format!(
                        "Unsupported field: size={}, signed={} at node {}",
                        size, signed, node_id.get()
                    ),
                );
            }
        }
    }

    pub(crate) fn emit_field_access_mov_sized_reg(
        &mut self,
        field_access_id: IrNodeId,
        offset: i32,
        base_reg: RegId,
        dest_reg: RegId,
        width: IntWidth,
    ) {
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::Reg(dest_reg)); // destination register
        operands.push(Operand::MemSib {
            base: base_reg, // #1519: caller-resolved receiver register
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
        base_reg: RegId,
        dest_reg: RegId,
        src_width: u8,
    ) {
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::Reg(dest_reg)); // destination register
        operands.push(Operand::MemSib {
            base: base_reg, // #1519: caller-resolved receiver register
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
        base_reg: RegId,
        dest_reg: RegId,
        src_width: u8,
    ) {
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::Reg(dest_reg)); // destination register
        operands.push(Operand::MemSib {
            base: base_reg, // #1519: caller-resolved receiver register
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

    /// paideia-as#1301 (v0.21-003c): if `sym_name` is a module-level
    /// `@atomic(SeqCst)` binding, emit an `mfence` (0F AE F0) BEFORE the
    /// subsequent load. On x86_64 TSO the plain aligned load is already
    /// acquire-ordered, so Relaxed / Acquire / Release produce no fence;
    /// only SeqCst needs the mfence to enforce the single global total
    /// order across cores (Intel SDM Vol. 3A §8.2). Returns silently when
    /// the binding is non-atomic or the ordering is not SeqCst — the
    /// caller always follows this with the plain load, so both paths
    /// converge on the same downstream instruction shape.
    fn emit_atomic_fence_pre_load(&mut self, sym_name: &str) {
        use paideia_as_ir::let_meta::AtomicOrdering;
        if !matches!(self.state.atomic_bindings.get(sym_name), Some(AtomicOrdering::SeqCst)) {
            return;
        }
        let fence_id = self.alloc_synthetic_id();
        let inst = Instruction {
            mnemonic: Mnemonic::Mfence,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        self.emit_inst(fence_id, inst);
    }

    /// pa-r17-005-e: Emit global record-field read via RIP-relative symbol.
    ///
    /// Emits `mov <dest_reg>, [rip + sym + addend]` for module-level records.
    /// Handles u32 (W32) and u64 (W64) sized loads; other sizes push a diagnostic.
    ///
    /// For u8/u16/i8/i16/i32, the fixture does not exercise these paths, so
    /// a diagnostic is emitted as a placeholder. This method is called from
    /// the visit_lambda arm for FieldAccess(Var(...)) tail-position reads.
    ///
    /// paideia-as#1301 (v0.21-003c): if `sym_name` names an atomic binding
    /// with `LetInfo::atomic == Some(SeqCst)`, an `mfence` is emitted BEFORE
    /// the load so the aggregate byte sequence matches the x86_64 TSO
    /// SeqCst-load recipe pinned in
    /// `crates/paideia-as-encoder/tests/memory_ops/atomic_ordering.rs`.
    pub(crate) fn emit_mem_read_via_rip_sym(
        &mut self,
        node_id: IrNodeId,
        dest_reg: RegId,
        sym_name: String,
        addend: i32,
        size: u8,
        signed: bool,
    ) {
        // paideia-as#1301: SeqCst load — mfence bracket goes BEFORE the mov.
        self.emit_atomic_fence_pre_load(&sym_name);
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
        // paideia-as#1301 (v0.21-003c): capture the fence disposition BEFORE
        // moving `sym_name` into the store's operand list. Post-store fence
        // (SeqCst) fires only when we actually emit the store; the diagnostic
        // branch below produces no bytes and so needs no fence either.
        let post_store_needs_mfence = {
            use paideia_as_ir::let_meta::AtomicOrdering;
            matches!(
                self.state.atomic_bindings.get(sym_name.as_str()),
                Some(AtomicOrdering::SeqCst)
            )
        };
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
                if post_store_needs_mfence {
                    let fence_id = self.alloc_synthetic_id();
                    let fence = Instruction {
                        mnemonic: Mnemonic::Mfence,
                        operands: SmallVec::new(),
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    };
                    self.emit_inst(fence_id, fence);
                }
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
                if post_store_needs_mfence {
                    let fence_id = self.alloc_synthetic_id();
                    let fence = Instruction {
                        mnemonic: Mnemonic::Mfence,
                        operands: SmallVec::new(),
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    };
                    self.emit_inst(fence_id, fence);
                }
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

    /// paideia-as#1508 (PAS-DEBT-B2-015): if the (type_id, field_index)
    /// pair is annotated with `@endian(be)`, emit a byte-swap on
    /// `reg` sized to `field_size`. `@endian(le)` on the native
    /// little-endian x86_64 target is a no-op; unannotated fields are
    /// a no-op (byte-identical to pre-Wave-55).
    ///
    /// Called on the LOAD side immediately after `emit_widening_load`
    /// materialises the memory value into `reg`. The byte-swap is
    /// safe in-place: `reg` is the load's destination and any prior
    /// contents are gone.
    ///
    /// **Signed narrow-width recipe (Wave 57, paideia-as#1560).**
    /// i16/i32 loads dispatched through `emit_widening_load` land in
    /// `reg` via `movsx r64, word[mem]` / `movsxd r64, [mem]`. The
    /// upper bits carry sign-extension of the raw big-endian byte
    /// sequence — i.e., the wrong sign for the intended value.
    /// The endian helper repairs this by:
    ///   1. Byte-reversing the low width in place (`rol r16, 8` for
    ///      i16; `bswap r32` for i32). The upper bits are now stale
    ///      but the low bits carry the correctly-ordered value.
    ///   2. Re-widening from the low `field_size` bytes with a
    ///      second `movsx`/`movsxd`, which discards the stale upper
    ///      bits and re-derives sign from the swapped low half.
    /// i8 is a one-byte no-op (endianness meaningless); u8/u16/u32
    /// and u64/i64 use the pre-Wave-56 single-step recipe (the
    /// upper bits are already zero for the unsigned narrow forms
    /// and byte-order is representation-neutral for the 8-byte
    /// forms).
    pub(crate) fn emit_endian_load_swap_if_needed(
        &mut self,
        type_id: RecordTypeId,
        field_index: u32,
        field_size: u8,
        field_signed: bool,
        reg: RegId,
    ) {
        let endian = match self.state.field_endian(type_id, field_index) {
            Some(e) => e,
            None => return, // Unannotated: hot path, no bswap.
        };
        // Explicit destructure to force a compile break should
        // `Endianness` grow a variant (append point per the AST
        // enum's forward-compat contract).
        match endian {
            Endianness::Le => {
                // Native little-endian on x86_64: no emission.
            }
            Endianness::Be => {
                let swapped = self.emit_bswap_low_bits(field_size, field_signed, reg);
                // Signed narrow (i16/i32): the initial movsx/movsxd
                // baked stale sign-extension into the upper bits.
                // Re-widen from the (now correctly-swapped) low
                // `field_size` bytes so upper bits track the true
                // sign of the intended value. i8 (field_size == 1)
                // skips this — no swap happened and the initial
                // movsx already covers the whole value.
                if swapped && field_signed && matches!(field_size, 2 | 4) {
                    self.emit_movsx_widen_after_swap(field_size, reg);
                }
            }
        }
    }

    /// paideia-as#1508 (PAS-DEBT-B2-015): store-side companion to
    /// `emit_endian_load_swap_if_needed`. When the annotation resolves
    /// to `Be`, emit `mov r11, value_reg; bswap-low r11` and return
    /// `R11` so the caller uses it as the store source. When
    /// unannotated or `Le`, return `value_reg` unchanged and emit
    /// nothing.
    ///
    /// R11 is used as scratch because it is (a) caller-saved in
    /// SysV, (b) never in the SysV argument-passing sequence, and
    /// (c) already the canonical last-resort scratch across
    /// `emit_store_record.rs` and `emit_int_match.rs` (see
    /// `PATTERN_SCRATCH` in `paideia-as-ir::abi`). Copying via a
    /// 64-bit `mov r11, value_reg` is safe for every field width —
    /// the store mnemonic downstream is width-aware
    /// (`MovSized{width}`) and writes exactly `field_size` bytes.
    ///
    /// Signed narrow widths (i16/i32) with `@endian(be)` (Wave 57,
    /// paideia-as#1560): no post-swap sign-extend is required on the
    /// store side because the downstream `MovSized{width}` narrows
    /// the write to exactly `field_size` bytes — the stale upper
    /// bits left by `rol r16` (or the zero-extended upper 32 left by
    /// `bswap r32`) are dropped by the store's operand-size prefix.
    /// The same three-instruction shape `mov r11, v; swap-low r11;
    /// MovSized [mem], r11` therefore serves both signed and
    /// unsigned narrow. i64 remains bit-pattern-identical to u64 at
    /// this level. i8 is a one-byte no-op.
    pub(crate) fn emit_endian_store_swap_if_needed(
        &mut self,
        type_id: RecordTypeId,
        field_index: u32,
        field_size: u8,
        field_signed: bool,
        value_reg: RegId,
    ) -> RegId {
        let endian = match self.state.field_endian(type_id, field_index) {
            Some(e) => e,
            None => return value_reg,
        };
        match endian {
            Endianness::Le => value_reg,
            Endianness::Be => {
                // size == 1 (u8 or i8) — byte-swap on a single byte
                // is the identity for either signedness; leave
                // value_reg untouched (equivalent to the unannotated
                // path).
                if field_size == 1 {
                    return value_reg;
                }
                // Widths 2/4/8 (signed and unsigned): emit
                // `mov r11, value_reg` (64-bit copy — the store's
                // own MovSized narrows on write) and then the
                // width-appropriate byte-reversal on R11. The store
                // downstream writes exactly `field_size` bytes, so
                // any stale upper bits in R11 after the swap are
                // dropped — no re-sign-extend is needed here (unlike
                // the load side, where the widened dest_reg must
                // carry correct sign in every bit).
                let mov_id = self.alloc_synthetic_id();
                let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                mov_operands.push(Operand::Reg(abi::R11));
                mov_operands.push(Operand::Reg(value_reg));
                let mov = Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: mov_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };
                self.emit_inst(mov_id, mov);
                if !self.emit_bswap_low_bits(field_size, field_signed, abi::R11) {
                    // Defensive: any unsupported width diagnoses
                    // through `emit_bswap_low_bits` and returns false.
                    // Restore `value_reg` as the store source so the
                    // pre-Wave path still produces bytes rather than
                    // an orphaned scratch move + broken store.
                    return value_reg;
                }
                abi::R11
            }
        }
    }

    /// paideia-as#1508 / paideia-as#1560: emit the width-appropriate
    /// byte-reversal instruction on `reg`. Returns `true` when an
    /// instruction was emitted (and the caller may rely on `reg`
    /// holding the swapped low bytes), `false` when the width is a
    /// no-op (single byte) or the width is unsupported (T0567 is
    /// emitted defensively in that case).
    ///
    /// `field_signed` is accepted for load-side callers that need to
    /// dispatch on it (they append a `movsx` re-widen when
    /// `field_signed && size in {2, 4}` — see
    /// `emit_endian_load_swap_if_needed`) but is NOT consulted here
    /// — the byte-reversal itself is identical between signed and
    /// unsigned for a given width. Wave 57 (#1560) removed the
    /// signed-narrow refusal that Wave 56 (#1508) landed as a
    /// silent-miscompile guard, since both load and store paths now
    /// carry the full recipe.
    ///
    /// Width recipes:
    ///   * 1 byte  — no-op (endianness is meaningless for a single
    ///     byte); returns `false`.
    ///   * 2 bytes — `rol r16, 8` (66h prefix; touches only the low
    ///     16 bits, upper bits unchanged). Callers whose value must
    ///     carry correct sign in the upper bits (load side, signed
    ///     narrow) must follow with `movsx r64, r16`.
    ///   * 4 bytes — `bswap r32` (0F C8+rd; zero-extends to r64).
    ///     Load-side signed narrow (i32) must follow with
    ///     `movsxd r64, r32` — bswap32's zero-extension is wrong for
    ///     negative values.
    ///   * 8 bytes — `bswap r64` (REX.W 0F C8+rd).
    ///   * other sizes — T0567 diagnostic (defensive), `false`.
    fn emit_bswap_low_bits(&mut self, field_size: u8, _field_signed: bool, reg: RegId) -> bool {
        // size == 1: byte-swap on a single byte is the identity for
        // either signedness. Emit nothing (u8/i8 both fall here).
        if field_size == 1 {
            return false;
        }
        match field_size {
            1 => {
                // Unreachable — handled above; kept for match
                // exhaustiveness against the `other` arm below.
                false
            }
            2 => {
                let rol_id = self.alloc_synthetic_id();
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::Reg(reg));
                operands.push(Operand::Imm64(8));
                let rol = Instruction {
                    mnemonic: Mnemonic::Rol { width: IntWidth::W16 },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };
                self.emit_inst(rol_id, rol);
                true
            }
            4 => {
                let bs_id = self.alloc_synthetic_id();
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::Reg(reg));
                let bs = Instruction {
                    mnemonic: Mnemonic::Bswap32,
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };
                self.emit_inst(bs_id, bs);
                true
            }
            8 => {
                let bs_id = self.alloc_synthetic_id();
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::Reg(reg));
                let bs = Instruction {
                    mnemonic: Mnemonic::Bswap,
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                };
                self.emit_inst(bs_id, bs);
                true
            }
            other => {
                self.push_typed_diag(
                    t0567_code(),
                    format!(
                        "@endian(be) on unsupported scalar size {} — recognised \
                         widths are 1/2/4/8",
                        other
                    ),
                );
                false
            }
        }
    }

    /// paideia-as#1560 (Wave 57): emit the post-swap re-sign-extend
    /// for load-side signed narrow (i16/i32). After
    /// `emit_bswap_low_bits` reversed the low `field_size` bytes in
    /// `reg`, this helper re-derives the sign from the swapped low
    /// half into all 64 bits of `reg` via a reg-reg `movsx`/`movsxd`:
    ///
    ///   * size 2 — `movsx r64, r16`  (REX.W 0F BF /r)
    ///   * size 4 — `movsxd r64, r32` (REX.W 63 /r)
    ///
    /// The encoding_hint mirrors `emit_field_access_movsx_reg`'s
    /// convention: `opcode = 0x0F` for widths 1/2 (two-byte 0F BE/BF
    /// escape), `0x63` for width 4 (single-byte MOVSXD). Called
    /// only from `emit_endian_load_swap_if_needed`; the store side
    /// does not need this leg because the downstream `MovSized`
    /// narrows the write and any stale upper bits are dropped.
    fn emit_movsx_widen_after_swap(&mut self, field_size: u8, reg: RegId) {
        // Opcode mirrors `emit_field_access_movsx_reg`'s convention
        // and `encode_movsx`'s dispatch: 0x0F for the two-byte 0F BE/BF
        // escape (sizes 1 and 2 — sign-extended widening from r8/r16),
        // 0x63 for the single-byte MOVSXD (size 4 — from r32).
        let opcode: u16 = match field_size {
            2 => 0x0F,
            4 => 0x63,
            _ => return, // Callers gate on {2, 4} — defensive no-op.
        };
        let widen_id = self.alloc_synthetic_id();
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        operands.push(Operand::Reg(reg));
        operands.push(Operand::Reg(reg));
        let widen = Instruction {
            mnemonic: Mnemonic::Movsx,
            operands,
            encoding_hint: Some(EncodingHint { opcode, operand_size: field_size }),
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        self.emit_inst(widen_id, widen);
    }
}
