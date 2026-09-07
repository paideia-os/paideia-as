//! Closure construction: [`EmitWalker::emit_closure_cons`].
//!
//! Materializes a fat pointer `[env_ptr:8 | code_ptr:8]` on the caller's
//! frame for a `ClosureCons` IR node. Handles every `BindingHome` variant
//! for the captured values (Reg, RegPair, EnvSlot, Closure, StackSlot).

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrNodeId, SmallVec, abi};

use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// #1233: Emit ClosureCons IR node — materialize fat pointer on caller's frame.
    ///
    /// Materializes a 16-byte fat pointer [env_ptr:8 | code_ptr:8] at the designated
    /// stack offset for this closure. Returns the fat pointer address in RAX for binding.
    ///
    /// For zero-capture closures: env_ptr = 0 (stored as imm64 0).
    /// For multi-capture closures: captures are precomputed on the frame by the caller.
    ///
    /// # Byte sequence
    ///
    /// Zero-capture:
    ///   mov qword [rsp + fat_off + 0], 0          (env_ptr = 0)
    ///   lea r11, [rip + closure_<mangled>]        (code_ptr via reloc)
    ///   mov [rsp + fat_off + 8], r11
    ///   lea rax, [rsp + fat_off]                  (return fat-pointer address)
    ///
    /// Multi-capture:
    ///   lea r11, [rsp + env_off]                  (env_ptr = &captures[0])
    ///   mov [rsp + fat_off + 0], r11
    ///   lea r11, [rip + closure_<mangled>]        (code_ptr via reloc)
    ///   mov [rsp + fat_off + 8], r11
    ///   lea rax, [rsp + fat_off]                  (return fat-pointer address)
    pub(crate) fn emit_closure_cons(&mut self, cc_id: IrNodeId, arena: &IrArena) {
        // Look up closure body Lambda and its metadata
        let cc_children = arena.children(cc_id);
        let lambda_id = match cc_children.first().copied() {
            Some(id) => id,
            None => return,
        };

        let closure_meta = match arena.closure_meta().get(lambda_id) {
            Some(meta) => meta,
            None => return,
        };

        // Look up frame layout for current function
        let current_lambda_id = match IrNodeId::new(self.state.current_function) {
            Some(id) => id,
            None => return,
        };

        let frame_layout = match arena.closure_frame_meta().get(current_lambda_id) {
            Some(layout) => layout,
            None => return,
        };

        let (fat_off, env_off) = match frame_layout.get_slot(cc_id) {
            Some((f, e)) => (f as i32, e as i32),
            None => return,
        };

        // Determine if this is a zero-capture closure
        let has_captures = !closure_meta.captures.is_empty();

        // Issue #994: write each captured value into its env slot BEFORE
        // constructing the env/fat pointer, so a subsequent read through the
        // fat pointer (env_ptr[k]) observes the value captured at
        // ClosureCons-construction time. Captures are read from wherever the
        // CALLER currently holds them.
        //
        // Issue #1238: Handle all BindingHome variants:
        // - Reg(r): mov [rsp + env_off + cap.offset], r
        // - RegPair(lo, hi): emit two writes for discriminant and payload
        // - EnvSlot(off): load from outer env [r14 + off] into r11, then store
        // - Closure(r): fat pointer captured, treat as single pointer register
        for cap in &closure_meta.captures {
            if let Some(binding_home) =
                self.state.local_bindings.get_home(&cap.name)
            {
                use crate::local_binding_table::BindingHome;
                match binding_home {
                    BindingHome::Reg(src_reg) => {
                        // Scalar register: mov [rsp + env_off + cap.offset], src_reg
                        let store_id = self.alloc_synthetic_id();
                        let mut store_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        store_ops.push(Operand::MemSib {
                            base: abi::RSP,
                            index: None,
                            scale: paideia_as_ir::Scale::X1,
                            disp: env_off + cap.offset,
                        });
                        store_ops.push(Operand::Reg(src_reg));
                        self.emit_inst(
                            store_id,
                            Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: store_ops,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
                            },
                        );
                    }
                    BindingHome::RegPair(lo_reg, hi_reg) => {
                        // Enum pair: emit two 8-byte writes for discriminant and payload
                        // mov [rsp + env_off + cap.offset], lo_reg
                        let store_lo_id = self.alloc_synthetic_id();
                        let mut store_lo_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        store_lo_ops.push(Operand::MemSib {
                            base: abi::RSP,
                            index: None,
                            scale: paideia_as_ir::Scale::X1,
                            disp: env_off + cap.offset,
                        });
                        store_lo_ops.push(Operand::Reg(lo_reg));
                        self.emit_inst(
                            store_lo_id,
                            Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: store_lo_ops,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
                            },
                        );

                        // mov [rsp + env_off + cap.offset + 8], hi_reg
                        let store_hi_id = self.alloc_synthetic_id();
                        let mut store_hi_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        store_hi_ops.push(Operand::MemSib {
                            base: abi::RSP,
                            index: None,
                            scale: paideia_as_ir::Scale::X1,
                            disp: env_off + cap.offset + 8,
                        });
                        store_hi_ops.push(Operand::Reg(hi_reg));
                        self.emit_inst(
                            store_hi_id,
                            Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: store_hi_ops,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
                            },
                        );
                    }
                    BindingHome::EnvSlot(outer_off) => {
                        // Nested closure re-capturing an outer capture.
                        // First load from outer env [r14 + outer_off] into r11
                        let load_id = self.alloc_synthetic_id();
                        let mut load_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        load_ops.push(Operand::Reg(abi::R11));
                        load_ops.push(Operand::MemSib {
                            base: abi::R14,
                            index: None,
                            scale: paideia_as_ir::Scale::X1,
                            disp: outer_off,
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

                        // Then store r11 to env slot [rsp + env_off + cap.offset]
                        let store_id = self.alloc_synthetic_id();
                        let mut store_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        store_ops.push(Operand::MemSib {
                            base: abi::RSP,
                            index: None,
                            scale: paideia_as_ir::Scale::X1,
                            disp: env_off + cap.offset,
                        });
                        store_ops.push(Operand::Reg(abi::R11));
                        self.emit_inst(
                            store_id,
                            Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: store_ops,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
                            },
                        );
                    }
                    BindingHome::Closure(closure_reg) => {
                        // Closure captured as fat pointer (pointer-sized).
                        // Treat like scalar register: mov [rsp + env_off + cap.offset], closure_reg
                        let store_id = self.alloc_synthetic_id();
                        let mut store_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        store_ops.push(Operand::MemSib {
                            base: abi::RSP,
                            index: None,
                            scale: paideia_as_ir::Scale::X1,
                            disp: env_off + cap.offset,
                        });
                        store_ops.push(Operand::Reg(closure_reg));
                        self.emit_inst(
                            store_id,
                            Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: store_ops,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
                            },
                        );
                    }
                    BindingHome::StackSlot(rbp_off) => {
                        // v0.22.0 (#1326 phase 3): capturing a SysV
                        // stack-passed param (idx >= 6). Mirrors the
                        // EnvSlot arm above but sources from [rbp +
                        // rbp_off] instead of [r14 + outer_off].
                        let load_id = self.alloc_synthetic_id();
                        let mut load_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        load_ops.push(Operand::Reg(abi::R11));
                        load_ops.push(Operand::MemSib {
                            base: abi::RBP,
                            index: None,
                            scale: paideia_as_ir::Scale::X1,
                            disp: rbp_off,
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

                        let store_id = self.alloc_synthetic_id();
                        let mut store_ops: SmallVec<[Operand; 3]> = SmallVec::new();
                        store_ops.push(Operand::MemSib {
                            base: abi::RSP,
                            index: None,
                            scale: paideia_as_ir::Scale::X1,
                            disp: env_off + cap.offset,
                        });
                        store_ops.push(Operand::Reg(abi::R11));
                        self.emit_inst(
                            store_id,
                            Instruction {
                                mnemonic: Mnemonic::Mov,
                                operands: store_ops,
                                encoding_hint: None,
                                byte_offset_in_text: None,
                                mode: self.current_mode(),
                                emission_order: 0,
                            },
                        );
                    }
                }
            }
        }

        if has_captures {
            // Multi-capture: lea r11, [rsp + env_off]
            let env_lea_id = self.alloc_synthetic_id();
            let mut env_lea_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            env_lea_ops.push(Operand::Reg(abi::R11));
            env_lea_ops.push(Operand::MemSib {
                base: abi::RSP,
                index: None,
                scale: paideia_as_ir::Scale::X1,
                disp: env_off,
            });
            self.emit_inst(
                env_lea_id,
                Instruction {
                    mnemonic: Mnemonic::Lea,
                    operands: env_lea_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                },
            );

            // mov [rsp + fat_off + 0], r11
            let env_mov_id = self.alloc_synthetic_id();
            let mut env_mov_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            env_mov_ops.push(Operand::MemSib {
                base: abi::RSP,
                index: None,
                scale: paideia_as_ir::Scale::X1,
                disp: fat_off,
            });
            env_mov_ops.push(Operand::Reg(abi::R11));
            self.emit_inst(
                env_mov_id,
                Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: env_mov_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                },
            );
        } else {
            // Zero-capture: mov qword [rsp + fat_off + 0], 0
            let zero_mov_id = self.alloc_synthetic_id();
            let mut zero_mov_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            zero_mov_ops.push(Operand::MemSib {
                base: abi::RSP,
                index: None,
                scale: paideia_as_ir::Scale::X1,
                disp: fat_off,
            });
            zero_mov_ops.push(Operand::Imm64(0));
            self.emit_inst(
                zero_mov_id,
                Instruction {
                    // Issue #994: plain Mnemonic::Mov has no [MemSib, Imm64] store
                    // form in the encoder (encode_mov only covers [MemSib, Reg] /
                    // [Reg, MemSib]); the immediate-to-memory store lives in
                    // encode_mov_sized, reachable only via MovSized. Without this,
                    // every zero-capture closure fails to build with B1705.
                    mnemonic: Mnemonic::MovSized { width: paideia_as_ir::instruction::IntWidth::W64 },
                    operands: zero_mov_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                },
            );
        }

        // lea r11, [rip + closure_<mangled>]
        let code_lea_id = self.alloc_synthetic_id();
        let mut code_lea_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        code_lea_ops.push(Operand::Reg(abi::R11));
        code_lea_ops.push(Operand::MemRipRelSym {
            name: closure_meta.mangled_name.clone(),
            addend: 0,
        });
        self.emit_inst(
            code_lea_id,
            Instruction {
                mnemonic: Mnemonic::Lea,
                operands: code_lea_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            },
        );

        // mov [rsp + fat_off + 8], r11
        let code_mov_id = self.alloc_synthetic_id();
        let mut code_mov_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        code_mov_ops.push(Operand::MemSib {
            base: abi::RSP,
            index: None,
            scale: paideia_as_ir::Scale::X1,
            disp: fat_off + 8,
        });
        code_mov_ops.push(Operand::Reg(abi::R11));
        self.emit_inst(
            code_mov_id,
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: code_mov_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            },
        );

        // lea rax, [rsp + fat_off]
        let ret_lea_id = self.alloc_synthetic_id();
        let mut ret_lea_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        ret_lea_ops.push(Operand::Reg(abi::RAX));
        ret_lea_ops.push(Operand::MemSib {
            base: abi::RSP,
            index: None,
            scale: paideia_as_ir::Scale::X1,
            disp: fat_off,
        });
        self.emit_inst(
            ret_lea_id,
            Instruction {
                mnemonic: Mnemonic::Lea,
                operands: ret_lea_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            },
        );
    }
}
