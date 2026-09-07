//! PA-r15-009b (#1032): Dense-match jump-table dispatch.
//!
//! Emits the 4-instruction dense dispatch sequence
//! (`sub` / `cmp` / `ja` / `jmp [table + rax*8]`) followed by per-arm
//! NOP-anchored bodies. Called from `visit_match` when the arena's
//! `MatchDispatchMeta` has `jump_table && density_ok`.
//!
//! Extracted verbatim from the pre-split `emit_enum_match.rs` (issue #1409).

use paideia_as_ir::instruction::{Cond, Instruction, Mnemonic, Operand};
use paideia_as_ir::{abi, IrArena, IrKind, IrNodeId, SmallVec};

use crate::emit_walker::EmitWalker;

use super::diagnostics::{t0556_code, t0560_code, u1649_code, u1650_code, u1651_code};

impl EmitWalker {
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
    pub(super) fn visit_match_jump_table(
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
}
