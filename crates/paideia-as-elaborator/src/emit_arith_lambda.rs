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
use paideia_as_ir::{IrArena, IrNodeId, SmallVec, abi};

use crate::emit_walker::EmitWalker;

impl EmitWalker {
    /// Emit add-immediate lambda: `lea rax, [src + imm]; ret`.
    /// For small immediates (disp8, -128..127), this is 4 bytes (48 8d 47 NN for SysV, 48 8d 41 NN for MS).
    /// Larger immediates require disp32 (7 bytes).
    /// PA19-r19-006: Use ABI-aware register lookup to support MS x64 calling convention.
    pub(crate) fn emit_add_imm_lambda(&mut self, lambda_node_id: IrNodeId, imm: i64, arena: &IrArena) {
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
        self.emit_ret(ret_id, arena);
    }

    /// Phase 8 m1-001d: Emit shift-left constant-by-variable lambda: `mov rax, const; mov rcx, <arg0>; shl rax, cl; ret`.
    ///
    /// Handles `fn (order: u64) -> PAGE_SIZE << order` where PAGE_SIZE is a constant.
    /// The constant is moved into RAX, the variable shift count (in parameter register) is moved to RCX,
    /// then SHL is performed with CL as the count.
    /// Uses 4 instructions (~13 bytes).
    ///
    /// v0.21-001 (#1277): ABI-aware — arg0 is RDI for SysV, RCX for MS x64.
    /// When arg0 is already in RCX (MS x64), the mov rcx, rcx is retained for
    /// structural uniformity; the encoder collapses it to no-op semantics.
    pub(crate) fn emit_shl_const_var_lambda(&mut self, lambda_node_id: IrNodeId, const_val: i64, arena: &IrArena) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 4).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        let cc = self.state.lambda_abi(lambda_node_id.get());
        let arg0 = Self::param_index_to_reg_for_abi(cc, 0).unwrap_or(abi::RDI);

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

        // Mov rcx, <arg0>: 48 89 f9 (SysV) or 48 89 c9 (MS, no-op)
        let mut mov2_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov2_operands.push(Operand::Reg(abi::RCX)); // rcx
        mov2_operands.push(Operand::Reg(arg0));     // rdi (SysV arg0) or rcx (MS arg0)

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
        self.emit_ret(ret_id, arena);
    }

    /// Phase 8 m1-001d: Emit shift-left immediate lambda: `mov rax, <arg0>; shl rax, imm8; ret`.
    ///
    /// Handles `fn (x) -> x << N` for immediate shift count.
    /// Operands: destination register (RAX), shift count.
    /// Uses 3 instructions: mov + shl + ret (~8 bytes).
    ///
    /// v0.21-001 (#1277): ABI-aware — arg0 is RDI for SysV, RCX for MS x64.
    pub(crate) fn emit_shl_imm_lambda(&mut self, lambda_node_id: IrNodeId, shift_count: i64, arena: &IrArena) {
        // Range validation before recording entry (defense-in-depth for #1167).
        let shift = if shift_count >= 0 && shift_count <= 63 {
            shift_count as u8
        } else {
            // Out of range; skip emission
            return;
        };

        let main_id = IrNodeId::new(lambda_node_id.get() * 3).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        let cc = self.state.lambda_abi(lambda_node_id.get());
        let arg0 = Self::param_index_to_reg_for_abi(cc, 0).unwrap_or(abi::RDI);

        // Mov rax, <arg0>
        let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov_operands.push(Operand::Reg(abi::RAX)); // rax
        mov_operands.push(Operand::Reg(arg0));     // rdi (SysV) or rcx (MS)

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
        self.emit_ret(ret_id, arena);
    }

    /// Phase 8 m1-001d: Emit shift-left variable lambda: `mov rax, <arg0>; mov rcx, <arg1>; shl rax, cl; ret`.
    ///
    /// Handles `fn (x, y) -> x << y`. Uses variable shift count in CL register.
    /// Uses 4 instructions (~12 bytes).
    ///
    /// v0.21-001 (#1277): ABI-aware — arg0/arg1 pair is (RDI, RSI) for SysV,
    /// (RCX, RDX) for MS x64. When arg0 is RCX (MS x64), the mov rax, rcx +
    /// mov rcx, rdx sequence is safe: the arg0→RAX move runs first, so RCX
    /// is free to receive arg1 before SHL uses CL.
    pub(crate) fn emit_shl_var_lambda(&mut self, lambda_node_id: IrNodeId, arena: &IrArena) {
        let main_id = IrNodeId::new(lambda_node_id.get() * 4).expect("main instr virtual id");
        self.record_lambda_entry(lambda_node_id, main_id);

        let cc = self.state.lambda_abi(lambda_node_id.get());
        let arg0 = Self::param_index_to_reg_for_abi(cc, 0).unwrap_or(abi::RDI);
        let arg1 = Self::param_index_to_reg_for_abi(cc, 1).unwrap_or(abi::RSI);

        // Mov rax, <arg0>
        let mut mov1_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov1_operands.push(Operand::Reg(abi::RAX)); // rax
        mov1_operands.push(Operand::Reg(arg0));     // rdi (SysV) or rcx (MS)

        let mov1_inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: mov1_operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: self.current_mode(),
                    emission_order: 0,
        };

        self.emit_inst(main_id, mov1_inst);

        // Mov rcx, <arg1>
        let mut mov2_operands: SmallVec<[Operand; 3]> = SmallVec::new();
        mov2_operands.push(Operand::Reg(abi::RCX)); // rcx
        mov2_operands.push(Operand::Reg(arg1));     // rsi (SysV) or rdx (MS)

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
        self.emit_ret(ret_id, arena);
    }
}
