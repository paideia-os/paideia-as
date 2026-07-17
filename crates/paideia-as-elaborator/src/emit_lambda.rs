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
        self.emit_ret(ret_id, arena);
    }

    /// Emit bitwise-NOT lambda: `mov rax, rdi; not rax; ret` (7 bytes:
    /// `48 89 f8` / `48 f7 d0` / `c3`).
    ///
    /// Phase 7 m4-001: lowers `fn (x) -> ~x`. The operand arrives in RDI;
    /// we move it into RAX, complement it in place, and return.
    ///
    /// Three instructions keyed on `node*3 + {0,1,2}` to keep them adjacent
    /// and correctly ordered in the instruction map.
    pub(crate) fn emit_bitnot_lambda(&mut self, lambda_node_id: IrNodeId, arena: &IrArena) {
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
        self.emit_ret(ret_id, arena);
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
    pub(crate) fn emit_cast_lambda(&mut self, lambda_node_id: IrNodeId, arena: &IrArena) {
        self.emit_cast_lambda_with_shape(
            lambda_node_id,
            CastShape {
                src_width: 4,
                dst_width: 8,
                src_signed: true,
                dst_signed: true,
            },
            arena,
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
        arena: &IrArena,
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
        self.emit_ret(ret_id, arena);
    }

    /// Emit double lambda: `lea rax, [rdi + rdi]; ret` (5 bytes).
    pub(crate) fn emit_double_lambda(&mut self, lambda_node_id: IrNodeId, arena: &IrArena) {
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
        self.emit_ret(ret_id, arena);
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
        // CALLER currently holds them; only register-resident sources are
        // supported for phase-1 (a capture whose current binding is not a
        // scalar register — e.g. re-capturing an outer closure's own
        // env-slot capture — is silently skipped; legalizing that path is
        // deferred to the closure-call work in #995).
        for cap in &closure_meta.captures {
            if let Some(crate::local_binding_table::BindingHome::Reg(src_reg)) =
                self.state.local_bindings.get_home(&cap.name)
            {
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
