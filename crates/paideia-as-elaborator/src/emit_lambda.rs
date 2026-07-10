//! Special-shape lambda emitters and the indirect-call sequence.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Hosts the
//! per-shape lambda lowerings (`identity`, `bitnot`, `cast`, `double`) plus
//! the indirect-call marshalling used by `PA-r17-004`.
//!
//! All functions are `impl EmitWalker` methods and share the walker's
//! internal state (`emit_inst`, `record_lambda_entry`, `current_mode`,
//! `state.local_bindings`, `emit_mov_literal_to_reg`) via `pub(crate)`
//! visibility set on `EmitWalker`.

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi};

use crate::cast_shape::{CastShape, cast_plan};
use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// Emit identity lambda: `mov rax, <src_reg>; ret` (5 bytes).
    ///
    /// PA-r17-004: resolve the referenced parameter's register via
    /// binding_names (populated by cmd_build pre-pass) + local_bindings
    /// (populated by register_nested_lambda_params). Fall back to RDI
    /// when the name is not resolvable (single-param convention +
    /// in-crate unit tests that skip the cmd_build pre-pass).
    pub(crate) fn emit_identity_lambda(
        &mut self,
        lambda_node_id: IrNodeId,
        body_id: IrNodeId,
        arena: &IrArena,
    ) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 2).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        let src_reg = arena
            .binding_names()
            .get(body_id)
            .and_then(|name| self.state.local_bindings.get(name))
            .unwrap_or(abi::RDI);

        let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov_operands.push(Operand::Reg(abi::RAX));
        mov_operands.push(Operand::Reg(src_reg));

        let mov_inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: mov_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        self.emit_inst(main_id, mov_inst);

        let ret_id = IrNodeId::new(lambda_node_id.get() * 2 + 1).expect("ret virtual id");
        let ret_inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };
        self.emit_inst(ret_id, ret_inst);
    }

    /// Emit bitwise-NOT lambda: `mov rax, rdi; not rax; ret` (7 bytes:
    /// `48 89 f8` / `48 f7 d0` / `c3`).
    ///
    /// Phase 7 m4-001: lowers `fn (x) -> ~x`. The operand arrives in RDI;
    /// we move it into RAX, complement it in place, and return.
    ///
    /// Three instructions keyed on `node*3 + {0,1,2}` to keep them adjacent
    /// and correctly ordered in the instruction map.
    pub(crate) fn emit_bitnot_lambda(&mut self, lambda_node_id: IrNodeId) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 3).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov_operands.push(Operand::Reg(abi::RAX));
        mov_operands.push(Operand::Reg(abi::RDI));

        let mov_inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: mov_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        self.emit_inst(main_id, mov_inst);

        let mut not_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        not_operands.push(Operand::Reg(abi::RAX));

        let not_inst = Instruction {
            mnemonic: Mnemonic::Not,
            operands: not_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        let not_id = IrNodeId::new(lambda_node_id.get() * 3 + 1).expect("not instr virtual id");
        self.emit_inst(not_id, not_inst);

        let ret_id = IrNodeId::new(lambda_node_id.get() * 3 + 2).expect("ret virtual id");
        let ret_inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };
        self.emit_inst(ret_id, ret_inst);
    }

    /// Emit cast lambda: a single width-conversion instruction then `ret`.
    ///
    /// Phase 7 m4-002 / PA8 m3-002 (#826). Lowers `fn (x) -> x as TYPE`. The
    /// operand arrives in RDI; the result is produced in RAX, then the
    /// function returns. Conversion instruction is chosen by [`cast_plan`].
    ///
    /// IR-pipeline callers do not yet resolve the `CastSideTable` `TypeId`
    /// to a concrete `(width, signedness)`; the structural-cast call site
    /// therefore passes the canonical `i32 as i64` shape.
    pub(crate) fn emit_cast_lambda(&mut self, lambda_node_id: IrNodeId) {
        self.emit_cast_lambda_with_shape(
            lambda_node_id,
            CastShape {
                src_width: 4,
                dst_width: 8,
                src_signed: true,
                dst_signed: true,
            },
        );
    }

    /// Emit a cast lambda for an explicit [`CastShape`], dispatching on width
    /// and signedness via [`cast_plan`].
    ///
    /// RAX is the destination, RDI the incoming argument. A `CastPlan::Nop`
    /// shape emits no conversion instruction — only the trailing `ret`.
    pub(crate) fn emit_cast_lambda_with_shape(
        &mut self,
        lambda_node_id: IrNodeId,
        shape: CastShape,
    ) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 2).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        let dst = abi::RAX;
        let src = abi::RDI;

        let plan = cast_plan(shape);
        if let Some((mnemonic, hint, _size)) = plan.instruction() {
            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
            operands.push(Operand::Reg(dst));
            operands.push(Operand::Reg(src));
            let inst = Instruction {
                mnemonic,
                operands,
                encoding_hint: hint,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                        emission_order: 0,
        };
            self.emit_inst(main_id, inst);
        }

        let ret_id = IrNodeId::new(lambda_node_id.get() * 2 + 1).expect("ret virtual id");
        let ret_inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };
        self.emit_inst(ret_id, ret_inst);
    }

    /// Emit double lambda: `lea rax, [rdi + rdi]; ret` (5 bytes).
    pub(crate) fn emit_double_lambda(&mut self, lambda_node_id: IrNodeId) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 2).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        let mut lea_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        lea_operands.push(Operand::Reg(abi::RAX));
        lea_operands.push(Operand::MemSib {
            base: abi::RDI,
            index: Some(abi::RDI),
            scale: paideia_as_ir::instruction::Scale::X1,
            disp: 0,
        });

        let lea_inst = Instruction {
            mnemonic: Mnemonic::Lea,
            operands: lea_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        self.emit_inst(main_id, lea_inst);

        let ret_id = IrNodeId::new(lambda_node_id.get() * 2 + 1).expect("ret virtual id");
        let ret_inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };
        self.emit_inst(ret_id, ret_inst);
    }

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
        let l = lambda_node_id.get();
        let r11 = abi::R11;
        let arg_regs = [abi::RDI, abi::RSI, abi::RDX, abi::RCX, abi::R8, abi::R9];

        let save_id = IrNodeId::new(1_040_000u32.saturating_add(l.saturating_mul(100)))
            .unwrap_or_else(|| IrNodeId::new(1).unwrap());
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

        let mut seq_id = 0u32;
        for (i, &arg_id) in arg_ids.iter().enumerate() {
            let dst = arg_regs[i];
            let arg_node = match arena.get(arg_id) {
                Some(n) => n,
                None => continue,
            };
            match arg_node.kind {
                IrKind::Literal => {
                    if let Some(v) = arena.literal_values().get(arg_id) {
                        self.emit_mov_literal_to_reg(lambda_node_id, dst, v);
                    }
                }
                IrKind::Var => {
                    if let Some(name) = arena.binding_names().get(arg_id) {
                        let iid = IrNodeId::new(1_060_000u32
                            .saturating_add(l.saturating_mul(100))
                            .saturating_add(seq_id))
                            .expect("arg instr virtual id");
                        seq_id += 1;
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

        let call_id = IrNodeId::new(1_070_000u32.saturating_add(l.saturating_mul(100)))
            .unwrap_or_else(|| IrNodeId::new(1).unwrap());
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

        let ret_id = IrNodeId::new(1_170_000u32.saturating_add(l.saturating_mul(100)))
            .unwrap_or_else(|| IrNodeId::new(1).unwrap());
        self.emit_inst(
            ret_id,
            Instruction {
                mnemonic: Mnemonic::Ret,
                operands: SmallVec::new(),
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
},
        );
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
        let l = lambda_node_id.get();
        let arg_regs = [abi::RDI, abi::RSI, abi::RDX, abi::RCX, abi::R8, abi::R9];

        let mut seq_id = 0u32;
        for (i, &arg_id) in arg_ids.iter().enumerate() {
            let dst = arg_regs[i];
            let arg_node = match arena.get(arg_id) {
                Some(n) => n,
                None => continue,
            };
            match arg_node.kind {
                IrKind::Literal => {
                    if let Some(v) = arena.literal_values().get(arg_id) {
                        self.emit_mov_literal_to_reg(lambda_node_id, dst, v);
                    }
                }
                IrKind::Var => {
                    if let Some(name) = arena.binding_names().get(arg_id) {
                        let iid = IrNodeId::new(1_060_000u32
                            .saturating_add(l.saturating_mul(100))
                            .saturating_add(seq_id))
                            .expect("arg instr virtual id");
                        seq_id += 1;
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

        let call_id = IrNodeId::new(1_070_000u32.saturating_add(l.saturating_mul(100)))
            .unwrap_or_else(|| IrNodeId::new(1).unwrap());
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

        let ret_id = IrNodeId::new(1_170_000u32.saturating_add(l.saturating_mul(100)))
            .unwrap_or_else(|| IrNodeId::new(1).unwrap());
        self.emit_inst(
            ret_id,
            Instruction {
                mnemonic: Mnemonic::Ret,
                operands: SmallVec::new(),
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
},
        );
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
        let l = lambda_node_id.get();
        let r11 = abi::R11;
        let arg_regs = [abi::RDI, abi::RSI, abi::RDX, abi::RCX, abi::R8, abi::R9];

        // Step 1: Load fnptr from [base_reg + field_offset] into R11
        let load_id = IrNodeId::new(1_040_000u32.saturating_add(l.saturating_mul(100)))
            .unwrap_or_else(|| IrNodeId::new(1).unwrap());
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
        let mut seq_id = 0u32;
        for (i, &arg_id) in arg_ids.iter().enumerate() {
            let dst = arg_regs[i];
            let arg_node = match arena.get(arg_id) {
                Some(n) => n,
                None => continue,
            };
            match arg_node.kind {
                IrKind::Literal => {
                    if let Some(v) = arena.literal_values().get(arg_id) {
                        self.emit_mov_literal_to_reg(lambda_node_id, dst, v);
                    }
                }
                IrKind::Var => {
                    if let Some(name) = arena.binding_names().get(arg_id) {
                        let iid = IrNodeId::new(1_060_000u32
                            .saturating_add(l.saturating_mul(100))
                            .saturating_add(seq_id))
                            .expect("arg instr virtual id");
                        seq_id += 1;
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
        let call_id = IrNodeId::new(1_070_000u32.saturating_add(l.saturating_mul(100)))
            .unwrap_or_else(|| IrNodeId::new(1).unwrap());
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
        let ret_id = IrNodeId::new(1_170_000u32.saturating_add(l.saturating_mul(100)))
            .unwrap_or_else(|| IrNodeId::new(1).unwrap());
        self.emit_inst(
            ret_id,
            Instruction {
                mnemonic: Mnemonic::Ret,
                operands: SmallVec::new(),
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
},
        );
    }
}
