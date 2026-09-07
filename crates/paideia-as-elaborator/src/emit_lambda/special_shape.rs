//! One-shot special-shape lambda emitters.
//!
//! Each of these lowers a syntactically recognized lambda body into a
//! minimal instruction sequence terminated by `ret`. All are dispatched
//! from the outer emit walker's per-lambda specialization pass.
//!
//! Contents:
//! - [`EmitWalker::emit_identity_lambda`] — `fn (x, ..) -> x`
//! - [`EmitWalker::emit_bitnot_lambda`]  — `fn (x) -> ~x`
//! - [`EmitWalker::emit_cast_lambda`]    — canonical `i32 as i64` shim
//! - [`EmitWalker::emit_cast_lambda_with_shape`] — arbitrary [`CastShape`]
//! - [`EmitWalker::emit_double_lambda`]  — `fn (x) -> x + x` via `lea`

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrNodeId, SmallVec, abi};

use crate::cast_shape::{CastShape, cast_plan};
use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// Emit identity lambda: `mov rax, <src_reg>; ret` (5 bytes).
    ///
    /// PA-r17-004: resolve the referenced parameter's register via
    /// binding_names (populated by cmd_build pre-pass) + local_bindings
    /// (populated by register_nested_lambda_params).
    ///
    /// v0.21-001 (#1277): fall back through `param_index_to_reg_for_abi`
    /// so MS x64 identity lambdas emit `mov rax, rcx` when name resolution
    /// misses (was: unconditional RDI fallback, which produced SysV bytes
    /// under `@abi("ms")`).
    pub(crate) fn emit_identity_lambda(
        &mut self,
        lambda_node_id: IrNodeId,
        body_id: IrNodeId,
        arena: &IrArena,
    ) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 2).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        let cc = self.state.lambda_abi(lambda_node_id.get());
        let home = arena
            .binding_names()
            .get(body_id)
            .and_then(|name| self.state.local_bindings.get_home(name));

        // v0.22.0 (#1326 phase 3): a bare-Var lambda body (e.g. `fn (a, ...,
        // g) -> g`) whose Var resolves to a StackSlot binding (SysV
        // stack-passed param, idx >= 6) cannot use the `mov rax, <reg>`
        // passthrough below — the value lives at [rbp + off], not in a
        // register. Emit a memory load instead. Falling through to the
        // register path's `param_index_to_reg_for_abi(cc, 0)` fallback
        // would be actively wrong here: it always resolves to the arg-0
        // register regardless of which param the body Var actually names.
        if let Some(crate::local_binding_table::BindingHome::StackSlot(off)) = home {
            let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            mov_operands.push(Operand::Reg(abi::RAX));
            mov_operands.push(Operand::MemSib {
                base: abi::RBP,
                index: None,
                scale: paideia_as_ir::Scale::X1,
                disp: off,
            });
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
            return;
        }

        let src_reg = match home {
            Some(crate::local_binding_table::BindingHome::Reg(r)) => r,
            _ => Self::param_index_to_reg_for_abi(cc, 0).unwrap_or(abi::RDI),
        };

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

    /// Emit bitwise-NOT lambda: `mov rax, <src_reg>; not rax; ret` (7 bytes).
    ///
    /// Phase 7 m4-001: lowers `fn (x) -> ~x`. The operand arrives in the
    /// ABI-selected arg-0 register (RDI for SysV, RCX for MS x64); we move
    /// it into RAX, complement it in place, and return.
    ///
    /// Three instructions keyed on `node*3 + {0,1,2}` to keep them adjacent
    /// and correctly ordered in the instruction map.
    ///
    /// v0.21-001 (#1277): ABI-aware source register selection.
    pub(crate) fn emit_bitnot_lambda(&mut self, lambda_node_id: IrNodeId, arena: &IrArena) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 3).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        let cc = self.state.lambda_abi(lambda_node_id.get());
        let src = Self::param_index_to_reg_for_abi(cc, 0).unwrap_or(abi::RDI);

        let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov_operands.push(Operand::Reg(abi::RAX));
        mov_operands.push(Operand::Reg(src));

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
        // v0.21-001 (#1277): source is arg-0 for the lambda's ABI
        // (RDI for SysV, RCX for MS x64).
        let cc = self.state.lambda_abi(lambda_node_id.get());
        let src = Self::param_index_to_reg_for_abi(cc, 0).unwrap_or(abi::RDI);

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

    /// Emit double lambda: `lea rax, [<src> + <src>]; ret` (5 bytes).
    ///
    /// v0.21-001 (#1277): ABI-aware source register (RDI for SysV, RCX for MS x64).
    pub(crate) fn emit_double_lambda(&mut self, lambda_node_id: IrNodeId, arena: &IrArena) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 2).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        let cc = self.state.lambda_abi(lambda_node_id.get());
        let src = Self::param_index_to_reg_for_abi(cc, 0).unwrap_or(abi::RDI);

        let mut lea_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        lea_operands.push(Operand::Reg(abi::RAX));
        lea_operands.push(Operand::MemSib {
            base: src,
            index: Some(src),
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
}
