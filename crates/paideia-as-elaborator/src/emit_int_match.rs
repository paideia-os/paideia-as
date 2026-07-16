//! Integer-scrutinee match emission (#1210).
//!
//! Emits cmp/jne cascade for match expressions over integer types.
//! Provides `visit_int_match` as the counterpart to `visit_match` for enum scrutinees.

use paideia_as_ir::instruction::{Cond, Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi, IntMatchScrutinee, SymbolKind};
use paideia_as_diagnostics::{DiagnosticCode, Category, Severity};

use crate::emit_block_body::TailContext;
use crate::emit_walker::EmitWalker;

/// Helper to construct T0556 diagnostic code.
fn t0556_code() -> DiagnosticCode {
    DiagnosticCode::new(Category::T, Severity::Error, 556)
        .expect("T0556 is within valid T range")
}

impl EmitWalker {
    /// Emit scrutinee load for integer-scrutinee match expressions (#1210).
    ///
    /// Supports:
    /// - Literal integers → mov rax, imm64
    /// - Var (local binding) → mov rax, reg (single-register only, not pair)
    /// - Var (module object symbol) → mov rax, [rip+sym]
    ///
    /// Never modifies RDX (preserves pair bindings from #1213).
    fn emit_int_scrutinee_load(
        &mut self,
        scrutinee_id: IrNodeId,
        _int_meta: IntMatchScrutinee,
        arena: &IrArena,
    ) {
        // #1210 contract: integer match never writes RDX; #1213 pair bindings remain intact
        debug_assert!(true, "integer match never writes RDX");

        let scrutinee_node = match arena.get(scrutinee_id) {
            Some(n) => n,
            None => return,
        };

        match scrutinee_node.kind {
            IrKind::Literal => {
                // Literal integer: extract value and emit mov rax, imm64
                let value = match arena.literal_values().get(scrutinee_id) {
                    Some(v) => v,
                    None => return,
                };

                let load_id = self.alloc_synthetic_id();
                let operands = {
                    let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                    ops.push(Operand::Reg(abi::RAX));
                    ops.push(Operand::Imm64(value));
                    ops
                };

                self.emit_inst(
                    load_id,
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
            IrKind::Var => {
                // Var: try local binding first, then module symbol
                let name = match arena.binding_names().get(scrutinee_id) {
                    Some(n) => n.to_string(),
                    None => return,
                };

                // Try local binding (single-register only; NOT get_pair)
                if let Some(src_reg) = self.state.local_bindings.get(&name) {
                    let load_id = self.alloc_synthetic_id();
                    let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                    operands.push(Operand::Reg(abi::RAX));
                    operands.push(Operand::Reg(src_reg));

                    self.emit_inst(
                        load_id,
                        Instruction {
                            mnemonic: Mnemonic::Mov,
                            operands,
                            encoding_hint: None,
                            byte_offset_in_text: None,
                            mode: self.current_mode(),
                            emission_order: 0,
                        },
                    );
                } else {
                    // Try module symbol (RIP-relative addressing)
                    if let Some(sym) = arena.symbols().lookup_by_name(&name) {
                        if sym.kind == SymbolKind::Object {
                            let load_id = self.alloc_synthetic_id();
                            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                            operands.push(Operand::Reg(abi::RAX));
                            operands.push(Operand::MemRipRelSym {
                                name: name.clone(),
                                addend: 0,
                            });

                            self.emit_inst(
                                load_id,
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
                    }
                    // Silent no-op if not found (like emit_scrutinee_load for enums)
                }
            }
            _ => {
                self.push_typed_diag(
                    t0556_code(),
                    format!("unsupported int match scrutinee kind: {:?}", scrutinee_node.kind),
                );
            }
        }
    }

    /// Emit a match arm body based on its IR node kind.
    fn emit_match_arm(
        &mut self,
        arm_id: IrNodeId,
        kind: IrKind,
        arena: &IrArena,
        typer: Option<&paideia_as_types::TypeInterner>,
    ) {
        match kind {
            IrKind::Action => {
                // Action (Block) arm: emit all statements + tail
                self.emit_block_body_arm(arm_id, arena, typer)
            }
            IrKind::Literal => {
                // Literal arm: load value into RAX
                if let Some(value) = arena.literal_values().get(arm_id) {
                    let load_id = self.alloc_synthetic_id();
                    let operands = {
                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        ops.push(Operand::Reg(abi::RAX));
                        ops.push(Operand::Imm64(value));
                        ops
                    };

                    self.emit_inst(
                        load_id,
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
            }
            _ => {}
        }
    }

    /// Emit match expression over integer scrutinee using cmp/jne cascade (#1210).
    ///
    /// Walks arms in order, emitting:
    /// 1. Scrutinee load: `mov rax, <value or reg>`
    /// 2. Cascade of cmp/jne pairs per arm
    /// 3. Default arm body (or NOP anchor if no default)
    /// 4. End label
    pub(crate) fn visit_int_match(
        &mut self,
        match_node_id: IrNodeId,
        arena: &IrArena,
        typer: Option<&paideia_as_types::TypeInterner>,
        _tail: TailContext,
        _int_meta: IntMatchScrutinee,
    ) {
        // Mark this match as emitted in tail position
        self.state.mark_match_emitted(match_node_id.get());

        let children = arena.children(match_node_id);
        if children.is_empty() {
            return;
        }

        let scrutinee_id = children[0];
        let arm_ids: Vec<IrNodeId> = children[1..].to_vec();

        if arm_ids.is_empty() {
            return;
        }

        // Emit scrutinee load
        self.emit_int_scrutinee_load(scrutinee_id, _int_meta, arena);

        // Label names
        let default_label = format!("match_default_{}", match_node_id.get());
        let end_label = format!("match_end_{}", match_node_id.get());

        // Track whether we saw a default arm to properly register the default_label
        let mut default_arm_registered = false;

        // Emit arms with cmp/jne cascade
        for (idx, &arm_id) in arm_ids.iter().enumerate() {
            let arm_meta = match arena.match_arm_meta().get(arm_id) {
                Some(m) => m,
                None => continue,
            };

            let arm_label = format!("match_arm_{}_{}", match_node_id.get(), idx);

            // If default arm, skip comparisons and emit body directly
            if arm_meta.is_default {
                self.state.register_label(default_label.clone());
                default_arm_registered = true;
                // Emit arm body based on its IR kind
                if let Some(arm_node) = arena.get(arm_id) {
                    self.emit_match_arm(arm_id, arm_node.kind, arena, typer);
                }
                continue;
            }

            // #1199: Register arm label at the START of the discriminator check
            self.state.register_label(arm_label);

            // Non-default arm: emit cmp rax, pattern_value
            if let Some(pattern_value) = arm_meta.int_pattern_value {
                let cmp_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10)
                    .expect("cmp id");
                let mut cmp_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                cmp_operands.push(Operand::Reg(abi::RAX)); // RAX
                cmp_operands.push(Operand::Imm64(pattern_value));

                self.emit_inst(cmp_id, Instruction {
                    mnemonic: Mnemonic::Cmp,
                    operands: cmp_operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                });

                // Emit jne to next arm or default
                // #1214: If the successor arm is a default (`_ =>`), it registers only
                // `default_label`, not `match_arm_N_(idx+1)`. Route jne to whichever label
                // that arm will actually register.
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

                let jne_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 1)
                    .expect("jne id");
                let mut jne_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                jne_operands.push(Operand::LabelRef {
                    name: next_label,
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
            }

            // Emit arm body based on its IR kind
            if let Some(arm_node) = arena.get(arm_id) {
                self.emit_match_arm(arm_id, arm_node.kind, arena, typer);
            }

            // Emit jmp to end
            let jmp_id = IrNodeId::new(match_node_id.get() * 100 + idx as u32 * 10 + 3)
                .expect("jmp id");
            let mut jmp_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            jmp_operands.push(Operand::LabelRef {
                name: end_label.clone(),
                addend: 0,
            });

            self.emit_inst(jmp_id, Instruction {
                mnemonic: Mnemonic::Jmp,
                operands: jmp_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            });
        }

        // After loop: if !default_arm_registered, emit NOP anchor at default_label
        if !default_arm_registered {
            let nop_id = IrNodeId::new(match_node_id.get() * 100 + 997)
                .expect("nop id");
            let nop_operands: SmallVec<[Operand; 3]> = SmallVec::new();

            self.emit_inst(nop_id, Instruction {
                mnemonic: Mnemonic::Nop,
                operands: nop_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            });

            self.state.register_label(default_label);
        }

        // End: NOP anchor at end_label
        let end_nop_id = IrNodeId::new(match_node_id.get() * 100 + 999)
            .expect("end nop id");
        let end_nop_operands: SmallVec<[Operand; 3]> = SmallVec::new();

        self.emit_inst(end_nop_id, Instruction {
            mnemonic: Mnemonic::Nop,
            operands: end_nop_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        });

        self.state.register_label(end_label);
    }
}
