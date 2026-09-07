//! Scrutinee load + stack-form discriminant extraction.
//!
//! - `emit_scrutinee_load` (#1084): prepare RAX (+ RDX for register-form,
//!   or RDI for stack-form) from a Var scrutinee before match dispatch or
//!   discriminant extraction.
//! - `visit_enum_discriminant` (PA-r17-008): extract the discriminant of
//!   an enum value; for register-form it is already in RAX, for stack-form
//!   we emit `mov rax, [rdi+0]`.
//!
//! Extracted verbatim from the pre-split `emit_enum_match.rs` (issue #1409).

use paideia_as_ir::instruction::{Instruction, IntWidth, Mnemonic, Operand};
use paideia_as_ir::{abi, EnumLayout, IrArena, IrKind, IrNodeId, SmallVec, SymbolKind};

use crate::emit_walker::EmitWalker;

use super::diagnostics::{t0556_code, u1648_code, u1649_code, u1658_code};

impl EmitWalker {
    /// #1084: Emit scrutinee load for match expressions.
    ///
    /// For Var scrutinees:
    /// - Module Object symbol: emit `mov rax, [rip+sym]` + optional
    ///   `mov rdx, [rip+sym+payload_offset]` for register-form enums.
    /// - Local binding: emit register-to-register moves.
    /// - Missing binding_names: silent no-op (#1133 compat for test-fixture Vars).
    /// - Non-Var: T0556 diagnostic.
    pub(super) fn emit_scrutinee_load(
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
}
