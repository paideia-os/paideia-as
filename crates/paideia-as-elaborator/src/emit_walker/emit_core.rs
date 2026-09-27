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

use paideia_as_ir::abi::{
    ms_return_placement_from_layout, sysv_return_placement_from_layout, MsReturnPlacement,
    SysvReturnPlacement,
};
use paideia_as_ir::instruction::{InstrMode, Instruction, Mnemonic, Operand};
use paideia_as_ir::let_meta::CallingConvention;
use paideia_as_ir::{IrArena, IrNodeId, SmallVec, abi};

use super::EmitWalker;

use crate::aggregate_return::{
    ms_callee_load_return_reg, ms_callee_sret_store, sysv_callee_load_return_pair,
    sysv_callee_sret_store,
};

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
    ///
    /// # PAS-DEBT-B4-002 Slice B / Slice C (paideia-as#1554)
    ///
    /// **Slice C** wires the callee-side aggregate-return splice: if
    /// the current function's Symbol carries a
    /// `return_record_layout`, the pass classifies its SysV / MS
    /// placement and splices the matching helper from
    /// `aggregate_return.rs` before the frame-pointer teardown:
    ///
    ///   * `Memory` placement → `sysv_callee_sret_store` /
    ///     `ms_callee_sret_store` copies the aggregate from a
    ///     callee-local source buffer (allocated inline via
    ///     `sub rsp, padded_size` right before the copy) into the
    ///     caller-provided sret buffer at `[RDI]` (SysV) / `[RCX]`
    ///     (MS), then `mov rax, rdi/rcx` per ABI contract.
    ///   * Register-return (IntSingle, IntPair, SseSingle, SsePair,
    ///     IntSse, SseInt, XmmSingle) → `sysv_callee_load_return_pair`
    ///     / `ms_callee_load_return_reg` loads each eightbyte from
    ///     the callee-local source buffer into the placement's
    ///     return registers.
    ///
    /// **Callee-local source buffer**: allocated with a bare
    /// `sub rsp, padded_size` before the copy. The buffer's contents
    /// come from whatever body-shape arm emitted before this RET —
    /// today that is the arm's own scratch, uninitialised for the
    /// `-> 0` fixtures (Slice C is deliberately scaffolding for
    /// those). Slice D lands a return-position record-cons pass
    /// that writes each field into this buffer BEFORE control
    /// reaches `emit_ret`, giving a semantically correct end-to-end
    /// story.
    ///
    /// The buffer is released by the same `mov rsp, rbp` teardown
    /// below — no matching `add rsp, N` needed for frame-pointer
    /// functions. `@no_frame` functions cannot host record returns
    /// under this design (they lack the RBP anchor); that is an
    /// implicit precondition — the pass leaves them alone.
    ///
    /// See `aggregate_return.rs` for the byte-exact helpers and
    /// `emit_call.rs` for the sibling caller-side wiring.
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

        // PAS-DEBT-B4-002 Slice C (paideia-as#1554): callee-side
        // aggregate-return splice. See the docblock above for the
        // scaffolding-source-buffer rationale.
        self.emit_callee_sret_splice(arena);

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

    /// PAS-DEBT-B4-002 Slice C (paideia-as#1554): splice the
    /// callee-side aggregate-return sequence.
    ///
    /// Fires in `emit_ret` between the closure-frame `add rsp` and
    /// the frame-pointer teardown. Silently no-ops when:
    ///
    ///   * `current_function == 0` (called outside any Lambda scope
    ///     — a caller bug; nothing to splice against),
    ///   * the current Lambda's Symbol has no `return_record_layout`
    ///     (scalar-return path preserved, byte-identical to
    ///     pre-Slice-C behaviour),
    ///   * the current Lambda is `@no_frame` (see the RBP
    ///     precondition in `emit_ret`'s docblock).
    ///
    /// Emitted sequence (SysV, Memory placement, `padded_size = P`):
    /// ```text
    ///   sub rsp, P                    ; scaffolding source buffer
    ///   mov r10, [rsp + 0]            ; sret_store (one qword pair
    ///   mov [rdi + 0], r10            ;  per aggregate eightbyte)
    ///   ... repeated `P/8` times ...
    ///   mov rax, rdi                  ; return sret buffer pointer
    /// ```
    ///
    /// The `mov rsp, rbp` teardown that follows this splice
    /// symmetrically releases the buffer — no matching `add rsp` is
    /// needed here.
    fn emit_callee_sret_splice(&mut self, arena: &IrArena) {
        if self.state.current_function == 0 {
            return;
        }
        if self.state.is_lambda_no_frame(self.state.current_function) {
            return;
        }
        let Some(lambda_id) = IrNodeId::new(self.state.current_function) else {
            return;
        };
        // Resolve the Symbol whose `ir_node` is this Lambda. See
        // `SymbolTable::lookup_by_ir_node` for the O(n) rationale
        // (small n, called once per RET site).
        let Some(sym) = arena.symbols().lookup_by_ir_node(lambda_id) else {
            return;
        };
        let Some(layout) = sym.return_record_layout.as_ref() else {
            return;
        };
        let abi_cc = sym.abi.unwrap_or(CallingConvention::Sysv);

        // Compute the source-buffer padding once — mirrors
        // `emit_call.rs::sret_padded_slot_bytes` on the caller side.
        // 16-multiple round-up preserves SysV `rsp mod 16` post
        // sub-only allocation (paired release is `mov rsp, rbp`).
        let padded_size: u32 = {
            let align = std::cmp::max(layout.align as u64, 16);
            let p = (layout.size + align - 1) & !(align - 1);
            let p16 = (p + 15) & !15;
            p16 as u32
        };
        if padded_size == 0 {
            // Zero-size aggregate → nothing to splice (matches the
            // classifier's `None` placement on the empty layout).
            return;
        }

        // Build the sret helper's instruction stream. Source lives
        // at `[rsp + 0]` right after the sub below.
        let instructions: Vec<Instruction> = match abi_cc {
            CallingConvention::Sysv => {
                let placement = sysv_return_placement_from_layout(layout);
                match placement {
                    SysvReturnPlacement::None => return,
                    SysvReturnPlacement::Memory => {
                        // 8-byte-aligned aggregates only: the helper
                        // asserts. Non-multiple-of-8 sizes require
                        // tail-byte handling that's a documented
                        // follow-up in `aggregate_return.rs`.
                        if (layout.size % 8) != 0 {
                            return;
                        }
                        sysv_callee_sret_store(layout.size as u32, abi::RSP, 0)
                    }
                    // Register-return placements: load from the
                    // source buffer at [rsp + 0]. Non-exhaustive
                    // wildcard per SysvReturnPlacement's
                    // #[non_exhaustive] contract.
                    _ => sysv_callee_load_return_pair(placement, abi::RSP, 0),
                }
            }
            CallingConvention::Ms => {
                let placement = ms_return_placement_from_layout(layout);
                match placement {
                    MsReturnPlacement::None => return,
                    MsReturnPlacement::Memory => {
                        if (layout.size % 8) != 0 {
                            return;
                        }
                        ms_callee_sret_store(layout.size as u32, abi::RSP, 0)
                    }
                    _ => ms_callee_load_return_reg(placement, abi::RSP, 0),
                }
            }
        };

        if instructions.is_empty() {
            return;
        }

        // Emit the source-buffer allocation. Small immediate; no
        // reserved-label pitfall. `sub rsp, imm` uses generic Mov
        // form, no special encoder issues.
        let mut sub_ops: SmallVec<[Operand; 3]> = SmallVec::new();
        sub_ops.push(Operand::Reg(abi::RSP));
        sub_ops.push(Operand::Imm64(padded_size as i64));
        let sub_inst = Instruction {
            mnemonic: Mnemonic::Sub,
            operands: sub_ops,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        let sub_id = self.alloc_synthetic_id();
        self.emit_inst(sub_id, sub_inst);

        // Splice the helper's instruction stream.
        for inst in instructions {
            let iid = self.alloc_synthetic_id();
            self.emit_inst(iid, inst);
        }
    }
}
