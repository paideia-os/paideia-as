//! `BulkMemOps` stdlib-lowering recipes.
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

/// Dispatch a `BulkMemOps::<method_name>` call to its lowering recipe.
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
        // #1228 (Phase 2 of #1064): BulkMemOps REP-string bulk-memory primitives.
        // SysVRegs: RDI = dest, RSI = src/fill, RDX = count. REP uses implicit RCX,
        // so each recipe first moves the SysV count (RDX) into RCX.
        "memcpy" => {
            // memcpy(dest, src, n) -> (): mov rcx, rdx; rep movsb
            let build_inst = |mnemonic, ops: SmallVec<[Operand; 3]>| Instruction {
                mnemonic,
                operands: ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode,
                emission_order: 0,
            };
            let make_ops = |ops: Vec<Operand>| -> SmallVec<[Operand; 3]> {
                let mut sv = SmallVec::new();
                for op in ops {
                    sv.push(op);
                }
                sv
            };

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    // 0: mov rcx, rdx              ; RCX = byte count (REP implicit counter)
                    build_inst(Mnemonic::Mov, make_ops(vec![Operand::Reg(abi::RCX), Operand::Reg(abi::RDX)])),
                    // 1: rep movsb                 ; copy [RSI]->[RDI], RCX times
                    build_inst(Mnemonic::RepMovsb, make_ops(vec![])),
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "memset" => {
            // memset(dest, fill, n) -> (): mov rax, rsi; mov rcx, rdx; rep stosb
            let build_inst = |mnemonic, ops: SmallVec<[Operand; 3]>| Instruction {
                mnemonic,
                operands: ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode,
                emission_order: 0,
            };
            let make_ops = |ops: Vec<Operand>| -> SmallVec<[Operand; 3]> {
                let mut sv = SmallVec::new();
                for op in ops {
                    sv.push(op);
                }
                sv
            };

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    // 0: mov rax, rsi              ; AL = fill byte (STOSB stores AL)
                    build_inst(Mnemonic::Mov, make_ops(vec![Operand::Reg(abi::RAX), Operand::Reg(abi::RSI)])),
                    // 1: mov rcx, rdx              ; RCX = byte count (REP implicit counter)
                    build_inst(Mnemonic::Mov, make_ops(vec![Operand::Reg(abi::RCX), Operand::Reg(abi::RDX)])),
                    // 2: rep stosb                 ; store AL to [RDI], RCX times
                    build_inst(Mnemonic::RepStosb, make_ops(vec![])),
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "memcpy_qwords" => {
            // memcpy_qwords(dest, src, n_qwords) -> (): mov rcx, rdx; rep movsq
            let build_inst = |mnemonic, ops: SmallVec<[Operand; 3]>| Instruction {
                mnemonic,
                operands: ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode,
                emission_order: 0,
            };
            let make_ops = |ops: Vec<Operand>| -> SmallVec<[Operand; 3]> {
                let mut sv = SmallVec::new();
                for op in ops {
                    sv.push(op);
                }
                sv
            };

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    // 0: mov rcx, rdx              ; RCX = qword count (REP implicit counter)
                    build_inst(Mnemonic::Mov, make_ops(vec![Operand::Reg(abi::RCX), Operand::Reg(abi::RDX)])),
                    // 1: rep movsq                 ; copy qword [RSI]->[RDI], RCX times
                    build_inst(Mnemonic::RepMovsq, make_ops(vec![])),
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "memset_qwords" => {
            // memset_qwords(dest, fill_qword, n_qwords) -> (): mov rax, rsi; mov rcx, rdx; rep stosq
            let build_inst = |mnemonic, ops: SmallVec<[Operand; 3]>| Instruction {
                mnemonic,
                operands: ops,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode,
                emission_order: 0,
            };
            let make_ops = |ops: Vec<Operand>| -> SmallVec<[Operand; 3]> {
                let mut sv = SmallVec::new();
                for op in ops {
                    sv.push(op);
                }
                sv
            };

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    // 0: mov rax, rsi              ; RAX = fill qword (STOSQ stores RAX)
                    build_inst(Mnemonic::Mov, make_ops(vec![Operand::Reg(abi::RAX), Operand::Reg(abi::RSI)])),
                    // 1: mov rcx, rdx              ; RCX = qword count (REP implicit counter)
                    build_inst(Mnemonic::Mov, make_ops(vec![Operand::Reg(abi::RCX), Operand::Reg(abi::RDX)])),
                    // 2: rep stosq                 ; store RAX to [RDI], RCX times
                    build_inst(Mnemonic::RepStosq, make_ops(vec![])),
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        _ => None,
    }
}
