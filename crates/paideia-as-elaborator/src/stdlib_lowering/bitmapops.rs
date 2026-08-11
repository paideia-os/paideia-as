//! `BitmapOps` stdlib-lowering recipes.
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

/// Dispatch a `BitmapOps::<method_name>` call to its lowering recipe.
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
        // PA-v0.21-003 (#1279): BitmapOps atomic bit-manipulation primitives.
        // Trait declared at paideia-stdlib/pdx/bitmap.pdx (PA-R16-010).
        // SysVRegs: RDI = bmap (*u64), RSI = bit_index (u64), RAX = return
        //           (previous bit as bool, 0 or 1).
        //
        // The AC says "compile to lock bts / btr" — each primitive here
        // lowers to the corresponding lock-prefixed bit-manipulation
        // instruction followed by a `setc al / movzx eax, al` tail that
        // hoists CF (the previous-bit value delivered by BTS/BTR/BTC) into
        // the SysV return register.
        //
        // Non-atomic BitmapOps (bitmap_get, bitmap_word_count,
        // bitmap_first_free) and the compound bitmap_claim_first_free
        // require additional sequences (bt for get, arithmetic-only for
        // word_count, per-word bsf scan for first_free, retry loop for
        // claim_first_free); they remain deferred and fall through to
        // normal call emission.
        "bitmap_set" => {
            // lock bts_q [rdi], rsi    ; F0 48 0F AB 37       (5 bytes)
            // setc al                  ; 0F 92 C0             (3 bytes)
            // movzx eax, al            ; 48 0F B6 C0          (4 bytes)
            use paideia_as_ir::instruction::Cond;

            let mut bts_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            bts_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            bts_ops.push(Operand::Reg(abi::RSI));

            let mut setc_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            setc_ops.push(Operand::Reg(abi::RAX));

            let mut movzx_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            movzx_ops.push(Operand::Reg(abi::RAX));
            movzx_ops.push(Operand::Reg(abi::RAX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::LockBts { width: IntWidth::W64 },
                        operands: bts_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Setcc(Cond::Below),
                        operands: setc_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Movzx,
                        operands: movzx_ops,
                        encoding_hint: Some(paideia_as_ir::instruction::EncodingHint {
                            opcode: 0x0F_B6,
                            operand_size: 1,
                        }),
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "bitmap_clear" => {
            // lock btr_q [rdi], rsi    ; F0 48 0F B3 37       (5 bytes)
            // setc al                  ; 0F 92 C0             (3 bytes)
            // movzx eax, al            ; 48 0F B6 C0          (4 bytes)
            use paideia_as_ir::instruction::Cond;

            let mut btr_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            btr_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            btr_ops.push(Operand::Reg(abi::RSI));

            let mut setc_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            setc_ops.push(Operand::Reg(abi::RAX));

            let mut movzx_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            movzx_ops.push(Operand::Reg(abi::RAX));
            movzx_ops.push(Operand::Reg(abi::RAX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::LockBtr { width: IntWidth::W64 },
                        operands: btr_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Setcc(Cond::Below),
                        operands: setc_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Movzx,
                        operands: movzx_ops,
                        encoding_hint: Some(paideia_as_ir::instruction::EncodingHint {
                            opcode: 0x0F_B6,
                            operand_size: 1,
                        }),
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "bitmap_toggle" => {
            // lock btc_q [rdi], rsi    ; F0 48 0F BB 37       (5 bytes)
            // setc al                  ; 0F 92 C0             (3 bytes)
            // movzx eax, al            ; 48 0F B6 C0          (4 bytes)
            use paideia_as_ir::instruction::Cond;

            let mut btc_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            btc_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            btc_ops.push(Operand::Reg(abi::RSI));

            let mut setc_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            setc_ops.push(Operand::Reg(abi::RAX));

            let mut movzx_ops: SmallVec<[Operand; 3]> = SmallVec::new();
            movzx_ops.push(Operand::Reg(abi::RAX));
            movzx_ops.push(Operand::Reg(abi::RAX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::LockBtc { width: IntWidth::W64 },
                        operands: btc_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Setcc(Cond::Below),
                        operands: setc_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
                    },
                    Instruction {
                        mnemonic: Mnemonic::Movzx,
                        operands: movzx_ops,
                        encoding_hint: Some(paideia_as_ir::instruction::EncodingHint {
                            opcode: 0x0F_B6,
                            operand_size: 1,
                        }),
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
