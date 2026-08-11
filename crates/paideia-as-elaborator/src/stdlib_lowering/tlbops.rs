//! `TlbOps` stdlib-lowering recipes.
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

/// Dispatch a `TlbOps::<method_name>` call to its lowering recipe.
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
        // PA-v0.21-009 (#1285): TlbOps — invlpg (SysVRegs) + wbinvd (Literal).
        //
        // invlpg_single(va: u64) -> ()
        //   va arrives in RDI; invlpg expects a memory operand [base+0] and
        //   ignores the actual dereferenced value — the base register itself
        //   is what carries the linear address to invalidate.
        "invlpg_single" => {
            let mut ops = SmallVec::new();
            ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::Invlpg,
                    operands: ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
                }],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        // v0.21-009-followup (#1297): TlbOps::invpcid_single / invpcid_all_nonglobal.
        //
        // invpcid_single(pcid: u16, va: u64) -> ()   [INVPCID type=0]
        //   SysV: pcid in RDI (u16 zero-extended), va in RSI.
        //   Descriptor layout (SDM Vol 3A §4.10.4): [pcid_low12:64][va:64].
        //   Recipe:
        //     sub  rsp, 16              ; allocate 128-bit descriptor slot
        //     mov  [rsp + 0], rdi       ; low  64 = pcid (caller ensures low 12 valid)
        //     mov  [rsp + 8], rsi       ; high 64 = va
        //     mov  rax, 0               ; type = 0 (individual-address)
        //     invpcid rax, [rsp]        ; execute
        //     add  rsp, 16              ; restore stack
        //
        // Register discipline: RAX is caller-saved and clobbered by the type
        // load. RDI/RSI die at the recipe boundary — consumed as inputs.
        // RSP is preserved (sub/add pair balances).
        "invpcid_single" => {
            let mut sub_rsp = SmallVec::new();
            sub_rsp.push(Operand::Reg(abi::RSP));
            sub_rsp.push(Operand::Imm64(16));

            let mut store_lo = SmallVec::new();
            store_lo.push(Operand::MemSib {
                base: abi::RSP,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            store_lo.push(Operand::Reg(abi::RDI));

            let mut store_hi = SmallVec::new();
            store_hi.push(Operand::MemSib {
                base: abi::RSP,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 8,
            });
            store_hi.push(Operand::Reg(abi::RSI));

            let mut mov_type = SmallVec::new();
            mov_type.push(Operand::Reg(abi::RAX));
            mov_type.push(Operand::Imm64(0));

            let mut invpcid_ops = SmallVec::new();
            invpcid_ops.push(Operand::Reg(abi::RAX));
            invpcid_ops.push(Operand::MemSib {
                base: abi::RSP,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });

            let mut add_rsp = SmallVec::new();
            add_rsp.push(Operand::Reg(abi::RSP));
            add_rsp.push(Operand::Imm64(16));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Sub,
                        operands: sub_rsp,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: store_lo,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: store_hi,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_type,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Invpcid,
                        operands: invpcid_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_rsp,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        // invpcid_all_nonglobal(pcid: u16) -> ()   [INVPCID type=1]
        //   SysV: pcid in RDI (u16 zero-extended). Linear-address slot must be
        //   zero (SDM: reserved bits in descriptor must be 0, even though type=1
        //   ignores the linear-addr field).
        //   Recipe:
        //     sub  rsp, 16
        //     mov  [rsp + 0], rdi       ; low 64 = pcid
        //     xor  rax, rax             ; rax = 0 (scratch for zero store + type)
        //     mov  [rsp + 8], rax       ; high 64 = 0
        //     mov  rax, 1               ; type = 1 (single-context / all-nonglobal)
        //     invpcid rax, [rsp]
        //     add  rsp, 16
        "invpcid_all_nonglobal" => {
            let mut sub_rsp = SmallVec::new();
            sub_rsp.push(Operand::Reg(abi::RSP));
            sub_rsp.push(Operand::Imm64(16));

            let mut store_lo = SmallVec::new();
            store_lo.push(Operand::MemSib {
                base: abi::RSP,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            store_lo.push(Operand::Reg(abi::RDI));

            let mut xor_rax = SmallVec::new();
            xor_rax.push(Operand::Reg(abi::RAX));
            xor_rax.push(Operand::Reg(abi::RAX));

            let mut store_hi = SmallVec::new();
            store_hi.push(Operand::MemSib {
                base: abi::RSP,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 8,
            });
            store_hi.push(Operand::Reg(abi::RAX));

            let mut mov_type = SmallVec::new();
            mov_type.push(Operand::Reg(abi::RAX));
            mov_type.push(Operand::Imm64(1));

            let mut invpcid_ops = SmallVec::new();
            invpcid_ops.push(Operand::Reg(abi::RAX));
            invpcid_ops.push(Operand::MemSib {
                base: abi::RSP,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });

            let mut add_rsp = SmallVec::new();
            add_rsp.push(Operand::Reg(abi::RSP));
            add_rsp.push(Operand::Imm64(16));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Sub,
                        operands: sub_rsp,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: store_lo,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Xor,
                        operands: xor_rax,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: store_hi,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: mov_type,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Invpcid,
                        operands: invpcid_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_rsp,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        // flush_cache_writeback() -> () — nullary WBINVD.
        "flush_cache_writeback" => {
            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::Wbinvd,
                    operands: SmallVec::new(),
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
                }],
                arg_convention: ArgConvention::Literal,
                labels: vec![],
            }))
        }
        _ => None,
    }
}
