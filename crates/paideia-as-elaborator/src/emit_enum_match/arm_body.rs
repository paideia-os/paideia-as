//! Emit arm body for `IrKind::App` (binary-operator application).
//!
//! Handles nested-pattern match arms like
//! `match r { Ok(Point{x,y}) => x + y, Err => 0u32 }` — the sum in the
//! Ok arm reaches here as an App node. Case-analyses on operand shape
//! (Var/Literal × Var/Literal) and emits the two-instruction
//! `mov rax, <a>; <op> rax, <b>` sequence (or a constant-folded `mov`
//! for Literal/Literal).
//!
//! Extracted verbatim from the pre-split `emit_enum_match.rs` (issue #1409).

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::{abi, IrArena, IrKind, IrNodeId, SmallVec};

use crate::emit_store_record::operator_lexeme_of;
use crate::emit_walker::EmitWalker;

use super::diagnostics::{
    t0561_code, t0562_code, t0563_code, u1655_code, u1656_code, u1657_code,
};

impl EmitWalker {
    /// Emit arm body for IrKind::App (binary operator application).
    /// Handles nested-pattern match arms like `match r { Ok(Point{x,y}) => x + y, Err => 0u32 }`.
    pub(super) fn emit_arm_body_app(&mut self, app_id: IrNodeId, arena: &IrArena, _match_id: IrNodeId, _idx: u32) {
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
}
