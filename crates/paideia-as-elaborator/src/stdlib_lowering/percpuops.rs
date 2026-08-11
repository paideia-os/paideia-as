//! `PerCpuOps` stdlib-lowering recipes.
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

/// Dispatch a `PerCpuOps::<method_name>` call to its lowering recipe.
///
/// Returns `None` for unknown methods (caller falls through to normal
/// call emission).
pub(super) fn try_lower(
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    match method_name {
        "percpu_inc" => {
            // percpu_inc(counter_gs_offset: u64) → lock inc qword [gs:offset]
            if arg_ids.len() != 1 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "PerCpuOps::percpu_inc",
                }));
            }

            let disp_val = match arena.literal_values().get(arg_ids[0]) {
                Some(val) => val,
                None => {
                    return Some(Err(StdlibLoweringError::NonLiteralArg {
                        arg_index: 0,
                        method: "PerCpuOps::percpu_inc",
                    }));
                }
            };

            // Validate that disp fits in i32
            if disp_val < i32::MIN as i64 || disp_val > i32::MAX as i64 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "PerCpuOps::percpu_inc",
                }));
            }

            let mut operands = SmallVec::new();
            operands.push(Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemDisp {
                    disp: disp_val as i32,
                }),
            });

            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::LockInc {
                        width: paideia_as_ir::instruction::IntWidth::W64,
                    },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
    }],
                arg_convention: ArgConvention::Literal,
                labels: vec![],
            }))
        }
        "percpu_add" => {
            // percpu_add(counter_gs_offset: u64, val: u64) → lock add qword [gs:offset], imm
            if arg_ids.len() != 2 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "PerCpuOps::percpu_add",
                }));
            }

            let disp_val = match arena.literal_values().get(arg_ids[0]) {
                Some(val) => val,
                None => {
                    return Some(Err(StdlibLoweringError::NonLiteralArg {
                        arg_index: 0,
                        method: "PerCpuOps::percpu_add",
                    }));
                }
            };

            let imm_val = match arena.literal_values().get(arg_ids[1]) {
                Some(val) => val,
                None => {
                    return Some(Err(StdlibLoweringError::NonLiteralArg {
                        arg_index: 1,
                        method: "PerCpuOps::percpu_add",
                    }));
                }
            };

            // Validate that disp fits in i32
            if disp_val < i32::MIN as i64 || disp_val > i32::MAX as i64 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "PerCpuOps::percpu_add",
                }));
            }

            let mut operands = SmallVec::new();
            operands.push(Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemDisp {
                    disp: disp_val as i32,
                }),
            });
            operands.push(Operand::Imm64(imm_val));

            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::LockAdd {
                        width: paideia_as_ir::instruction::IntWidth::W64,
                    },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
    }],
                arg_convention: ArgConvention::Literal,
                labels: vec![],
            }))
        }
        "read_u64" => {
            // read_u64(off: u64) -> u64
            //   SysVRegs: RDI = off, RAX = return.
            //   Recipe:   mov rax, [gs:rdi + 0]
            //
            // R18-M2-003 (paideia-os#767): the per-CPU CB is reached via
            // `gs:[rdi + 0]` — treating RDI as the byte offset within the CB.
            // The MemSeg pre-pass unwraps the inner MemSib and prepends the
            // 0x65 GS prefix; the encoder path is proven by the
            // `mov_rax_gs_rax_disp0` test in tests/addressing/gs_relative.rs.
            let mut load_ops = SmallVec::new();
            load_ops.push(Operand::Reg(abi::RAX));
            load_ops.push(Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemSib {
                    base: abi::RDI,
                    index: None,
                    scale: paideia_as_ir::instruction::Scale::X1,
                    disp: 0,
                }),
            });

            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: load_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
                }],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "write_u64" => {
            // write_u64(off: u64, val: u64) -> ()
            //   SysVRegs: RDI = off, RSI = val.
            //   Recipe:   mov [gs:rdi + 0], rsi
            //
            // R18-M2-003 (paideia-os#767). Same addressing as read_u64;
            // store direction. Not atomic — an aligned qword store on x86-64
            // is a single instruction but has release ordering only relative
            // to same-CPU loads; cross-CPU visibility ordering needs an
            // mfence or cmpxchg64 depending on the semantics the caller
            // requires.
            let mut store_ops = SmallVec::new();
            store_ops.push(Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemSib {
                    base: abi::RDI,
                    index: None,
                    scale: paideia_as_ir::instruction::Scale::X1,
                    disp: 0,
                }),
            });
            store_ops.push(Operand::Reg(abi::RSI));

            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::Mov,
                    operands: store_ops,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
                }],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "cmpxchg64" => {
            // cmpxchg64(off: u64, expected: u64, new: u64) -> u64
            //   SysVRegs: RDI = off, RSI = expected, RDX = new,
            //             RAX = return (observed value at [gs:off]).
            //
            //   Recipe:
            //     mov rax, rsi                  ; RAX = expected (cmpxchg comparand)
            //     lock cmpxchg [gs:rdi + 0], rdx
            //
            // Intel SDM Vol 2A CMPXCHG semantics:
            //   IF RAX == [mem] THEN ZF=1, [mem]=src (rdx)
            //   ELSE                 ZF=0, RAX=[mem]
            // Either way, on exit RAX holds the value that was originally in
            // [mem] — which is exactly what the SysV return-value convention
            // expects for a `cmpxchg64` primitive that returns "observed old
            // value" (the CAS-success test is `observed == expected`).
            //
            // Register discipline: the first instruction clobbers RAX before
            // the cmpxchg reads it — safe because SysV RAX is caller-saved
            // scratch on entry (no live value from caller). RDI, RSI, RDX
            // are argument regs and remain intact across the recipe
            // (cmpxchg does not modify [mem]'s base register RDI, and RSI
            // is only used as a mov source in step 1).
            let mut load_expected_ops = SmallVec::new();
            load_expected_ops.push(Operand::Reg(abi::RAX));
            load_expected_ops.push(Operand::Reg(abi::RSI));

            let mut cmpxchg_ops = SmallVec::new();
            cmpxchg_ops.push(Operand::MemSeg {
                seg: SegPrefix::Gs,
                inner: Box::new(Operand::MemSib {
                    base: abi::RDI,
                    index: None,
                    scale: paideia_as_ir::instruction::Scale::X1,
                    disp: 0,
                }),
            });
            cmpxchg_ops.push(Operand::Reg(abi::RDX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Mov,
                        operands: load_expected_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::LockCmpxchg,
                        operands: cmpxchg_ops,
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
        _ => None,
    }
}
