//! `MsrOps` stdlib-lowering recipes.
//!
//! Extracted 2026-08-11 from `stdlib_lowering.rs` (God-file refactor).
//! Each stdlib trait family lives in its own submodule so unrelated
//! intrinsic families evolve independently.

#![allow(unused_imports)]

use paideia_as_ir::{
    IrArena, IrNodeId, SmallVec, abi,
    instruction::{InstrMode, Instruction, IntWidth, Mnemonic, Operand, SegPrefix},
};

use super::{ArgConvention, LoweringRecipe, StdlibLoweringError};

/// Dispatch a `MsrOps::<method_name>` call to its lowering recipe.
///
/// Returns `None` for unknown methods (caller falls through to normal
/// call emission).
pub(super) fn try_lower(
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    let _ = (arg_ids, arena);
    match method_name {
        // PA-v0.21-008 (#1284): MsrOps — typed rdmsr/wrmsr with SysV marshalling.
        //
        // rdmsr(idx: u32) -> u64
        //   idx  arrives in EDI (SysV arg 0; upper 32 of RDI = 0 for u32).
        //   RCX  ← RDI       (mov r64,r64; drops caller's high bits — a u32
        //                     arg's upper 32 are already zero, so RCX = idx.)
        //   rdmsr            reads MSR[ECX]; hardware zero-extends RAX/RDX
        //                     upper 32 in 64-bit mode (SDM Vol 2B RDMSR).
        //   shl rdx, 32      RDX high half now in position for the pack.
        //   or  rax, rdx     RAX = (EDX << 32) | EAX; matches SysV return.
        "rdmsr" => {
            let mut mov_ops = SmallVec::new();
            mov_ops.push(Operand::Reg(abi::RCX));
            mov_ops.push(Operand::Reg(abi::RDI));

            let mut shl_ops = SmallVec::new();
            shl_ops.push(Operand::Reg(abi::RDX));
            shl_ops.push(Operand::Imm64(32));

            let mut or_ops = SmallVec::new();
            or_ops.push(Operand::Reg(abi::RAX));
            or_ops.push(Operand::Reg(abi::RDX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Rdmsr,
                        operands: SmallVec::new(),
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Shl,
                        operands: shl_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Or,
                        operands: or_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
                extern_target: None,
            }))
        }
        // wrmsr(idx: u32, val: u64) -> ()
        //   idx  arrives in EDI; val in RSI.
        //   RCX  ← RDI       (idx into ECX slot.)
        //   RAX  ← RSI       (val low half naturally in EAX.)
        //   RDX  ← RSI       (copy val to RDX, then shift down.)
        //   shr rdx, 32      RDX = val >> 32; upper 32 zeroed by shr.
        //   wrmsr            MSR[ECX] ← EDX:EAX.
        "wrmsr" => {
            let mut mov_rcx = SmallVec::new();
            mov_rcx.push(Operand::Reg(abi::RCX));
            mov_rcx.push(Operand::Reg(abi::RDI));

            let mut mov_rax = SmallVec::new();
            mov_rax.push(Operand::Reg(abi::RAX));
            mov_rax.push(Operand::Reg(abi::RSI));

            let mut mov_rdx = SmallVec::new();
            mov_rdx.push(Operand::Reg(abi::RDX));
            mov_rdx.push(Operand::Reg(abi::RSI));

            let mut shr_rdx = SmallVec::new();
            shr_rdx.push(Operand::Reg(abi::RDX));
            shr_rdx.push(Operand::Imm64(32));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_rcx,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_rax,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_rdx,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Shr,
                        operands: shr_rdx,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Wrmsr,
                        operands: SmallVec::new(),
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
                extern_target: None,
            }))
        }
        _ => None,
    }
}
