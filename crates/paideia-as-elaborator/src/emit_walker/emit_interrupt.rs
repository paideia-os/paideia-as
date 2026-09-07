//! EmitWalker — ISR (interrupt handler) entry/exit stub emission.
//!
//! paideia-as#1278 phase 2. Provides:
//!   * `emit_interrupt_prologue` — 13-GPR spill + `cld` at ISR entry.
//!   * `emit_interrupt_epilogue` — mirror pop chain + optional 8-byte
//!     error-code skip + `iretq`.
//!   * `emit_interrupt_epilogues` — the post-pass that runs the epilogue
//!     for every ISR-marked lambda whose epilogue hasn't yet been emitted,
//!     then syncs into the arena's instruction side-table.
//!
//! Split from `emit_walker.rs` (paideia-as#1411).

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrArena, IrNodeId, SmallVec, abi};

use super::EmitWalker;

impl EmitWalker {
    /// paideia-as#1278 phase 2: emit the ISR entry-stub prologue for the
    /// current lambda — the 13-push GPR spill (`rax, rcx, rdx, rsi, rdi,
    /// r8, r9, r10, r11, r12, r13, r14, r15` in that order) followed by
    /// `cld` (clear direction flag so string ops in the body use forward
    /// stepping regardless of caller state — required by Intel SDM Vol. 3A
    /// §6.15 discipline for interrupt handlers).
    ///
    /// Called from `emit_visit_lambda::visit_lambda` at Lambda entry when
    /// `EmitPassState::lambda_interrupt(lambda_id).is_some()`, ahead of the
    /// body-shape dispatch. Idempotent: guarded by
    /// `emitted_interrupt_prologue` so a second call for the same lambda
    /// (should any future path invoke `visit_lambda` more than once for the
    /// same node) is a no-op.
    ///
    /// The 13-push list omits `rbp` (a callee-saved frame register that
    /// interrupted code is entitled to have restored transparently — a
    /// handler that touches it must save/restore inside its own body) and
    /// `rsp` (the stack pointer itself; the CPU IST-switch or the current
    /// stack already holds the interrupted state, and `push rsp` would
    /// smash the just-saved value). `rbx` is likewise omitted per the same
    /// callee-saved convention. Between them these three fully cover the
    /// callee-saved GPR set; the ISR body is responsible for saving them
    /// if it uses them.
    pub(crate) fn emit_interrupt_prologue(&mut self, lambda_id: IrNodeId) {
        let fn_id = lambda_id.get();
        if self.state.was_interrupt_prologue_emitted(fn_id) {
            return;
        }
        self.state.mark_interrupt_prologue_emitted(fn_id);

        // Order matches Intel SDM Vol. 3A §6.15 convention for handler
        // spills: caller-saved GPRs first (rax, rcx, rdx, rsi, rdi), then
        // extended (r8-r11), then callee-saved extended (r12-r15). The
        // matching pop sequence in `emit_interrupt_epilogue` walks the
        // reverse.
        const ISR_SPILL_REGS: [paideia_as_ir::instruction::RegId; 13] = [
            abi::RAX, abi::RCX, abi::RDX, abi::RSI, abi::RDI,
            abi::R8, abi::R9, abi::R10, abi::R11,
            abi::R12, abi::R13, abi::R14, abi::R15,
        ];
        let mut first_push_id: Option<IrNodeId> = None;
        for reg in ISR_SPILL_REGS {
            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
            operands.push(Operand::Reg(reg));
            let push_inst = Instruction {
                mnemonic: Mnemonic::Push,
                operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let push_id = self.alloc_synthetic_id();
            if first_push_id.is_none() {
                first_push_id = Some(push_id);
            }
            self.emit_inst(push_id, push_inst);
        }

        // Pin `lambda_first_instr` to the first `push rax` so the emitted
        // ELF symbol for this ISR starts AT the spill chain, not after it.
        // Body-shape arms in `visit_lambda` use `record_lambda_entry`
        // (or_insert) and `arm_pending_first_instr_unless_claimed` — both
        // gated on the absence of an existing entry — so recording here
        // makes those calls no-ops. `cmd_build`'s post-`UnsafeWalker` wire
        // that would otherwise unconditionally overwrite this entry with
        // the unsafe body's first instruction is explicitly skipped for
        // interrupt-marked lambdas (see the `lambda_interrupt(...).is_some()`
        // guard around `insert_lambda_first_instr` in cmd_build.rs).
        if let Some(push_id) = first_push_id {
            self.record_lambda_entry(lambda_id, push_id);
        }

        // Emit: cld — clear direction flag so string ops in the body see a
        // known-forward DF regardless of caller state.
        let cld_inst = Instruction {
            mnemonic: Mnemonic::Cld,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        let cld_id = self.alloc_synthetic_id();
        self.emit_inst(cld_id, cld_inst);
    }

    /// paideia-as#1278 phase 2: emit the ISR entry-stub epilogue for the
    /// current lambda — the mirror of `emit_interrupt_prologue`: 13-pop
    /// GPR restore in reverse order (r15, r14, r13, r12, r11, r10, r9, r8,
    /// rdi, rsi, rdx, rcx, rax), an optional `add rsp, 8` to skip the
    /// CPU-pushed error code (when `attr.has_error_code == true`, i.e. the
    /// source `@interrupt_error("…")` form), and finally `iretq`.
    ///
    /// This helper does NOT touch the `emitted_interrupt_epilogue` guard —
    /// its callers do, because there are (potentially) two: the post-pass
    /// `emit_interrupt_epilogues` and — if a future in-line path takes over
    /// the emission — a caller inside `emit_ret`. Keeping the guard flip on
    /// the caller side lets the helper stay a pure emitter and mirrors the
    /// division of labour in `emit_interrupt_prologue`'s
    /// `was_interrupt_prologue_emitted` gate.
    pub(crate) fn emit_interrupt_epilogue(&mut self, attr: &paideia_as_ir::let_meta::InterruptAttr) {
        // Reverse of the prologue spill list.
        const ISR_RESTORE_REGS: [paideia_as_ir::instruction::RegId; 13] = [
            abi::R15, abi::R14, abi::R13, abi::R12,
            abi::R11, abi::R10, abi::R9, abi::R8,
            abi::RDI, abi::RSI, abi::RDX, abi::RCX, abi::RAX,
        ];
        for reg in ISR_RESTORE_REGS {
            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
            operands.push(Operand::Reg(reg));
            let pop_inst = Instruction {
                mnemonic: Mnemonic::Pop,
                operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let pop_id = self.alloc_synthetic_id();
            self.emit_inst(pop_id, pop_inst);
        }

        // If the vector carries a CPU-pushed error code (Intel SDM Vol. 3A
        // §6.15 vectors 8, 10, 11, 12, 13, 14, 17, 21, 29, 30 — recorded
        // in `attr.has_error_code` by phase-1 parser via the
        // `@interrupt_error(…)` spelling), skip the 8-byte err-code slot
        // between the topmost GPR restore and `iretq`. Without this,
        // `iretq` would pop the error code as RIP and blow the return.
        if attr.has_error_code {
            let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
            operands.push(Operand::Reg(abi::RSP));
            operands.push(Operand::Imm64(8));
            let add_inst = Instruction {
                mnemonic: Mnemonic::Add,
                operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: self.current_mode(),
                emission_order: 0,
            };
            let add_id = self.alloc_synthetic_id();
            self.emit_inst(add_id, add_inst);
        }

        // Emit: iretq — return from interrupt, 64-bit.
        let iretq_inst = Instruction {
            mnemonic: Mnemonic::Iretq,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
            emission_order: 0,
        };
        let iretq_id = self.alloc_synthetic_id();
        self.emit_inst(iretq_id, iretq_inst);
    }

    /// paideia-as#1278 phase 2: post-pass that emits the ISR epilogue tail
    /// for every lambda marked as an interrupt handler whose epilogue has
    /// not yet been emitted.
    ///
    /// Runs after the main walker, `UnsafeWalker::run`, and
    /// `emit_pending_unsafe_bodies` have all completed — i.e. after every
    /// body instruction (raw asm for unsafe bodies, elaborator-lowered for
    /// non-unsafe bodies) has consumed its share of `next_emission_order`.
    /// Emitting here guarantees the pop-restore + optional errcode-skip +
    /// `iretq` tail lands at strictly higher emission_order than anything
    /// in the body, which is what the text emitter's
    /// `(emission_order, node_id)` sort uses to place instructions in
    /// program order.
    ///
    /// The `sync_state_instructions_to_arena` tail is critical: this pass
    /// runs after `walk_inner`'s own sync, so instructions emitted here
    /// would otherwise be stranded in `state.instructions` and silently
    /// absent from the encoded .text (same class of bug as pre-#1146 for
    /// `emit_pending_unsafe_bodies`).
    pub fn emit_interrupt_epilogues(&mut self, arena: &mut IrArena) {
        // Snapshot before mutating so the borrow of `state.lambda_interrupt`
        // doesn't overlap the `mut self` calls below.
        let entries: Vec<(u32, paideia_as_ir::let_meta::InterruptAttr)> = self
            .state
            .interrupt_lambdas()
            .filter(|(id, _)| !self.state.was_interrupt_epilogue_emitted(**id))
            .map(|(id, attr)| (*id, attr.clone()))
            .collect();

        for (lambda_id, attr) in entries {
            // Route the emit through `current_function` so instr_to_lambda
            // records this tail against the correct owner (needed by
            // resolve_var_operands and by function_instructions test
            // helpers). Save/restore the prior value in case a caller
            // relies on it after this returns.
            let prev = self.state.current_function;
            self.state.current_function = lambda_id;
            self.emit_interrupt_epilogue(&attr);
            self.state.mark_interrupt_epilogue_emitted(lambda_id);
            self.state.current_function = prev;
        }

        // Transfer this pass's instructions into the arena — same rationale
        // as the closing sync in `walk_inner` and in
        // `emit_pending_unsafe_bodies`. Idempotent, so re-syncing entries
        // already copied by prior passes is a no-op.
        self.sync_state_instructions_to_arena(arena);
    }
}
