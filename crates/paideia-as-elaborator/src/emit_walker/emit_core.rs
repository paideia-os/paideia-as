//! EmitWalker — core instruction-emit primitives.
//!
//! `emit_inst` is the single canonical way any EmitWalker path writes an
//! `Instruction` into `state.instructions`. It handles:
//!   * lazy SysV frame-pointer prologue injection (paideia-as#1276 phase 3),
//!   * emission_order stamping,
//!   * `pending_first_instr_lambda` consumption (PA8-m1-002c / #994),
//!   * `estimated_offset` bookkeeping via the real encoder's
//!     `estimated_bytes` (#985, #986).
//!
//! `emit_ret` composes the closure-frame teardown, the SysV frame-pointer
//! epilogue (unless suppressed), and the trailing `ret` — with the ISR
//! variant redirecting through `emit_interrupt_epilogues` (see sibling
//! `emit_interrupt.rs`) rather than emitting `ret` at all.
//!
//! `alloc_synthetic_id` and `current_mode` are shared plumbing used by
//! every emit path in the walker family.
//!
//! Split from `emit_walker.rs` (paideia-as#1411).

use paideia_as_ir::instruction::{InstrMode, Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrNodeId, SmallVec, abi};

use super::EmitWalker;

impl EmitWalker {
    /// Insert an `Instruction` into the side-table and advance
    /// `estimated_offset` by exactly the number of bytes the encoder
    /// will emit for it.
    ///
    /// This is the single canonical way to emit an instruction from
    /// `EmitWalker`. Retires ~65 scattered `state.instructions.insert(...);
    /// state.estimated_offset += <literal>;` pairs whose size literals had
    /// drifted from encoder reality on multiple occasions (#985, #986).
    ///
    /// The size is computed by calling the real encoder into a throwaway
    /// buffer via `paideia_as_encoder::estimated_bytes`. If the encoder
    /// cannot handle the instruction, size is 0 — callers must ensure
    /// their instructions actually encode.
    pub(crate) fn emit_inst(&mut self, node_id: IrNodeId, mut inst: Instruction) {
        // paideia-as#1276 phase 3: lazy frame-prologue injection.
        // If `visit_lambda` armed `pending_frame_prologue` for the current
        // function, inject `push rbp; mov rbp, rsp` BEFORE this instruction
        // — that way (a) the prologue sorts first in this function's
        // .text bytes, (b) its `push rbp` claims `lambda_first_instr[L]`
        // so the ELF symbol starts AT the prologue (not after), and (c)
        // lambdas whose body-shape arm never calls `emit_inst` produce
        // zero bytes and continue to flag B1704, preserving the pre-#1276
        // diagnostic contract.
        //
        // Take the arm BEFORE the recursive calls so those calls don't
        // re-enter this branch and blow the stack.
        if self.state.current_function != 0
            && self.state.take_pending_frame_prologue(self.state.current_function)
        {
            self.state.mark_frame_prologue_emitted(self.state.current_function);
            let fn_id = self.state.current_function;

            // Emit: push rbp
            let mut push_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            push_operands.push(Operand::Reg(abi::RBP));
            let push_inst = Instruction {
                mnemonic: Mnemonic::Push,
                operands: push_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let push_id = self.alloc_synthetic_id();
            self.emit_inst(push_id, push_inst);

            // Emit: mov rbp, rsp
            let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            mov_operands.push(Operand::Reg(abi::RBP));
            mov_operands.push(Operand::Reg(abi::RSP));
            let mov_inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: mov_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let mov_id = self.alloc_synthetic_id();
            self.emit_inst(mov_id, mov_inst);

            // Claim `lambda_first_instr[fn_id]` for `push rbp` — the ELF
            // symbol for this function starts at the frame prologue, not at
            // the body's first instruction. `record_lambda_entry` on the
            // body-shape arms (Var/BitNot/Cast/Literal) uses `.or_insert`
            // and may have already recorded a body-instruction id; the
            // App/Action/Match/EnumCons/Store arms arm
            // `pending_first_instr_lambda` and the recursive `emit_inst` for
            // `push rbp` above would have consumed it. Either way, force the
            // entry to `push_id` so the resulting ELF symbol's address
            // matches the first executed byte of the function. Anything else
            // means control jumps INTO the function past `push rbp`,
            // leaving `pop rbp` in the epilogue to pull off the caller's
            // frame instead.
            self.state.lambda_first_instr.insert(fn_id, push_id);
        }

        let bytes = paideia_as_encoder::estimated_bytes(&inst);
        inst.emission_order = self.state.next_emission_order;
        self.state.next_emission_order += 1;
        // #1139: Record which lambda owns this instruction.
        self.state.instr_to_lambda.insert(node_id, self.state.current_function);
        self.state.instructions.insert(node_id, inst);
        // PA8-m1-002c: Capture first instruction of pending lambda before offset advances.
        // NOTE (#994): this is an unconditional overwrite, not entry/or_insert — at
        // least one existing body-shape path (Match, exercised by
        // caller_app_pair_arg_value) relies on a *later* arm+consume cycle for the
        // same lambda id winning over an earlier one. Closures avoid needing a
        // competing semantics here: the closure-frame prologue (visit_lambda,
        // emit_visit_lambda.rs) guards each body-shape arm's own
        // `pending_first_instr_lambda` (re-)arming so it only fires when this
        // lambda has no `lambda_first_instr` entry yet, letting the prologue's
        // claim (the function's true entry point) stick without changing this
        // shared, order-sensitive mechanism's semantics for every other caller.
        if let Some(lid) = self.state.pending_first_instr_lambda.take() {
            self.state.lambda_first_instr.insert(lid, node_id);
            self.state.mark_lambda_emitted(lid);
        }
        self.state.estimated_offset += bytes;
    }

    /// #1141: Allocate a monotonic synthetic IrNodeId for instructions that
    /// have no natural AST-derived id (bridge saves, CALL sites, indirect-call
    /// scaffolds). These are identity-only post-#1140 — `.text` order is
    /// governed by emission_order, so this counter just needs to hand out
    /// unique ids that don't collide with arena ids or each other.
    pub(crate) fn alloc_synthetic_id(&mut self) -> IrNodeId {
        let id = self.state.next_synthetic_id;
        self.state.next_synthetic_id = self.state.next_synthetic_id.saturating_add(1);
        IrNodeId::new(id).expect("synthetic id must be non-zero")
    }

    /// Phase 15 m2-002: Get the current instruction mode (Mode64 if stack is empty).
    /// Will be used in m2-002b for scope-aware mode propagation.
    pub(crate) fn current_mode(&self) -> InstrMode {
        self.state
            .mode_stack
            .last()
            .copied()
            .unwrap_or(InstrMode::Mode64)
    }

    /// Emit epilogue (add rsp if needed, frame-pointer restore if applicable)
    /// followed by ret.
    ///
    /// #1233 Phase A: Looks up frame_size from closure_frame_meta for
    /// current_function. If total_size > 0, emits `add rsp, total_size`
    /// before `ret`.
    ///
    /// paideia-as#1276 phase 3: Also emits the default SysV frame-pointer
    /// epilogue `mov rsp, rbp; pop rbp` before `ret`, mirroring the
    /// `push rbp; mov rbp, rsp` prologue that `visit_lambda` emits at
    /// function entry. Suppressed when `current_function`'s Lambda binding
    /// was annotated `@no_frame`.
    ///
    /// Emission order — enforced by insertion order (each `emit_inst` bumps
    /// `emission_order`):
    ///   1. `add rsp, N`     (closure-frame teardown; existing)
    ///   2. `mov rsp, rbp`   (frame-pointer restore; unless @no_frame)
    ///   3. `pop rbp`        (frame-pointer restore; unless @no_frame)
    ///   4. `ret`
    ///
    /// The `add rsp, N` + `mov rsp, rbp` pair is redundant (the mov
    /// overwrites rsp), but harmless: the add's estimated_bytes are
    /// accounted for and the resulting bytes are equivalent. Keeping the
    /// add preserves byte-exactness for @no_frame closure tests where
    /// only the add fires.
    pub(crate) fn emit_ret(&mut self, ret_id: IrNodeId, arena: &IrArena) {
        // Check if current function has frame layout
        if let Some(lambda_id) = IrNodeId::new(self.state.current_function) {
            if let Some(frame_layout) = arena.closure_frame_meta().get(lambda_id) {
                if frame_layout.total_size > 0 {
                    // Emit: add rsp, total_size
                    let mut add_operands: SmallVec<[Operand; 3]> = SmallVec::new();
                    add_operands.push(Operand::Reg(abi::RSP));
                    add_operands.push(Operand::Imm64(frame_layout.total_size as i64));

                    let add_inst = Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_operands,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode: self.current_mode(),
                        emission_order: 0,
                    };
                    let add_id = self.alloc_synthetic_id();
                    self.emit_inst(add_id, add_inst);
                }
            }
        }

        // paideia-as#1276 phase 3: frame-pointer epilogue for non-@no_frame
        // functions whose prologue actually fired. Matches the lazy
        // `push rbp; mov rbp, rsp` prologue injected by `emit_inst` on the
        // first body instruction. Skip when:
        //   * `current_function == 0` (called outside any Lambda scope — a
        //     caller bug; emitting nothing extra is the safe recovery),
        //   * the lambda opted out via `@no_frame`, OR
        //   * the prologue never actually fired (body-shape arm produced no
        //     instructions, `pending_frame_prologue` is still armed). Emitting
        //     `mov rsp, rbp; pop rbp` without a matching prologue would pop
        //     off the caller's frame — silently corrupting whatever value
        //     was above the return address.
        if self.state.current_function != 0
            && !self.state.is_lambda_no_frame(self.state.current_function)
            && self.state.was_frame_prologue_emitted(self.state.current_function)
        {
            // Emit: mov rsp, rbp
            let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            mov_operands.push(Operand::Reg(abi::RSP));
            mov_operands.push(Operand::Reg(abi::RBP));
            let mov_inst = Instruction {
                mnemonic: Mnemonic::Mov,
                operands: mov_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let mov_id = self.alloc_synthetic_id();
            self.emit_inst(mov_id, mov_inst);

            // Emit: pop rbp
            let mut pop_operands: SmallVec<[Operand; 3]> = SmallVec::new();
            pop_operands.push(Operand::Reg(abi::RBP));
            let pop_inst = Instruction {
                mnemonic: Mnemonic::Pop,
                operands: pop_operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let pop_id = self.alloc_synthetic_id();
            self.emit_inst(pop_id, pop_inst);
        }

        // paideia-as#1278 phase 2: ISR entry stubs must terminate with
        // `iretq`, not `ret`. Suppress the normal ret here; the matching
        // 13-pop GPR restore + optional `add rsp, 8` errcode-skip + `iretq`
        // tail is synthesised by `emit_interrupt_epilogues`, a post-pass
        // that runs after every body path (unsafe or otherwise) has been
        // fully lowered, guaranteeing the epilogue lands at the highest
        // emission_order in this function's .text range. The @no_frame
        // implication set by phase-1 `lower.rs` already suppressed the SysV
        // frame-pointer epilogue block above, so nothing else needs
        // suppressing here — the return path is exclusively the ISR tail.
        if self.state.lambda_interrupt(self.state.current_function).is_some() {
            return;
        }

        // Emit: ret
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
}
