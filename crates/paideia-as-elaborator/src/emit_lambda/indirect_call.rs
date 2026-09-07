//! Indirect-call sequences: via register, via RIP-relative symbol, and via
//! `[base + disp]`.
//!
//! Shared discipline (issue #1099 unified ID scheme):
//! - `1_040_000 + L*100`: fnptr save/load (only for reg / base+disp forms)
//! - `1_060_000 + L*100 + seq`: arg moves (disjoint from direct-call MOVs at 1_000_000)
//! - `1_070_000 + L*100`: call
//! - `1_170_000 + L*100`: ret
//!
//! Contents:
//! - [`EmitWalker::emit_indirect_call_via_reg`]         — save callee to R11 first
//! - [`EmitWalker::emit_indirect_call_via_mem_rip_sym`] — single `call [rip+sym]`
//! - [`EmitWalker::emit_indirect_call_via_mem_base_disp`] — load through record field

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi};

use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// PA-r17-004: Emit indirect call via a register holding a function
    /// pointer.
    ///
    /// Handles 0-6 argument calls to functions referenced via a register.
    /// Structure:
    /// - (1) `mov r11, <callee_reg>` — save fnptr BEFORE arg marshalling
    /// - (2) `mov <arg_reg>, <arg_src>` per argument
    /// - (3) `call r11`
    /// - (4) `ret`
    ///
    /// Instruction ordering via unified ID scheme (issue #1099):
    /// - `1_040_000 + L*100`: save (mov r11, callee)
    /// - `1_060_000 + L*100 + seq`: arg moves (disjoint from direct-call MOVs at 1_000_000)
    /// - `1_070_000 + L*100`: call r11
    /// - `1_170_000 + L*100`: ret
    pub(crate) fn emit_indirect_call_via_reg(
        &mut self,
        lambda_node_id: IrNodeId,
        callee_reg: RegId,
        arg_ids: &[IrNodeId],
        arena: &IrArena,
    ) {
        let first_id = self.alloc_synthetic_id();
        self.record_lambda_entry(lambda_node_id, first_id);

        let r11 = abi::R11;
        let arg_regs = [abi::RDI, abi::RSI, abi::RDX, abi::RCX, abi::R8, abi::R9];

        let save_id = first_id;
        let mut save_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        save_ops.push(Operand::Reg(r11));
        save_ops.push(Operand::Reg(callee_reg));
        self.emit_inst(
            save_id,
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: save_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
},
        );

        for (i, &arg_id) in arg_ids.iter().enumerate() {
            let dst = arg_regs[i];
            let arg_node = match arena.get(arg_id) {
                Some(n) => n,
                None => continue,
            };
            match arg_node.kind {
                IrKind::Literal => {
                    if let Some(v) = arena.literal_values().get(arg_id) {
                        self.emit_mov_literal_to_reg(dst, v);
                    }
                }
                IrKind::Var => {
                    if let Some(name) = arena.binding_names().get(arg_id) {
                        let iid = self.alloc_synthetic_id();
                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        ops.push(Operand::Reg(dst));
                        ops.push(Operand::Var { name: name.to_string() });
                        self.emit_inst(
                            iid,
                            Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: ops,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
},
                        );
                    }
                }
                _ => { /* Not handled in #982 */ }
            }
        }

        let call_id = self.alloc_synthetic_id();
        let mut call_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        call_ops.push(Operand::Reg(r11));
        self.emit_inst(
            call_id,
            Instruction {
                mnemonic: Mnemonic::Call,
                operands: call_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
},
        );

        let ret_id = self.alloc_synthetic_id();
        self.emit_ret(ret_id, arena);
    }

    /// Emit indirect call via RIP-relative symbol: single `call [rip + sym + addend]` instruction.
    ///
    /// PA-R17-015: Optimized path for calling function pointers stored at module-level symbols.
    /// Emits a single direct RIP-relative memory call (FF 15) instead of:
    ///   - `mov r11, [rip + sym + addend]` (mov reg64 from memory)
    ///   - `call r11` (call via register)
    ///
    /// Marshals arguments into RDI/RSI/RDX/RCX/R8/R9 via SysV ABI, then emits:
    ///   1. Argument loads (mov rdi, arg0; mov rsi, arg1; ...)
    ///   2. Single `call [rip + sym + addend]`
    ///   3. `ret`
    ///
    /// Issue #1099: Uses unified ID scheme to ensure arg MOVs sort before CALL and RET is last:
    /// - `1_060_000 + L*100 + seq`: arg moves
    /// - `1_070_000 + L*100`: call [rip + sym]
    /// - `1_170_000 + L*100`: ret
    pub(crate) fn emit_indirect_call_via_mem_rip_sym(
        &mut self,
        lambda_node_id: IrNodeId,
        callee_name: String,
        callee_addend: i32,
        arg_ids: &[IrNodeId],
        arena: &IrArena,
    ) {
        let first_id = self.alloc_synthetic_id();
        self.record_lambda_entry(lambda_node_id, first_id);
        let mut first_id_opt = Some(first_id);

        let arg_regs = [abi::RDI, abi::RSI, abi::RDX, abi::RCX, abi::R8, abi::R9];

        for (i, &arg_id) in arg_ids.iter().enumerate() {
            let dst = arg_regs[i];
            let arg_node = match arena.get(arg_id) {
                Some(n) => n,
                None => continue,
            };
            match arg_node.kind {
                IrKind::Literal => {
                    if let Some(v) = arena.literal_values().get(arg_id) {
                        let iid = first_id_opt.take().unwrap_or_else(|| self.alloc_synthetic_id());
                        self.emit_mov_literal_to_reg_with_id(iid, dst, v);
                    }
                }
                IrKind::Var => {
                    if let Some(name) = arena.binding_names().get(arg_id) {
                        let iid = first_id_opt.take().unwrap_or_else(|| self.alloc_synthetic_id());
                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        ops.push(Operand::Reg(dst));
                        ops.push(Operand::Var { name: name.to_string() });
                        self.emit_inst(
                            iid,
                            Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: ops,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
},
                        );
                    }
                }
                _ => { /* Not handled in #982 */ }
            }
        }

        let call_id = first_id_opt.take().unwrap_or_else(|| self.alloc_synthetic_id());
        let mut call_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        call_ops.push(Operand::MemRipRelSym {
            name: callee_name,
            addend: callee_addend,
        });
        self.emit_inst(
            call_id,
            Instruction {
                mnemonic: Mnemonic::Call,
                operands: call_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
},
        );

        let ret_id = first_id_opt.take().unwrap_or_else(|| self.alloc_synthetic_id());
        self.emit_ret(ret_id, arena);
    }

    /// PA-r17-004b: Emit indirect call via a memory location addressed by base + disp.
    ///
    /// Handles 0-6 argument calls to functions referenced via a field in a local-bound record.
    /// Emission order (CRITICAL — base_reg aliases arg regs):
    /// - (1) `mov r11, [base_reg + field_offset]` — load fnptr FIRST while base is still live
    /// - (2) `mov <arg_reg>, <arg_src>` per argument
    /// - (3) `call r11`
    /// - (4) `ret`
    ///
    /// Issue #1099: Uses unified ID scheme to ensure fnptr load precedes arg MOVs,
    /// arg MOVs precede CALL, and RET is last:
    /// - `1_040_000 + L*100`: load fnptr (mov r11, [base + disp])
    /// - `1_060_000 + L*100 + seq`: arg moves
    /// - `1_070_000 + L*100`: call r11
    /// - `1_170_000 + L*100`: ret
    pub(crate) fn emit_indirect_call_via_mem_base_disp(
        &mut self,
        lambda_node_id: IrNodeId,
        base_reg: RegId,
        field_offset: i32,
        arg_ids: &[IrNodeId],
        arena: &IrArena,
    ) {
        let first_id = self.alloc_synthetic_id();
        self.record_lambda_entry(lambda_node_id, first_id);

        let r11 = abi::R11;
        let arg_regs = [abi::RDI, abi::RSI, abi::RDX, abi::RCX, abi::R8, abi::R9];

        // Step 1: Load fnptr from [base_reg + field_offset] into R11
        let load_id = first_id;
        let mut load_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        load_ops.push(Operand::Reg(r11));
        load_ops.push(Operand::MemSib {
            base: base_reg,
            index: None,
            scale: paideia_as_ir::instruction::Scale::X1,
            disp: field_offset,
        });
        self.emit_inst(
            load_id,
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: load_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
},
        );

        // Step 2: Marshal arguments into arg_regs
        for (i, &arg_id) in arg_ids.iter().enumerate() {
            let dst = arg_regs[i];
            let arg_node = match arena.get(arg_id) {
                Some(n) => n,
                None => continue,
            };
            match arg_node.kind {
                IrKind::Literal => {
                    if let Some(v) = arena.literal_values().get(arg_id) {
                        self.emit_mov_literal_to_reg(dst, v);
                    }
                }
                IrKind::Var => {
                    if let Some(name) = arena.binding_names().get(arg_id) {
                        let iid = self.alloc_synthetic_id();
                        let mut ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        ops.push(Operand::Reg(dst));
                        ops.push(Operand::Var { name: name.to_string() });
                        self.emit_inst(
                            iid,
                            Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: ops,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
},
                        );
                    }
                }
                _ => { /* Not handled yet */ }
            }
        }

        // Step 3: Call R11
        let call_id = self.alloc_synthetic_id();
        let mut call_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        call_ops.push(Operand::Reg(r11));
        self.emit_inst(
            call_id,
            Instruction {
                mnemonic: Mnemonic::Call,
                operands: call_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
},
        );

        // Step 4: Return
        let ret_id = self.alloc_synthetic_id();
        self.emit_ret(ret_id, arena);
    }
}
