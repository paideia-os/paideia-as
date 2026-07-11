//! Arithmetic / shift lambda emitters.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Hosts the
//! four synthetic lambda lowerings used by the elaborator for common
//! arithmetic patterns:
//!
//! - `emit_add_imm_lambda`     — `lea rax, [rdi + imm]; ret`
//! - `emit_shl_const_var_lambda` — `mov rax, CONST; mov rcx, rdi; shl rax, cl; ret`
//! - `emit_shl_imm_lambda`     — `mov rax, rdi; shl rax, N; ret`
//! - `emit_shl_var_lambda`     — `mov rax, rdi; mov rcx, rsi; shl rax, cl; ret`

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand};
use paideia_as_ir::{IrNodeId, SmallVec, abi};

use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// Emit add-immediate lambda: `lea rax, [src + imm]; ret`.
    /// For small immediates (disp8, -128..127), this is 4 bytes (48 8d 47 NN for SysV, 48 8d 41 NN for MS).
    /// Larger immediates require disp32 (7 bytes).
    /// PA19-r19-006: Use ABI-aware register lookup to support MS x64 calling convention.
    pub(crate) fn emit_add_imm_lambda(&mut self, lambda_node_id: IrNodeId, imm: i64) {
        // Range validation before recording entry (defense-in-depth for #1167).
        let disp = if imm >= -128 && imm <= 127 {
            imm as i32
        } else {
            // For now, only handle disp8; larger immediates can be deferred.
            return;
        };

        let main_id = IrNodeId::new(lambda_node_id.get() * 2).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        // PA19-r19-006: Resolve the calling convention and get the first argument register.
        let cc = self.state.lambda_abi(lambda_node_id.get());
        let src = EmitWalker::param_index_to_reg_for_abi(cc, 0)
            .unwrap_or(abi::RDI); // Fallback to RDI if resolution fails

        // Lea rax, [src + disp8]: 48 8d 47 NN (SysV) or 48 8d 41 NN (MS)
        let mut lea_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        lea_operands.push(Operand::Reg(abi::RAX)); // rax
        lea_operands.push(Operand::MemSib {
            base: src, // rdi (SysV) or rcx (MS)
            index: None,
            scale: paideia_as_ir::instruction::Scale::X1,
            disp,
        });

        let lea_inst = Instruction {
            mnemonic: Mnemonic::Lea,
            operands: lea_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        // Use node_id * 2 for main instruction, * 2 + 1 for ret
        self.emit_inst(main_id, lea_inst);

        // Ret: c3 (1 byte)
        // Emit ret as a separate instruction with node_id * 2 + 1 to sort right after
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

    /// Phase 8 m1-001d: Emit shift-left constant-by-variable lambda: `mov rax, const; mov rcx, rdi; shl rax, cl; ret`.
    ///
    /// Handles `fn (order: u64) -> PAGE_SIZE << order` where PAGE_SIZE is a constant.
    /// The constant is moved into RAX, the variable shift count (in parameter register) is moved to RCX,
    /// then SHL is performed with CL as the count.
    /// Uses 4 instructions (~13 bytes).
    pub(crate) fn emit_shl_const_var_lambda(&mut self, lambda_node_id: IrNodeId, const_val: i64) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 4).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        // PA8-m3-001 (width not available — generic Mov retained): the first move
        // (`mov rax, const`) is `(Reg, Imm64)` and so MovSized-encodable in shape,
        // but this is a *synthetic* lowering of the fixed `CONST << var` pattern.
        // No Let/binding node carries this immediate, so there is no IR width to
        // resolve. The shifted result must also be 64-bit-clean for the `shl
        // rax, cl` that follows, so the full-width move is the safe choice. The
        // two later moves (mov rcx, rdi / shl operands) are reg-reg and cannot be
        // MovSized at all.
        // Mov rax, imm64: 48 b8 XXXXXXXX XXXXXXXX (10 bytes, or fewer for smaller immediates)
        let mut mov1_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov1_operands.push(Operand::Reg(abi::RAX)); // rax
        mov1_operands.push(Operand::Imm64(const_val));

        let mov1_inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: mov1_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        self.emit_inst(main_id, mov1_inst);

        // Mov rcx, rdi: 48 89 f9 (3 bytes)
        // RDI holds the shift count (parameter 0)
        let mut mov2_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov2_operands.push(Operand::Reg(abi::RCX)); // rcx
        mov2_operands.push(Operand::Reg(abi::RDI)); // rdi (arg0)

        let mov2_inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: mov2_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        let mov2_id = IrNodeId::new(lambda_node_id.get() * 4 + 1).expect("mov2 instr virtual id");
        self.emit_inst(mov2_id, mov2_inst);

        // Shl rax, cl: 48 d3 e0 (3 bytes)
        let mut shl_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        shl_operands.push(Operand::Reg(abi::RAX)); // rax
        shl_operands.push(Operand::Reg(abi::RCX)); // rcx (implicit for variable shifts)

        let shl_inst = Instruction {
            mnemonic: Mnemonic::Shl,
            operands: shl_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        let shl_id = IrNodeId::new(lambda_node_id.get() * 4 + 2).expect("shl instr virtual id");
        self.emit_inst(shl_id, shl_inst);

        // Ret: c3 (1 byte)
        let ret_id = IrNodeId::new(lambda_node_id.get() * 4 + 3).expect("ret virtual id");
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

    /// Phase 8 m1-001d: Emit shift-left immediate lambda: `mov rax, rdi; shl rax, imm8; ret`.
    ///
    /// Handles `fn (x) -> x << N` for immediate shift count.
    /// Operands: destination register (RAX), shift count.
    /// Uses 3 instructions: mov + shl + ret (~8 bytes).
    // PA8-m3-001 (generic Mov retained): the `mov rax, rdi` here is reg-to-reg
    // and not MovSized-encodable; the shift operand is an immediate to SHL, not MOV.
    pub(crate) fn emit_shl_imm_lambda(&mut self, lambda_node_id: IrNodeId, shift_count: i64) {
        // Range validation before recording entry (defense-in-depth for #1167).
        let shift = if shift_count >= 0 && shift_count <= 63 {
            shift_count as u8
        } else {
            // Out of range; skip emission
            return;
        };

        let main_id = IrNodeId::new(lambda_node_id.get() * 3).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        // Mov rax, rdi: 48 89 f8 (3 bytes)
        let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov_operands.push(Operand::Reg(abi::RAX)); // rax
        mov_operands.push(Operand::Reg(abi::RDI)); // rdi (arg0)

        let mov_inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: mov_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        self.emit_inst(main_id, mov_inst);

        // Shl rax, imm8: 48 c1 e0 NN (4 bytes)
        let mut shl_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        shl_operands.push(Operand::Reg(abi::RAX)); // rax
        shl_operands.push(Operand::Imm64(shift as i64));

        let shl_inst = Instruction {
            mnemonic: Mnemonic::Shl,
            operands: shl_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        let shl_id = IrNodeId::new(lambda_node_id.get() * 3 + 1).expect("shl instr virtual id");
        self.emit_inst(shl_id, shl_inst);

        // Ret: c3 (1 byte)
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

    /// Phase 8 m1-001d: Emit shift-left variable lambda: `mov rax, rdi; mov rcx, rsi; shl rax, cl; ret`.
    ///
    /// Handles `fn (x) -> x << y` where y is the second parameter (in RSI).
    /// Uses variable shift count in CL register. Uses 4 instructions (~12 bytes).
    pub(crate) fn emit_shl_var_lambda(&mut self, lambda_node_id: IrNodeId) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 4).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        // PA8-m3-001 (generic Mov retained): both moves here (`mov rax, rdi` /
        // `mov rcx, rsi`) are reg-to-reg and not MovSized-encodable.
        // Mov rax, rdi: 48 89 f8 (3 bytes)
        let mut mov1_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov1_operands.push(Operand::Reg(abi::RAX)); // rax
        mov1_operands.push(Operand::Reg(abi::RDI)); // rdi (arg0)

        let mov1_inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: mov1_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        self.emit_inst(main_id, mov1_inst);

        // Mov rcx, rsi: 48 89 f1 (3 bytes)
        let mut mov2_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov2_operands.push(Operand::Reg(abi::RCX)); // rcx
        mov2_operands.push(Operand::Reg(abi::RSI)); // rsi (arg1)

        let mov2_inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: mov2_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        let mov2_id = IrNodeId::new(lambda_node_id.get() * 4 + 1).expect("mov2 instr virtual id");
        self.emit_inst(mov2_id, mov2_inst);

        // Shl rax, cl: 48 d3 e0 (3 bytes)
        let mut shl_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        shl_operands.push(Operand::Reg(abi::RAX)); // rax
        shl_operands.push(Operand::Reg(abi::RCX)); // rcx (implicit for variable shifts)

        let shl_inst = Instruction {
            mnemonic: Mnemonic::Shl,
            operands: shl_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        let shl_id = IrNodeId::new(lambda_node_id.get() * 4 + 2).expect("shl instr virtual id");
        self.emit_inst(shl_id, shl_inst);

        // Ret: c3 (1 byte)
        let ret_id = IrNodeId::new(lambda_node_id.get() * 4 + 3).expect("ret virtual id");
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
