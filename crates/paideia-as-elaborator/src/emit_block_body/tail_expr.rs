//! `emit_tail_expr`: tail-position placement of a small set of expression
//! kinds per `TailContext`. Used by the match-arm and lambda-body paths
//! to ensure the result lands in RAX / RAX:RDX / `[RDI+disp]` as required
//! by the enclosing return convention.
//!
//! Extracted verbatim from the pre-split `emit_block_body.rs`
//! (issue #1410). No behavior change.

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi};

use crate::emit_walker::EmitWalker;

use super::TailContext;

impl EmitWalker {
    /// PA-r17-013 (#991): Emit a tail expression in the proper context.
    ///
    /// For trailing expressions in match arms and lambda bodies, this ensures
    /// the result lands in RAX (or RAX:RDX / [RDI+disp] per the tail context).
    ///
    /// Handles:
    /// - Literal: emit Mov [tail_reg], Imm64(v)
    /// - Var: emit Mov RAX, <var_reg>
    /// - EnumCons: recurse via visit_enum_cons (writes RAX/RDX or [RDI+disp])
    /// - Match: recurse via visit_match with tail context
    /// - Branch: recurse into arms with tail propagation
    #[allow(dead_code)]
    pub(crate) fn emit_tail_expr(
        &mut self,
        tail_id: IrNodeId,
        arena: &IrArena,
        typer: Option<&paideia_as_types::TypeInterner>,
        tail: TailContext,
    ) {
        if let Some(node) = arena.get(tail_id) {
            match node.kind {
                IrKind::Literal => {
                    if let Some(value) = arena.literal_values().get(tail_id) {
                        match tail {
                            TailContext::ReturnRax => {
                                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                operands.push(Operand::Reg(abi::RAX));
                                operands.push(Operand::Imm64(value));
                                let inst = Instruction {
                                    mnemonic: Mnemonic::Mov,
                                    operands,
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                emission_order: 0,
                                };
                                self.emit_inst(tail_id, inst);
                            }
                            TailContext::ReturnRaxRdx => {
                                // For small enum (≤16 bytes), put discriminant in RAX
                                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                operands.push(Operand::Reg(abi::RAX));
                                operands.push(Operand::Imm64(value));
                                let inst = Instruction {
                                    mnemonic: Mnemonic::Mov,
                                    operands,
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                emission_order: 0,
                                };
                                self.emit_inst(tail_id, inst);
                            }
                            TailContext::ReturnIndirect { disc_size: _ } => {
                                // For large enum (>16 bytes), write to [RDI+0] for discriminant
                                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                operands.push(Operand::MemSib {
                                    base: abi::RDI,
                                    index: None,
                                    scale: paideia_as_ir::instruction::Scale::X1,
                                    disp: 0,
                                });
                                operands.push(Operand::Imm64(value));
                                let inst = Instruction {
                                    mnemonic: Mnemonic::Mov,
                                    operands,
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                emission_order: 0,
                                };
                                self.emit_inst(tail_id, inst);
                            }
                            TailContext::Discard => {
                                // Discarded: emit mov rax, imm (standard path)
                                let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                                operands.push(Operand::Reg(abi::RAX));
                                operands.push(Operand::Imm64(value));
                                let inst = Instruction {
                                    mnemonic: Mnemonic::Mov,
                                    operands,
                                    encoding_hint: None,
                                    byte_offset_in_text: None,
                                    mode: self.current_mode(),
                                emission_order: 0,
                                };
                                self.emit_inst(tail_id, inst);
                            }
                        }
                    }
                }
                IrKind::Var => {
                    // Load variable from its binding and move to RAX
                    if let Some(var_name) = arena.binding_names().get(tail_id) {
                        if let Some(var_reg) = self.state.local_bindings.get(var_name) {
                            // Move from var_reg to RAX
                            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
                            operands.push(Operand::Reg(abi::RAX));
                            operands.push(Operand::Reg(var_reg));
                            let inst = Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                            emission_order: 0,
                            };
                            self.emit_inst(tail_id, inst);
                        }
                    }
                }
                IrKind::EnumCons => {
                    // Delegate to visit_enum_cons which already handles RAX/RDX or [RDI+disp]
                    self.visit_enum_cons(tail_id, arena);
                }
                IrKind::Match => {
                    // Recurse via visit_match with tail context
                    self.visit_match(tail_id, arena, typer, tail);
                    self.state.mark_match_emitted(tail_id.get());
                }
                IrKind::Branch => {
                    // Recurse into branch with tail context (not implemented yet, deferred)
                    // For now, just skip to avoid panics
                }
                _ => {
                    // Other expression kinds deferred
                }
            }
        }
    }
}
