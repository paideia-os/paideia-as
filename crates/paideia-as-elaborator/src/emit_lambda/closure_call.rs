//! Closure invocation: [`EmitWalker::emit_closure_call`].
//!
//! Loads the fat pair `[env_ptr:8 | code_ptr:8]` referenced by a
//! closure-typed local binding, marshals up to 6 arguments through SysV
//! arg regs, and dispatches via `call [r11 + 8]` after saving/restoring
//! live scratch bindings (RCX / RDX / R8 / R9) around the call.

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
use paideia_as_ir::{IrArena, IrKind, IrNodeId, SmallVec, abi};

use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// #995: Emit closure-call sequence: `mov r11, f_reg; <scratch-save>; marshal args;
    /// mov r14, [r11+0]; call [r11+8]; <scratch-restore>`.
    ///
    /// Handles 0-6 argument calls to closures referenced via a closure-typed local binding.
    /// Emission sequence:
    /// - (1) `mov r11, <closure_reg>` — snapshot fat-pair address BEFORE arg marshalling
    /// - (2) Push live scratch bindings in {RCX, RDX, R8, R9} that would be clobbered
    /// - (3) Marshal args into RDI-R9 via SysV convention (same discipline as emit_indirect_call_via_reg)
    /// - (4) `mov r14, [r11 + 0]` — load env_ptr for closure body
    /// - (5) `call [r11 + 8]` — indirect call through code_ptr slot
    /// - (6) Pop scratch bindings (LIFO order)
    ///
    /// R14 preservation exemption: closure bodies do NOT need R14 restored after the call;
    /// the R14 value written by this call site is fresh for each invocation.
    pub(crate) fn emit_closure_call(
        &mut self,
        lambda_node_id: IrNodeId,
        closure_reg: RegId,
        arg_ids: &[IrNodeId],
        arena: &IrArena,
    ) {
        let first_id = self.alloc_synthetic_id();
        self.record_lambda_entry(lambda_node_id, first_id);

        let r11 = abi::R11;
        let r14 = abi::R14;
        let arg_regs = [abi::RDI, abi::RSI, abi::RDX, abi::RCX, abi::R8, abi::R9];

        // Step 1: Snapshot fat-pair address to R11 (BEFORE scratch-save, in case closure_reg aliases a scratch)
        let snapshot_id = first_id;
        let mut snapshot_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        snapshot_ops.push(Operand::Reg(r11));
        snapshot_ops.push(Operand::Reg(closure_reg));
        self.emit_inst(
            snapshot_id,
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: snapshot_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            },
        );

        // Step 2: Compute live scratch bindings to spill
        let live = self.state.local_bindings.live_regs();
        let caller_save_scratch = [abi::RCX, abi::RDX, abi::R8, abi::R9];
        let scratch_save_set: Vec<RegId> = caller_save_scratch.iter()
            .copied()
            .filter(|r| live.contains(r))
            .collect();

        // Emit push instructions for each live scratch register
        for &reg in &scratch_save_set {
            let scratch_save_id = self.alloc_synthetic_id();
            let mut push_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            push_ops.push(Operand::Reg(reg));
            self.emit_inst(
                scratch_save_id,
                Instruction {
                    mnemonic: Mnemonic::Push,
                    operands: push_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                },
            );
        }

        // Step 3: Marshal arguments into RDI-R9
        for (i, &arg_id) in arg_ids.iter().enumerate() {
            if i >= arg_regs.len() {
                // Too many arguments — silently skip for now (encoder will error)
                break;
            }

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

        // Step 4: Load env_ptr from [R11 + 0] into R14
        let env_load_id = self.alloc_synthetic_id();
        let mut env_load_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        env_load_ops.push(Operand::Reg(r14));
        env_load_ops.push(Operand::MemSib {
            base: r11,
            index: None,
            scale: paideia_as_ir::instruction::Scale::X1,
            disp: 0,
        });
        self.emit_inst(
            env_load_id,
            Instruction {
                mnemonic: Mnemonic::Mov,
                operands: env_load_ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            },
        );

        // Step 5: Call code_ptr at [R11 + 8]
        let call_id = self.alloc_synthetic_id();
        let mut call_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        call_ops.push(Operand::MemSib {
            base: r11,
            index: None,
            scale: paideia_as_ir::instruction::Scale::X1,
            disp: 8,
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

        // Step 6: Pop scratch bindings in LIFO order
        for &reg in scratch_save_set.iter().rev() {
            let scratch_restore_id = self.alloc_synthetic_id();
            let mut pop_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            pop_ops.push(Operand::Reg(reg));
            self.emit_inst(
                scratch_restore_id,
                Instruction {
                    mnemonic: Mnemonic::Pop,
                    operands: pop_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode: self.current_mode(),
                    emission_order: 0,
                },
            );
        }

        // RAX now holds the return value
    }
}
