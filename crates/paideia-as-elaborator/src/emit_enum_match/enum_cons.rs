//! Enum variant constructor lowering.
//!
//! - `visit_enum_cons` (PA-r17-007): guarded entry point that skips
//!   emission for EnumCons nodes the data_encoder pre-pass has already
//!   materialized as static data (#1084, #1145).
//! - `emit_enum_cons_inner`: unconditional body — emits the register-pair
//!   or indirect passing convention move sequence for a variant.
//!
//! Extracted verbatim from the pre-split `emit_enum_match.rs` (issue #1409).

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::{abi, IrArena, IrKind, IrNodeId, PassingConvention, SmallVec};

use crate::emit_walker::EmitWalker;

use super::diagnostics::{t0559_code, u1648_code, u1649_code};

impl EmitWalker {
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
}
