//! Enum + match lowering — variant constructors, discriminant extraction,
//! pattern binding decomposition, and match dispatch.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. The four
//! methods hosted here are cohesive: `visit_enum_cons` produces a value
//! that `visit_enum_discriminant` and `visit_match` consume, and
//! `lower_pattern` is the recursive helper that both stack-form enum
//! bindings and match arms delegate to.

use paideia_as_ir::instruction::{Cond, Instruction, Mnemonic, Operand, RegId, IntWidth};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi, PassingConvention, EnumLayout, SymbolKind};
use paideia_as_diagnostics::{DiagnosticCode, Category, Severity};

use crate::emit_block_body::TailContext;
use crate::emit_store_record::operator_lexeme_of;
use crate::emit_walker::EmitWalker;

/// Helper to construct T0556 diagnostic code.
fn t0556_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 556)
        .expect("T0556 is within valid T range")
}

/// Helper to construct T0559 diagnostic code.
fn t0559_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 559)
        .expect("T0559 is within valid T range")
}

/// Helper to construct T0560 diagnostic code.
fn t0560_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 560)
        .expect("T0560 is within valid T range")
}

/// Helper to construct T0561 diagnostic code.
fn t0561_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 561)
        .expect("T0561 is within valid T range")
}

/// Helper to construct T0562 diagnostic code.
fn t0562_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 562)
        .expect("T0562 is within valid T range")
}

/// Helper to construct T0563 diagnostic code.
fn t0563_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 563)
        .expect("T0563 is within valid T range")
}

/// Helper to construct T0566 diagnostic code.
fn t0566_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 566)
        .expect("T0566 is within valid T range")
}

/// Helper to construct U1648 diagnostic code.
fn u1648_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1648)
        .expect("U1648 is within valid U range")
}

/// Helper to construct U1649 diagnostic code.
fn u1649_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1649)
        .expect("U1649 is within valid U range")
}

/// Helper to construct U1650 diagnostic code.
fn u1650_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1650)
        .expect("U1650 is within valid U range")
}

/// Helper to construct U1651 diagnostic code.
fn u1651_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1651)
        .expect("U1651 is within valid U range")
}

/// Helper to construct U1652 diagnostic code.
fn u1652_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1652)
        .expect("U1652 is within valid U range")
}

/// Helper to construct U1653 diagnostic code.
fn u1653_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1653)
        .expect("U1653 is within valid U range")
}

/// Helper to construct U1654 diagnostic code.
fn u1654_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1654)
        .expect("U1654 is within valid U range")
}

/// Helper to construct U1655 diagnostic code.
fn u1655_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1655)
        .expect("U1655 is within valid U range")
}

/// Helper to construct U1656 diagnostic code.
fn u1656_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1656)
        .expect("U1656 is within valid U range")
}

/// Helper to construct U1657 diagnostic code.
fn u1657_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1657)
        .expect("U1657 is within valid U range")
}

/// Helper to construct U1658 diagnostic code.
fn u1658_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, 1658)
        .expect("U1658 is within valid U range")
}

impl EmitWalker {
    /// #1084: Emit scrutinee load for match expressions.
    ///
    /// For Var scrutinees:
    /// - Module Object symbol: emit `mov rax, [rip+sym]` + optional
    ///   `mov rdx, [rip+sym+payload_offset]` for register-form enums.
    /// - Local binding: emit register-to-register moves.
    /// - Missing binding_names: silent no-op (#1133 compat for test-fixture Vars).
    /// - Non-Var: T0556 diagnostic.
    fn emit_scrutinee_load(
        &mut self,
        _match_id: IrNodeId,
        scrutinee_id: IrNodeId,
        layout: &EnumLayout,
        arena: &IrArena,
    ) {
        debug_assert!(arena.int_match_scrutinee().get(_match_id).is_none(),
            "int match reached enum path");

        let scrutinee_node = match arena.get(scrutinee_id) {
            Some(n) => n,
            None => return,
        };

        // Only Var scrutinees are supported.
        if scrutinee_node.kind != IrKind::Var {
            self.push_typed_diag(
                t0556_code(),
                format!("unsupported scrutinee kind: {:?}", scrutinee_node.kind),
            );
            return;
        }

        // Get binding name from binding_names table.
        // Silent no-op if not found (#1133 compat: test-fixture Vars without binding_names).
        let name = match arena.binding_names().get(scrutinee_id) {
            Some(n) => n.to_string(),
            None => return,
        };

        // Issue #1212: Try local bindings first (defensive consumer reorder).
        if let Some((src_reg, payload_reg)) = self.state.local_bindings.get_pair(&name) {
            // Issue #1156 (Part 3): Skip self-move for discriminant register.
            // Emit mov rax, <src_reg> (skip if src_reg == RAX to avoid redundant mov rax, rax)
            if src_reg != abi::RAX {
                let mov_rax_id = self.alloc_synthetic_id();
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::Reg(abi::RAX));
                operands.push(Operand::Reg(src_reg));

                self.emit_inst(
                    mov_rax_id,
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    },
                );
            }

            // #1154: RDX loaded from payload_reg when layout.payload_size > 0; missing payload_reg → U1658.
            if layout.payload_size > 0 {
                if let Some(p) = payload_reg {
                    // Issue #1156 (Part 3): Skip self-move for payload register.
                    // Emit mov rdx, <payload_reg> (skip if p == RDX to avoid redundant mov rdx, rdx)
                    if p != abi::RDX {
                        let load_rdx_id = self.alloc_synthetic_id();
                        let mut rdx_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        rdx_operands.push(Operand::Reg(abi::RDX));
                        rdx_operands.push(Operand::Reg(p));

                        self.emit_inst(
                            load_rdx_id,
                            Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: rdx_operands,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
                            },
                        );
                    }
                } else {
                    self.push_typed_diag(u1658_code(), format!(
                        "register-form enum local binding '{}' has no payload register; RDX would be stale",
                        name
                    ));
                }
            }
            return;
        }

        // Fall back to module symbol.
        if let Some(symbol) = arena.symbols().lookup_by_name(&name) {
            if matches!(symbol.kind, SymbolKind::Object) {
                // #1153: Stack-form enums (size > 16) — caller-pointer convention.
                // Consumers (visit_enum_discriminant, lower_pattern base_reg=RDI)
                // read `[rdi+offset]`; load the object's address into RDI.
                if layout.size > 16 {
                    let lea_rdi_id = self.alloc_synthetic_id();
                    let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                    operands.push(Operand::Reg(abi::RDI));
                    operands.push(Operand::MemRipRelSym { name, addend: 0 });
                    self.emit_inst(lea_rdi_id, Instruction {
                        mnemonic: Mnemonic::Lea,
                        operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    });
                    return;
                }

                // Emit mov rax, [rip+name+0]
                let load_rax_id = self.alloc_synthetic_id();
                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                operands.push(Operand::Reg(abi::RAX));
                operands.push(Operand::MemRipRelSym {
                    name: name.clone(),
                    addend: 0,
                });

                self.emit_inst(
                    load_rax_id,
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W64,
                        },
                        operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    },
                );

                // If payload_size > 0, emit mov rdx, [rip+name+payload_offset]
                if layout.payload_size > 0 {
                    let load_rdx_id = self.alloc_synthetic_id();
                    let mut rdx_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                    rdx_operands.push(Operand::Reg(abi::RDX));
                    rdx_operands.push(Operand::MemRipRelSym {
                        name,
                        addend: 8,
                    });

                    self.emit_inst(
                        load_rdx_id,
                        Instruction {
                            mnemonic: Mnemonic::MovSized {
                                width: IntWidth::W64,
                            },
                            operands: rdx_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        },
                    );
                }

                return;
            }
        }

        // Neither local nor module — silent no-op (compat).
    }

    /// PA-r17-007: Emit enum variant constructor lowering.
    ///
    /// Guard function that checks if the EnumCons has already been handled.
    /// If so, returns early. Otherwise delegates to emit_enum_cons_inner.
    ///
    /// EnumCons node children: [payload_expr (optional)]
    pub(crate) fn visit_enum_cons(&mut self, enum_cons_id: IrNodeId, arena: &IrArena) {
        // #1084: If this EnumCons has been marked as handled by the pre-pass
        // (module-scope Let → EnumCons owned by data_encoder::encode_enum_cons),
        // skip code generation. The data_encoder has already serialized the enum
        // to binary data.
        if self.state.was_enum_cons_handled(enum_cons_id.get()) {
            return;
        }

        // #1145: If this EnumCons has a RecordCons payload that's been marked as
        // handled (by the pre-pass for data-let EnumCons), skip code generation.
        // The data_encoder has already serialized the enum to binary data.
        let children = arena.children(enum_cons_id);
        if let Some(&payload_id) = children.first() {
            if let Some(_payload_node) = arena.get(payload_id) {
                if self.state.was_record_cons_handled(payload_id.get()) {
                    // This is a data-encoder-owned enum value; skip emission.
                    return;
                }
            }
        }

        self.emit_enum_cons_inner(enum_cons_id, arena);
    }

    /// Emit the body of enum variant constructor lowering.
    ///
    /// Handles register form (≤16-byte enums) and stack form (>16-byte enums).
    /// Register form: RAX = discriminant, RDX = payload (if any)
    /// Stack form: [rsp+0] = discriminant, [rsp+8] = payload
    ///
    /// Called by visit_enum_cons (guarded entry point) and emit_visit_lambda (for lambda bodies).
    pub(crate) fn emit_enum_cons_inner(&mut self, enum_cons_id: IrNodeId, arena: &IrArena) {
        let info = match arena.enum_cons_info().get(enum_cons_id) {
            Some(i) => i,
            None => {
                self.push_typed_diag(u1648_code(), format!(
                    "EnumCons node {} has no EnumConsInfo",
                    enum_cons_id.get()
                ));
                return;
            }
        };

        let (_layout_size, layout_payload_size) =
            match self.state.enum_layout(info.type_id) {
                Some(l) => (l.size, l.payload_size),
                None => {
                    self.push_typed_diag(u1649_code(), format!(
                        "No enum layout found for type {}",
                        info.type_id.0
                    ));
                    return;
                }
            };

        let variant_index = info.variant_index as i64;
        let layout = match self.state.enum_layout(info.type_id) {
            Some(l) => l,
            None => {
                self.push_typed_diag(u1649_code(), format!(
                    "No enum layout found for type {}",
                    info.type_id.0
                ));
                return;
            }
        };

        let discriminant_size = layout.discriminant_size as i32;
        let passing_conv = layout.passing_convention();

        match passing_conv {
            PassingConvention::RegisterPair => {
                // Register form: RAX = discriminant, RDX = payload (if any)
                // Emit 1: mov rax, <variant_index>
                let disc_id = IrNodeId::new(enum_cons_id.get() * 10)
                    .expect("virtual disc id");
                let mut disc_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                disc_operands.push(Operand::Reg(abi::RAX));  // RAX
                disc_operands.push(Operand::Imm64(variant_index));

                self.emit_inst(disc_id, Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: disc_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                emission_order: 0,
                });

                // Emit 2 only if payload_size > 0
                if layout_payload_size > 0 {
                    let payload_id = IrNodeId::new(enum_cons_id.get() * 10 + 1)
                        .expect("virtual payload id");
                    let children = arena.children(enum_cons_id);
                    let payload_child_id = children.first().copied();

                    let payload_operand = match payload_child_id {
                        Some(child_id) => {
                            let child = arena.get(child_id);
                            match child.map(|n| n.kind) {
                                Some(IrKind::Literal) => {
                                    let val = arena.literal_values().get(child_id).unwrap_or(0);
                                    Operand::Imm64(val)
                                }
                                Some(IrKind::Var) => {
                                    // Var → Reg source (RDI for now, matching visit_field_assign convention)
                                    Operand::Reg(abi::RDI)
                                }
                                _ => {
                                    self.push_typed_diag(
                                        t0559_code(),
                                        format!(
                                            "EnumCons {} payload child {:?} not supported (only Literal/Var)",
                                            enum_cons_id.get(),
                                            child.map(|n| n.kind)
                                        ),
                                    );
                                    return;
                                }
                            }
                        }
                        None => {
                            self.push_typed_diag(u1648_code(), format!(
                                "EnumCons {} has payload_size > 0 but no child",
                                enum_cons_id.get()
                            ));
                            return;
                        }
                    };

                    let mut payload_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                    payload_operands.push(Operand::Reg(abi::RDX));  // RDX
                    payload_operands.push(payload_operand);

                    self.emit_inst(payload_id, Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: payload_operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                    emission_order: 0,
                    });
                }
            }
            PassingConvention::Indirect => {
                // Indirect form: [rdi+0] = disc, [rdi+disc_size] = payload
                // Emit 1: mov [rdi+0], <disc>
                let disc_id = IrNodeId::new(enum_cons_id.get() * 10)
                    .expect("virtual disc id");
                let mut disc_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                disc_operands.push(Operand::MemSib {
                    base: abi::RDI,  // RDI (return slot pointer)
                    index: None,
                    scale: paideia_as_ir::instruction::Scale::X1,
                    disp: 0,
                });
                disc_operands.push(Operand::Imm64(variant_index));

                self.emit_inst(disc_id, Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: disc_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                emission_order: 0,
                });

                if layout_payload_size > 0 {
                    // Emit 2: mov [rdi+disc_size], payload_value or reg
                    let payload_id = IrNodeId::new(enum_cons_id.get() * 10 + 1)
                        .expect("virtual payload id");
                    let children = arena.children(enum_cons_id);
                    let payload_val = children.first()
                        .and_then(|&c| Some(arena.literal_values().get(c).unwrap_or(0)))
                        .unwrap_or(0);

                    let mut payload_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                    payload_operands.push(Operand::MemSib {
                        base: abi::RDI,
                        index: None,
                        scale: paideia_as_ir::instruction::Scale::X1,
                        disp: discriminant_size,
                    });
                    payload_operands.push(Operand::Imm64(payload_val));

                    self.emit_inst(payload_id, Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: payload_operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                    emission_order: 0,
                    });
                }
            }
        }
    }


    /// PA-r17-008: Emit enum discriminant extraction.
    ///
    /// Extracts the discriminant from an enum value. Handling differs by layout form:
    /// - Register form (size ≤ 16): discriminant already in RAX, no load needed.
    /// - Stack form (size > 16): emit `mov rax, [rdi+0]` to load discriminant.
    pub(crate) fn visit_enum_discriminant(&mut self, enum_disc_id: IrNodeId, arena: &IrArena) {
        let type_id = match arena.enum_disc_info().get(enum_disc_id) {
            Some(tid) => *tid,
            None => {
                self.push_typed_diag(u1648_code(), format!(
                    "EnumDiscriminant node {} has no EnumTypeId registered",
                    enum_disc_id.get()
                ));
                return;
            }
        };

        let layout = match self.state.enum_layout(type_id) {
            Some(l) => l,
            None => {
                self.push_typed_diag(u1649_code(), format!(
                    "No enum layout found for type {}",
                    type_id.0
                ));
                return;
            }
        };

        // Register form: discriminant already in RAX, no load needed.
        if layout.size <= 16 {
            return;
        }

        // Stack form: emit mov rax, [rdi+0] (3 bytes: 48 8B 07)
        let disc_load_id = IrNodeId::new(enum_disc_id.get() * 10).expect("disc load id");
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

    /// Phase 17 m9-009 (pa-r17-009): Lower nested pattern bindings.
    ///
    /// Recursively decomposes a PatternBinding tree, emitting load instructions
    /// to extract nested record fields and enum payloads. Bindings are recorded
    /// in LocalBindingTable for later variable resolution.
    ///
    /// Algorithm:
    /// - `Wildcard`: no-op
    /// - `Simple(name)`: emit width-correct load; insert into LocalBindingTable
    /// - `EnumVariant { payload: Some(inner) }`: recurse with payload offset (base_offset + 8)
    /// - `Record { type_id, fields }`: for each field, compute sub_offset and recurse
    ///
    /// Register allocation from scratch pool: [RCX(1), RDX(2), R8(8), R10(10), R11(11)]
    /// Exhaustion emits diagnostic; no spill.
    ///
    /// # Panics
    /// None under normal operation; may emit diagnostics on register exhaustion or
    /// missing layout information.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_pattern(
        &mut self,
        pattern: &paideia_as_ir::PatternBinding,
        base_reg: RegId,
        base_offset: i32,
        arm_id: IrNodeId,
        slot: &mut u32,
        arena: &IrArena,
        default_size_signed: (u8, bool),
    ) {
        // #1216: pool snapshot at outer entry; all sibling binders share one pool.
        let base_pool = [abi::RCX, abi::RDX, abi::R8, abi::R10, abi::R11];
        let live = self.state.local_bindings.live_regs();
        let pool: SmallVec<[RegId; 5]> =
            base_pool.iter().copied().filter(|r| !live.contains(r)).collect();
        self.lower_pattern_inner(pattern, base_reg, base_offset, arm_id, slot, arena, default_size_signed, &pool);
    }

    /// Private inner method for lower_pattern that threads the register pool through recursion.
    /// Pool is snapshot once at outer entry and shared across all sibling binders.
    #[allow(clippy::too_many_arguments)]
    fn lower_pattern_inner(
        &mut self,
        pattern: &paideia_as_ir::PatternBinding,
        base_reg: RegId,
        base_offset: i32,
        arm_id: IrNodeId,
        slot: &mut u32,
        arena: &IrArena,
        default_size_signed: (u8, bool),
        pool: &[RegId],
    ) {
        use paideia_as_ir::PatternBinding;

        match pattern {
            PatternBinding::Wildcard => {
                // No-op: wildcard matches anything without binding
            }

            PatternBinding::Simple(name) => {
                let idx = *slot as usize;
                if idx >= pool.len() {
                    // U1654 — pool exhausted after outer live-reg filter
                    self.push_typed_diag(u1654_code(), format!(
                        "Nested pattern binding exhaustion: pool={} slot={} in arm {}",
                        pool.len(), *slot, arm_id.get()));
                    return;
                }
                let dest_reg = pool[idx];
                let load_id = IrNodeId::new(arm_id.get() * 1000 + *slot + 1).unwrap_or(arm_id);
                *slot += 1;

                // Emit width-correct load via the unified dispatch.
                let (size, signed) = default_size_signed;
                let before = self.diagnostics.len();
                self.emit_widening_load(load_id, base_offset, dest_reg, size, signed);
                if self.diagnostics.len() > before {
                    // Unsupported size — the helper already pushed a diagnostic.
                    return;
                }

                // Insert binding into LocalBindingTable
                self.state.local_bindings.insert(name.clone(), dest_reg);
            }

            PatternBinding::EnumVariant {
                variant_index: _,
                payload_type,
                payload: Some(inner),
            } => {
                // Payload at offset 8 (enum layout standard)
                let sub_offset = base_offset + 8;

                let (sub_size, sub_signed) = if let Some(payload_type_id) = payload_type {
                    // If payload_type is a record, look up its layout
                    if let Some(rec_layout) = self.state.record_layout(*payload_type_id) {
                        // Use first field's size/signed as default for nested pattern
                        if let Some(first_field) = rec_layout.fields.first() {
                            (first_field.size, first_field.signed)
                        } else {
                            default_size_signed
                        }
                    } else {
                        // Layout not found; use default
                        default_size_signed
                    }
                } else {
                    default_size_signed
                };

                // Recurse with payload pattern
                self.lower_pattern_inner(inner, base_reg, sub_offset, arm_id, slot, arena, (sub_size, sub_signed), pool);
            }

            PatternBinding::EnumVariant {
                payload: None,
                ..
            } => {
                // Unit variant; no payload to extract
            }

            PatternBinding::Record {
                type_id,
                fields,
            } => {
                // Look up record layout
                let rec_layout = match self.state.record_layout(*type_id) {
                    Some(l) => l.clone(),
                    None => {
                        self.push_typed_diag(u1652_code(), format!(
                            "No record layout found for nested pattern type {}",
                            type_id.0
                        ));
                        return;
                    }
                };

                // For each field, compute offset and recurse
                for (field_name, sub_pattern) in fields {
                    let field_idx = match rec_layout.field_index_by_name(field_name) {
                        Some(idx) => idx,
                        None => {
                            self.push_typed_diag(u1653_code(), format!(
                                "Field '{}' not found in record layout type {}",
                                field_name, type_id.0
                            ));
                            continue;
                        }
                    };

                    let field_layout = &rec_layout.fields[field_idx];
                    let sub_offset = base_offset + field_layout.offset as i32;
                    let sub_size_signed = (field_layout.size, field_layout.signed);

                    self.lower_pattern_inner(
                        sub_pattern,
                        base_reg,
                        sub_offset,
                        arm_id,
                        slot,
                        arena,
                        sub_size_signed,
                        pool,
                    );
                }
            }
        }
    }

    /// #1084 (follow-up): Extract pattern fields from a register (register-form enums).
    ///
    /// Mirrors `lower_pattern` but works from RDX (register source) instead of memory
    /// via RDI. Used for register-form enums (layout.size <= 16) where the payload
    /// is already in RDX after scrutinee load.
    ///
    /// For `Simple` leaves:
    /// - Emit `mov <dest>, <source_reg>`
    /// - If bit_offset > 0 and size < 8: emit `shr <dest>, bit_offset*8` (or `sar` if signed)
    /// - If size < 8: emit narrowing move (movzx, movsx, or plain mov reg32)
    /// - Insert `local_bindings[name] = dest`
    ///
    /// For nested patterns (Record, EnumVariant with payload):
    /// - Compute sub_offset = bit_offset + field.offset*8 (but stay within bit boundaries)
    /// - Recurse with adjusted bit_offset
    ///
    /// Scratch pool: [RCX, R8, R10, R11] (4 regs, excludes RDX which is source).
    /// Exhaustion → diagnostic (same as `lower_pattern`).
    pub(crate) fn lower_pattern_from_reg(
        &mut self,
        pattern: &paideia_as_ir::PatternBinding,
        source_reg: RegId,
        bit_offset: u32,  // Bit offset into source_reg (0, 32, etc.)
        arm_id: IrNodeId,
        slot: &mut u32,
        arena: &IrArena,
        default_size_signed: (u8, bool),
    ) {
        // #1216: pool snapshot at outer entry; all sibling binders share one pool.
        let base_pool = [abi::RCX, abi::R8, abi::R10, abi::R11];
        let live = self.state.local_bindings.live_regs();
        let pool: SmallVec<[RegId; 4]> =
            base_pool.iter().copied().filter(|r| !live.contains(r)).collect();
        self.lower_pattern_from_reg_inner(pattern, source_reg, bit_offset, arm_id, slot, arena, default_size_signed, &pool);
    }

    /// Private inner method for lower_pattern_from_reg that threads the register pool through recursion.
    /// Pool is snapshot once at outer entry and shared across all sibling binders.
    fn lower_pattern_from_reg_inner(
        &mut self,
        pattern: &paideia_as_ir::PatternBinding,
        source_reg: RegId,
        bit_offset: u32,  // Bit offset into source_reg (0, 32, etc.)
        arm_id: IrNodeId,
        slot: &mut u32,
        arena: &IrArena,
        default_size_signed: (u8, bool),
        pool: &[RegId],
    ) {
        use paideia_as_ir::PatternBinding;

        match pattern {
            PatternBinding::Wildcard => {
                // No-op: wildcard matches anything without binding
            }

            PatternBinding::Simple(name) => {
                let idx = *slot as usize;
                if idx >= pool.len() {
                    // U1654 — pool exhausted after outer live-reg filter
                    self.push_typed_diag(u1654_code(), format!(
                        "Nested pattern binding exhaustion: pool={} slot={} in arm {} (from register)",
                        pool.len(), *slot, arm_id.get()));
                    return;
                }
                let dest_reg = pool[idx];
                *slot += 1;

                let (size, signed) = default_size_signed;

                // Step 1: Emit `mov <dest>, <source_reg>` (always load full 64 bits first)
                let mov_id = self.alloc_synthetic_id();
                let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                mov_operands.push(Operand::Reg(dest_reg));
                mov_operands.push(Operand::Reg(source_reg));
                self.emit_inst(mov_id, Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: mov_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });

                // Step 2: If bit_offset > 0, emit shift to align field to LSBs
                // bit_offset is a compile-time constant, so use immediate-form shift encoding
                if bit_offset > 0 {
                    debug_assert!(bit_offset < 64, "shift amount must fit in 6 bits");
                    let shift_id = self.alloc_synthetic_id();
                    let mut shift_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                    shift_operands.push(Operand::Reg(dest_reg));
                    shift_operands.push(Operand::Imm64(bit_offset as i64));
                    let mnemonic = if signed { Mnemonic::Sar } else { Mnemonic::Shr };
                    self.emit_inst(shift_id, Instruction {
                        mnemonic,
                        operands: shift_operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    });
                }

                // Step 3: Emit narrowing move based on size/signedness
                // For register-to-register narrowing, use the appropriate mnemonic
                // without additional encoding hints (let the encoder handle register-form encoding)
                match (size, signed) {
                    (8, _) => {
                        // Full 64-bit; already in dest_reg, no narrowing needed
                    }
                    (4, false) => {
                        // Zero-extend: mov dest32, dest32 (implicit zero-extend in encoder)
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Mov,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    (4, true) => {
                        // Sign-extend: movsxd dest, dest32 (encoder handles via movsx with width=4)
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Movsx,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    (2, false) => {
                        // Zero-extend: movzx dest, dest16
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Movzx,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    (2, true) => {
                        // Sign-extend: movsx dest, dest16
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Movsx,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    (1, false) => {
                        // Zero-extend: movzx dest, dest8
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Movzx,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    (1, true) => {
                        // Sign-extend: movsx dest, dest8
                        let narrow_id = self.alloc_synthetic_id();
                        let mut narrow_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                        narrow_operands.push(Operand::Reg(dest_reg));
                        narrow_operands.push(Operand::Reg(dest_reg));
                        self.emit_inst(narrow_id, Instruction {
                            mnemonic: Mnemonic::Movsx,
                            operands: narrow_operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        });
                    }
                    _ => {
                        self.push_typed_diag(t0566_code(), format!("Unsupported field size/signed in register pattern: size={}, signed={}", size, signed));
                    }
                }

                // Insert binding into LocalBindingTable
                self.state.local_bindings.insert(name.clone(), dest_reg);
            }

            PatternBinding::EnumVariant {
                variant_index: _,
                payload_type,
                payload: Some(inner),
            } => {
                // #1084: In register form, payload record fields start at bit_offset (no additional offset).
                // The enum's discriminant is in RAX; payload is already in source_reg (RDX) at bit 0.
                let sub_bit_offset = bit_offset;

                let (sub_size, sub_signed) = if let Some(payload_type_id) = payload_type {
                    // If payload_type is a record, look up its layout
                    if let Some(rec_layout) = self.state.record_layout(*payload_type_id) {
                        // Use first field's size/signed as default for nested pattern
                        if let Some(first_field) = rec_layout.fields.first() {
                            (first_field.size, first_field.signed)
                        } else {
                            default_size_signed
                        }
                    } else {
                        // Layout not found; use default
                        default_size_signed
                    }
                } else {
                    default_size_signed
                };

                // Recurse with payload pattern
                self.lower_pattern_from_reg_inner(
                    inner,
                    source_reg,
                    sub_bit_offset,
                    arm_id,
                    slot,
                    arena,
                    (sub_size, sub_signed),
                    pool,
                );
            }

            PatternBinding::EnumVariant {
                payload: None,
                ..
            } => {
                // Unit variant; no payload to extract
            }

            PatternBinding::Record {
                type_id,
                fields,
            } => {
                // Look up record layout
                let rec_layout = match self.state.record_layout(*type_id) {
                    Some(l) => l.clone(),
                    None => {
                        self.push_typed_diag(u1652_code(), format!(
                            "No record layout found for nested pattern type {}",
                            type_id.0
                        ));
                        return;
                    }
                };

                // For each field, compute bit offset and recurse
                for (field_name, sub_pattern) in fields {
                    let field_idx = match rec_layout.field_index_by_name(field_name) {
                        Some(idx) => idx,
                        None => {
                            self.push_typed_diag(u1653_code(), format!(
                                "Field '{}' not found in record layout type {}",
                                field_name, type_id.0
                            ));
                            continue;
                        }
                    };

                    let field_layout = &rec_layout.fields[field_idx];
                    // Compute bit offset: byte_offset * 8 bits/byte + bit_offset
                    let sub_bit_offset = bit_offset + (field_layout.offset as u32) * 8;
                    let sub_size_signed = (field_layout.size, field_layout.signed);

                    self.lower_pattern_from_reg_inner(
                        sub_pattern,
                        source_reg,
                        sub_bit_offset,
                        arm_id,
                        slot,
                        arena,
                        sub_size_signed,
                        pool,
                    );
                }
            }
        }
    }

    /// PA-r15-009b (#1032): Emit jump-table dispatch for dense matches.
    ///
    /// Emits a 4-instruction sequence when `dispatch_meta.jump_table && dispatch_meta.density_ok`:
    /// - sub rax, min_arm         ; normalize arm value to 0..range
    /// - cmp rax, range           ; bounds check
    /// - ja _default_<match_id>   ; out of bounds → default arm
    /// - jmp [_jt_<match_id> + rax*8]  ; memory-indirect near jump via rodata
    ///
    /// Then emits arm bodies and default arm (no cmp/jne cascade).
    /// Arm labels are registered at the start of each arm body.
    /// End label registered at the end.
    fn visit_match_jump_table(
        &mut self,
        match_node_id: IrNodeId,
        arena: &IrArena,
        dispatch_meta: &paideia_as_ir::MatchDispatchMeta,
    ) {
        let children = arena.children(match_node_id);
        if children.is_empty() {
            self.push_typed_diag(u1650_code(), format!(
                "Match node {} has no children; expected scrutinee + arms",
                match_node_id.get()
            ));
            return;
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
        let _layout_payload_size = layout.payload_size;

        // #1084: Emit scrutinee load from module symbol or local binding
        self.emit_scrutinee_load(match_node_id, scrutinee_id, &layout, arena);

        // Emit discriminant load for stack form
        // PA-r15-009c: Use match_id * 1000 + N numbering so discriminant load (0)
        // sorts before dispatch sequence (1-4) and arm end jumps (100+).
        if layout_size > 16 {
            let disc_load_id = IrNodeId::new(match_node_id.get() * 1000 + 0)
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
        let jt_name = format!("_jt_{}", match_node_id.get());

        // Emit the 4-instruction jump-table dispatch sequence

        // 1. sub rax, min_arm
        if dispatch_meta.min_arm != 0 {
            let sub_id = IrNodeId::new(match_node_id.get() * 1000 + 1).expect("sub id");
            let mut sub_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            sub_operands.push(Operand::Reg(abi::RAX));
            sub_operands.push(Operand::Imm64(dispatch_meta.min_arm));

            self.emit_inst(sub_id, Instruction {
                mnemonic: Mnemonic::Sub,
                operands: sub_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
            emission_order: 0,
            });
        }

        // 2. cmp rax, range
        let cmp_id = IrNodeId::new(match_node_id.get() * 1000 + 2).expect("cmp id");
        let mut cmp_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        cmp_operands.push(Operand::Reg(abi::RAX));
        cmp_operands.push(Operand::Imm64(dispatch_meta.range as i64 - 1));

        self.emit_inst(cmp_id, Instruction {
            mnemonic: Mnemonic::Cmp,
            operands: cmp_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        emission_order: 0,
        });

        // 3. ja _default_<match_id>
        let ja_id = IrNodeId::new(match_node_id.get() * 1000 + 3).expect("ja id");
        let mut ja_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        ja_operands.push(Operand::LabelRef {
            name: default_label.clone(),
            addend: 0,
        });

        self.emit_inst(ja_id, Instruction {
            mnemonic: Mnemonic::Jcc(Cond::Above),
            operands: ja_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        emission_order: 0,
        });

        // 4. jmp [_jt_<match_id> + rax*8]
        let jmp_id = IrNodeId::new(match_node_id.get() * 1000 + 4).expect("jmp id");
        let mut jmp_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        jmp_operands.push(Operand::MemSymIndexed {
            name: jt_name,
            addend: 0,
            index: abi::RAX,
            scale: paideia_as_ir::instruction::Scale::X8,
        });

        self.emit_inst(jmp_id, Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: jmp_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
        emission_order: 0,
        });

        // Track whether we encounter an explicit default arm
        let mut default_arm_registered = false;

        // Now emit arm bodies and default arm
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

            // If default arm, skip to default label
            if arm_meta.is_default {
                self.state.register_label(default_label.clone());
                default_arm_registered = true;
                if let Some(arm_node) = arena.get(arm_id) {
                    match arm_node.kind {
                        IrKind::Action => self.emit_block_body_arm(arm_id, arena, None),
                        _ => {}
                    }
                }
                continue;
            }

            // Emit NOP anchor for arm label (jump table entries will jump here)
            // PA-r15-009c: Use M*1000 + 100 + idx*10 to sort before arm body instructions
            let arm_nop_id = IrNodeId::new(match_node_id.get() * 1000 + 100 + idx as u32 * 10)
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

            // Emit arm body
            if let Some(arm_node) = arena.get(arm_id) {
                match arm_node.kind {
                    IrKind::Action => self.emit_block_body_arm(arm_id, arena, None),
                    IrKind::Literal => {
                        if let Some(value) = arena.literal_values().get(arm_id) {
                            let mov_id = IrNodeId::new(match_node_id.get() * 1000 + 100 + idx as u32 * 10 + 4)
                                .expect("literal arm mov id");
                            self.emit_mov_literal_to_reg_with_id(mov_id, abi::RAX, value);
                        }
                    }
                    IrKind::Var => {
                        if let Some(var_name) = arena.binding_names().get(arm_id) {
                            if let Some(var_reg) = self.state.local_bindings.get(var_name) {
                                if var_reg != abi::RAX {
                                    let mov_id = IrNodeId::new(match_node_id.get() * 1000 + 100 + idx as u32 * 10 + 3)
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
                    _ => self.push_typed_diag(t0560_code(), format!(
                        "unsupported match arm body IR kind: {:?}", arm_node.kind)),
                }
            }

            // Emit jmp end
            // PA-r15-009c: Use 100 + idx*10 + 5 to sort after dispatch sequence (0-4)
            let jmp_end_id = IrNodeId::new(match_node_id.get() * 1000 + 100 + idx as u32 * 10 + 5)
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
        if !default_arm_registered {
            let default_nop_id = IrNodeId::new(match_node_id.get() * 1000 + 998)
                .expect("match default anchor NOP virtual id");
            self.emit_inst(default_nop_id, Instruction {
                mnemonic: Mnemonic::Nop,
                operands: SmallVec::new(),
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            });
            self.state.insert_label(default_label, default_nop_id);
        }

        // #1120: register end_label via label_to_instr (offset resolved post-sort)
        // instead of register_label (which captures walker-time estimated_offset).
        // Anchor the label to a 1-byte NOP whose IrNodeId is high enough to sort
        // after every arm-end jmp (M*1000 + 5..M*1000 + arm_count*10+5).
        let nop_id = IrNodeId::new(match_node_id.get() * 1000 + 999)
            .expect("match end anchor NOP virtual id");
        let nop_inst = Instruction {
            mnemonic: Mnemonic::Nop,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
};
        self.emit_inst(nop_id, nop_inst);
        self.state.insert_label(end_label, nop_id);
    }

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

        let _scrutinee_id = children[0];
        let arm_ids: Vec<IrNodeId> = children[1..].to_vec();

        if arm_ids.is_empty() {
            self.push_typed_diag(u1650_code(), format!(
                "Match node {} has scrutinee but no arms",
                match_node_id.get()
            ));
            return;
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
        self.emit_scrutinee_load(match_node_id, _scrutinee_id, &layout, arena);

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
                self.state.register_label(default_label.clone());
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
            self.state.register_label(arm_label);

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
                self.state.register_label(body_label);
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

    /// Emit arm body for IrKind::App (binary operator application).
    /// Handles nested-pattern match arms like `match r { Ok(Point{x,y}) => x + y, Err => 0u32 }`.
    fn emit_arm_body_app(&mut self, app_id: IrNodeId, arena: &IrArena, _match_id: IrNodeId, _idx: u32) {
        let children = arena.children(app_id);
        if children.len() < 3 {
            self.push_typed_diag(u1655_code(), format!(
                "App node {} has fewer than 3 children (callee, arg0, arg1)",
                app_id.get()
            ));
            return;
        }

        // Extract argument IDs; callee (children[0]) is looked up via call_sites() #1196
        let arg0_id = children[1];
        let arg1_id = children[2];

        // #1196: Use authoritative operator lexeme from call_sites()
        let operator = match operator_lexeme_of(arena, app_id) {
            Some(op) => op,
            None => {
                self.push_typed_diag(t0561_code(), format!("App node {}: operator lexeme not found in call_sites", app_id.get()));
                return;
            }
        };

        let arg0_node = match arena.get(arg0_id) {
            Some(n) => n,
            None => {
                self.push_typed_diag(u1655_code(), format!(
                    "App node {}'s arg0 child {} not found",
                    app_id.get(),
                    arg0_id.get()
                ));
                return;
            }
        };

        let arg1_node = match arena.get(arg1_id) {
            Some(n) => n,
            None => {
                self.push_typed_diag(u1655_code(), format!(
                    "App node {}'s arg1 child {} not found",
                    app_id.get(),
                    arg1_id.get()
                ));
                return;
            }
        };

        // Case-analyze operand shapes
        match (&arg0_node.kind, &arg1_node.kind) {
            (IrKind::Var, IrKind::Var) => {
                // (Var, Var): both arguments in registers
                let arg0_name = match arena.binding_names().get(arg0_id) {
                    Some(n) => n,
                    None => {
                        self.push_typed_diag(u1656_code(), format!(
                            "App node {} arg0 Var has no binding name",
                            app_id.get()
                        ));
                        return;
                    }
                };

                let arg1_name = match arena.binding_names().get(arg1_id) {
                    Some(n) => n,
                    None => {
                        self.push_typed_diag(u1656_code(), format!(
                            "App node {} arg1 Var has no binding name",
                            app_id.get()
                        ));
                        return;
                    }
                };

                let arg0_reg = match self.state.local_bindings.get(arg0_name) {
                    Some(reg) => reg,
                    None => {
                        self.push_typed_diag(u1657_code(), format!(
                            "App node {} arg0 var '{}' not in local_bindings",
                            app_id.get(),
                            arg0_name
                        ));
                        return;
                    }
                };

                let arg1_reg = match self.state.local_bindings.get(arg1_name) {
                    Some(reg) => reg,
                    None => {
                        self.push_typed_diag(u1657_code(), format!(
                            "App node {} arg1 var '{}' not in local_bindings",
                            app_id.get(),
                            arg1_name
                        ));
                        return;
                    }
                };

                // Emit: mov rax, arg0_reg
                let mov_id = self.alloc_synthetic_id();
                let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                mov_operands.push(Operand::Reg(abi::RAX));
                mov_operands.push(Operand::Reg(arg0_reg));
                self.emit_inst(mov_id, Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: mov_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });

                // Emit: op rax, arg1_reg
                let mnemonic = match operator {
                    "+" => Mnemonic::Add,
                    "-" => Mnemonic::Sub,
                    "*" => Mnemonic::Imul,
                    "&" => Mnemonic::And,
                    "|" => Mnemonic::Or,
                    "^" => Mnemonic::Xor,
                    "<<" => Mnemonic::Shl,
                    ">>" => Mnemonic::Shr,
                    _ => {
                        self.push_typed_diag(t0562_code(), format!("App node {}: unsupported operator '{}'", app_id.get(), operator));
                        return;
                    }
                };

                let op_id = self.alloc_synthetic_id();
                let mut op_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                op_operands.push(Operand::Reg(abi::RAX));
                op_operands.push(Operand::Reg(arg1_reg));
                self.emit_inst(op_id, Instruction {
                    mnemonic,
                    operands: op_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });
            }

            (IrKind::Var, IrKind::Literal) => {
                // (Var, Literal): arg0 in register, arg1 is immediate
                let arg0_name = match arena.binding_names().get(arg0_id) {
                    Some(n) => n,
                    None => {
                        self.push_typed_diag(u1656_code(), format!(
                            "App node {} arg0 Var has no binding name",
                            app_id.get()
                        ));
                        return;
                    }
                };

                let arg1_value = match arena.literal_values().get(arg1_id) {
                    Some(val) => val,
                    None => {
                        self.push_typed_diag(u1656_code(), format!(
                            "App node {} arg1 Literal has no value",
                            app_id.get()
                        ));
                        return;
                    }
                };

                let arg0_reg = match self.state.local_bindings.get(arg0_name) {
                    Some(reg) => reg,
                    None => {
                        self.push_typed_diag(u1657_code(), format!(
                            "App node {} arg0 var '{}' not in local_bindings",
                            app_id.get(),
                            arg0_name
                        ));
                        return;
                    }
                };

                // Emit: mov rax, arg0_reg
                let mov_id = self.alloc_synthetic_id();
                let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                mov_operands.push(Operand::Reg(abi::RAX));
                mov_operands.push(Operand::Reg(arg0_reg));
                self.emit_inst(mov_id, Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: mov_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });

                // Emit: op rax, imm64
                let mnemonic = match operator {
                    "+" => Mnemonic::Add,
                    "-" => Mnemonic::Sub,
                    "*" => Mnemonic::Imul,
                    "&" => Mnemonic::And,
                    "|" => Mnemonic::Or,
                    "^" => Mnemonic::Xor,
                    "<<" => Mnemonic::Shl,
                    ">>" => Mnemonic::Shr,
                    _ => {
                        self.push_typed_diag(t0562_code(), format!("App node {}: unsupported operator '{}'", app_id.get(), operator));
                        return;
                    }
                };

                let op_id = self.alloc_synthetic_id();
                let mut op_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                op_operands.push(Operand::Reg(abi::RAX));
                op_operands.push(Operand::Imm64(arg1_value));
                self.emit_inst(op_id, Instruction {
                    mnemonic,
                    operands: op_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });
            }

            (IrKind::Literal, IrKind::Var) => {
                // (Literal, Var): arg0 is immediate, arg1 in register
                let arg0_value = match arena.literal_values().get(arg0_id) {
                    Some(val) => val,
                    None => {
                        self.push_typed_diag(u1656_code(), format!(
                            "App node {} arg0 Literal has no value",
                            app_id.get()
                        ));
                        return;
                    }
                };

                let arg1_name = match arena.binding_names().get(arg1_id) {
                    Some(n) => n,
                    None => {
                        self.push_typed_diag(u1656_code(), format!(
                            "App node {} arg1 Var has no binding name",
                            app_id.get()
                        ));
                        return;
                    }
                };

                let arg1_reg = match self.state.local_bindings.get(arg1_name) {
                    Some(reg) => reg,
                    None => {
                        self.push_typed_diag(u1657_code(), format!(
                            "App node {} arg1 var '{}' not in local_bindings",
                            app_id.get(),
                            arg1_name
                        ));
                        return;
                    }
                };

                // Emit: mov rax, imm64
                let mov_id = self.alloc_synthetic_id();
                let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                mov_operands.push(Operand::Reg(abi::RAX));
                mov_operands.push(Operand::Imm64(arg0_value));
                self.emit_inst(mov_id, Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: mov_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });

                // Emit: op rax, arg1_reg
                let mnemonic = match operator {
                    "+" => Mnemonic::Add,
                    "-" => Mnemonic::Sub,
                    "*" => Mnemonic::Imul,
                    "&" => Mnemonic::And,
                    "|" => Mnemonic::Or,
                    "^" => Mnemonic::Xor,
                    "<<" => Mnemonic::Shl,
                    ">>" => Mnemonic::Shr,
                    _ => {
                        self.push_typed_diag(t0562_code(), format!("App node {}: unsupported operator '{}'", app_id.get(), operator));
                        return;
                    }
                };

                let op_id = self.alloc_synthetic_id();
                let mut op_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                op_operands.push(Operand::Reg(abi::RAX));
                op_operands.push(Operand::Reg(arg1_reg));
                self.emit_inst(op_id, Instruction {
                    mnemonic,
                    operands: op_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });
            }

            (IrKind::Literal, IrKind::Literal) => {
                // (Literal, Literal): both arguments are constants, constant-fold
                let arg0_value = match arena.literal_values().get(arg0_id) {
                    Some(val) => val,
                    None => {
                        self.push_typed_diag(u1656_code(), format!(
                            "App node {} arg0 Literal has no value",
                            app_id.get()
                        ));
                        return;
                    }
                };

                let arg1_value = match arena.literal_values().get(arg1_id) {
                    Some(val) => val,
                    None => {
                        self.push_typed_diag(u1656_code(), format!(
                            "App node {} arg1 Literal has no value",
                            app_id.get()
                        ));
                        return;
                    }
                };

                let result = match operator {
                    "+" => arg0_value.wrapping_add(arg1_value),
                    "-" => arg0_value.wrapping_sub(arg1_value),
                    "*" => arg0_value.wrapping_mul(arg1_value),
                    "&" => arg0_value & arg1_value,
                    "|" => arg0_value | arg1_value,
                    "^" => arg0_value ^ arg1_value,
                    "<<" => arg0_value.wrapping_shl(arg1_value as u32),
                    ">>" => arg0_value.wrapping_shr(arg1_value as u32),
                    _ => {
                        self.push_typed_diag(t0562_code(), format!("App node {}: unsupported operator '{}'", app_id.get(), operator));
                        return;
                    }
                };

                // Emit: mov rax, result
                let mov_id = self.alloc_synthetic_id();
                let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                mov_operands.push(Operand::Reg(abi::RAX));
                mov_operands.push(Operand::Imm64(result));
                self.emit_inst(mov_id, Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: mov_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });
            }

            _ => {
                self.push_typed_diag(t0563_code(), format!("App node {}: unsupported operand kinds ({:?}, {:?})", app_id.get(), arg0_node.kind, arg1_node.kind));
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{MatchDispatchMeta, EnumTypeId};
    use paideia_as_diagnostics::{FileId, Span};

    fn span() -> Span {
        Span::new(FileId::new(1).unwrap(), 0, 1)
    }

    /// PA-r15-009b unit test: jump-table codegen emits 4-instruction sequence.
    #[test]
    fn visit_match_jump_table_emits_dispatch_sequence() {
        // Construct a minimal IR arena with a Match node
        let mut arena = IrArena::new();

        // Allocate a Match node with a single Scrutinee and one Arm
        let scrutinee_id = arena.alloc(IrKind::Var, span());
        let arm_body_id = arena.alloc(IrKind::Literal, span());
        let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
        let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

        // Register scrutinee type
        arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));

        // Register dispatch metadata: dense, jump_table enabled
        arena.match_dispatch_meta_mut().insert(
            match_id,
            MatchDispatchMeta {
                jump_table: true,
                min_arm: 0,
                range: 4,
                covered_arms: 4,
                density_ok: true,
            },
        );

        // Register per-arm values: value 0 at arm index 0
        arena.match_jump_table_arm_values_mut().insert(
            match_id,
            vec![(0, 0)],
        );

        // Register arm metadata: non-default arm with index 0
        let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
        arm_meta.is_default = false;
        arm_meta.variant_index = Some(0);
        arena.match_arm_meta_mut().insert(arm_id, arm_meta);

        // Register literal value for the arm body
        arena.literal_values_mut().insert(arm_body_id, 0x1234i64);

        // Construct an EmitWalker with a mock state that provides enum layout
        let mut walker = EmitWalker::new();
        walker.state.insert_enum_layout(
            EnumTypeId(0),
            paideia_as_ir::EnumLayout::new(0),
        );

        // Call visit_match_jump_table
        walker.visit_match_jump_table(match_id, &arena, &MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 4,
            covered_arms: 4,
            density_ok: true,
        });

        // Verify the instruction sequence
        // PA-r15-009c: Sort instruction IDs to avoid HashMap order randomization
        // (production text_emitter.rs also sorts for deterministic ordering)
        let mut node_ids: Vec<_> = walker.state.instructions.entries().keys().copied().collect();
        node_ids.sort();

        let instructions: Vec<&Instruction> = node_ids.iter()
            .filter_map(|node_id| walker.state.instructions.get(*node_id))
            .collect();

        // We expect at least: cmp, ja, jmp (+ possibly arm body literal load)
        // The exact count may vary based on the arm body emission, but the key sequence should be there.
        assert!(!instructions.is_empty(), "Should emit instructions");

        // Find the dispatch sequence (look for cmp followed by ja/jmp)
        let mut found_cmp = false;
        let mut found_ja = false;
        let mut found_jmp_memidx = false;

        for instr in instructions.iter() {
            if instr.mnemonic == Mnemonic::Cmp {
                found_cmp = true;
            }
            if found_cmp && !found_ja && matches!(instr.mnemonic, Mnemonic::Jcc(Cond::Above)) {
                found_ja = true;
            }
            if found_ja && instr.mnemonic == Mnemonic::Jmp {
                // Check if the operand is MemSymIndexed
                if let Some(Operand::MemSymIndexed { name, index, scale, .. }) = instr.operands.first() {
                    if name.starts_with("_jt_") && *index == abi::RAX && *scale == paideia_as_ir::instruction::Scale::X8 {
                        found_jmp_memidx = true;
                    }
                }
            }
        }

        assert!(found_cmp, "Should emit cmp instruction");
        assert!(found_ja, "Should emit ja (Above) conditional jump");
        assert!(found_jmp_memidx, "Should emit jmp with MemSymIndexed operand");
    }

    /// PA-r15-009b fallback test: non-dense match uses cmp/jne cascade.
    #[test]
    fn visit_match_fallback_uses_cmp_jne_cascade() {
        let mut arena = IrArena::new();

        // Allocate a Match node
        let scrutinee_id = arena.alloc(IrKind::Var, span());
        let arm_body_id = arena.alloc(IrKind::Literal, span());
        let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
        let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

        // Register scrutinee type
        arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));

        // Register dispatch metadata: sparse (density_ok = false)
        arena.match_dispatch_meta_mut().insert(
            match_id,
            MatchDispatchMeta {
                jump_table: true,
                min_arm: 0,
                range: 100, // Large range, low density
                covered_arms: 1,
                density_ok: false, // Not dense enough
            },
        );

        arena.match_jump_table_arm_values_mut().insert(
            match_id,
            vec![(0, 0)],
        );

        let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
        arm_meta.is_default = false;
        arm_meta.variant_index = Some(0);
        arena.match_arm_meta_mut().insert(arm_id, arm_meta);

        arena.literal_values_mut().insert(arm_body_id, 0x5678i64);

        let mut walker = EmitWalker::new();
        walker.state.insert_enum_layout(
            EnumTypeId(0),
            paideia_as_ir::EnumLayout::new(0),
        );

        // Call visit_match (not visit_match_jump_table, since density_ok = false)
        // PA-r17-013 (#991): pass TailContext::Discard since this is a top-level test
        walker.visit_match(match_id, &arena, None, TailContext::Discard);

        let instructions: Vec<&Instruction> = walker.state.instructions.iter()
            .map(|(_, instr)| instr)
            .collect();

        // Should NOT contain MemSymIndexed or _jt_ symbols
        let has_memsymindexed = instructions.iter().any(|instr| {
            matches!(instr.mnemonic, Mnemonic::Jmp) &&
            instr.operands.first().map(|op| matches!(op, Operand::MemSymIndexed { .. })).unwrap_or(false)
        });
        assert!(!has_memsymindexed, "Sparse match should not use MemSymIndexed jmp");

        // Should use cmp/jne cascade instead
        let has_cmp_jne = instructions.iter().any(|instr| {
            matches!(instr.mnemonic, Mnemonic::Cmp)
        }) && instructions.iter().any(|instr| {
            matches!(instr.mnemonic, Mnemonic::Jcc(Cond::Ne))
        });
        assert!(has_cmp_jne, "Sparse match should use cmp/jne cascade");
    }

    /// #1120: Verify end_label is registered via label_to_instr (jump-table path)
    #[test]
    fn end_label_resolves_via_label_to_instr_jump_table() {
        let mut arena = IrArena::new();

        let scrutinee_id = arena.alloc(IrKind::Var, span());
        let arm_body_id = arena.alloc(IrKind::Literal, span());
        let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
        let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

        arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));
        arena.match_dispatch_meta_mut().insert(
            match_id,
            MatchDispatchMeta {
                jump_table: true,
                min_arm: 0,
                range: 4,
                covered_arms: 4,
                density_ok: true,
            },
        );
        arena.match_jump_table_arm_values_mut().insert(match_id, vec![(0, 0)]);

        let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
        arm_meta.is_default = false;
        arm_meta.variant_index = Some(0);
        arena.match_arm_meta_mut().insert(arm_id, arm_meta);
        arena.literal_values_mut().insert(arm_body_id, 0x1234i64);

        let mut walker = EmitWalker::new();
        walker.state.insert_enum_layout(EnumTypeId(0), paideia_as_ir::EnumLayout::new(0));

        walker.visit_match_jump_table(match_id, &arena, &MatchDispatchMeta {
            jump_table: true,
            min_arm: 0,
            range: 4,
            covered_arms: 4,
            density_ok: true,
        });

        let end_label = format!("match_end_{}", match_id.get());
        // #1120: end_label must be in label_to_instr, not labels
        assert!(walker.state().label_to_instr.contains_key(&end_label),
            "#1120: jump-table end_label must use label_to_instr");
        assert!(!walker.state().labels.contains_key(&end_label),
            "#1120: jump-table end_label should not be in labels (walker-time offsets)");

        // #1120: Verify the NOP anchor's byte offset is correct via real encoding
        let mut table = walker.state().instructions().clone();
        let mut buf = Vec::new();
        let result = paideia_as_emitter_pe::emit_text_from_instructions(&mut table, &mut buf)
            .expect("encode should succeed");

        let &nop_id = walker.state().label_to_instr().get(&end_label)
            .expect("#1120: end_label must resolve via label_to_instr");
        let &nop_offset = result.offset_map.get(&nop_id)
            .expect("#1120: NOP anchor must have an offset_map entry");

        assert_eq!(
            nop_offset as usize,
            buf.len() - 1,
            "#1120: end_label NOP anchor must be the LAST byte in .text \
             (sort-order proof — this is what the walker-order bug broke). \
             Actual: nop at offset {}, buf.len()={}",
            nop_offset,
            buf.len()
        );
    }

    /// #1120: Verify end_label is registered via label_to_instr (cmp/jne cascade path)
    #[test]
    fn end_label_resolves_via_label_to_instr_cmp_jne() {
        let mut arena = IrArena::new();

        let scrutinee_id = arena.alloc(IrKind::Var, span());
        let arm_body_id = arena.alloc(IrKind::Literal, span());
        let arm_id = arena.alloc_with_children(IrKind::Action, span(), [arm_body_id]);
        let match_id = arena.alloc_with_children(IrKind::Match, span(), [scrutinee_id, arm_id]);

        arena.match_scrutinee_table_mut().insert(match_id, EnumTypeId(0));
        // density_ok = false forces cmp/jne cascade
        arena.match_dispatch_meta_mut().insert(
            match_id,
            MatchDispatchMeta {
                jump_table: true,
                min_arm: 0,
                range: 100,
                covered_arms: 1,
                density_ok: false, // Force cmp/jne cascade
            },
        );
        arena.match_jump_table_arm_values_mut().insert(match_id, vec![(0, 0)]);

        let mut arm_meta = paideia_as_ir::MatchArmMeta::default();
        arm_meta.is_default = false;
        arm_meta.variant_index = Some(0);
        arena.match_arm_meta_mut().insert(arm_id, arm_meta);
        arena.literal_values_mut().insert(arm_body_id, 0x5678i64);

        let mut walker = EmitWalker::new();
        walker.state.insert_enum_layout(EnumTypeId(0), paideia_as_ir::EnumLayout::new(0));

        walker.visit_match(match_id, &arena, None, TailContext::Discard);

        let end_label = format!("match_end_{}", match_id.get());
        // #1120: end_label must be in label_to_instr, not labels
        assert!(walker.state().label_to_instr.contains_key(&end_label),
            "#1120: cmp/jne end_label must use label_to_instr");
        assert!(!walker.state().labels.contains_key(&end_label),
            "#1120: cmp/jne end_label should not be in labels (walker-time offsets)");

        // #1120: Verify the NOP anchor's byte offset is correct via real encoding
        let mut table = walker.state().instructions().clone();
        let mut buf = Vec::new();
        let result = paideia_as_emitter_pe::emit_text_from_instructions(&mut table, &mut buf)
            .expect("encode should succeed");

        let &nop_id = walker.state().label_to_instr().get(&end_label)
            .expect("#1120: end_label must resolve via label_to_instr");
        let &nop_offset = result.offset_map.get(&nop_id)
            .expect("#1120: NOP anchor must have an offset_map entry");

        assert_eq!(
            nop_offset as usize,
            buf.len() - 1,
            "#1120: end_label NOP anchor must be the LAST byte in .text \
             (sort-order proof — this is what the walker-order bug broke). \
             Actual: nop at offset {}, buf.len()={}",
            nop_offset,
            buf.len()
        );
    }
}

