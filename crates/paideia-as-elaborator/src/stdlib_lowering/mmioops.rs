//! `MmioOps` stdlib-lowering recipes.
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

/// Dispatch a `MmioOps::<method_name>` call to its lowering recipe.
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
        "mmio_read_u32" => {
            // mmio_read_u32(addr: u64) → mov eax, dword [addr]
            if arg_ids.len() != 1 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "MmioOps::mmio_read_u32",
                }));
            }

            let addr_val = match arena.literal_values().get(arg_ids[0]) {
                Some(val) => val,
                None => {
                    return Some(Err(StdlibLoweringError::NonLiteralArg {
                        arg_index: 0,
                        method: "MmioOps::mmio_read_u32",
                    }));
                }
            };

            // Validate that addr fits in i32
            if addr_val < i32::MIN as i64 || addr_val > i32::MAX as i64 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "MmioOps::mmio_read_u32",
                }));
            }

            let mut operands = SmallVec::new();
            operands.push(Operand::Reg(abi::RAX));
            operands.push(Operand::MemDisp {
                disp: addr_val as i32,
            });

            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::MovSized {
                        width: IntWidth::W32,
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
        "mmio_write_u32" => {
            // mmio_write_u32(addr: u64, val: u32) → mov dword [addr], imm
            if arg_ids.len() != 2 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "MmioOps::mmio_write_u32",
                }));
            }

            let addr_val = match arena.literal_values().get(arg_ids[0]) {
                Some(val) => val,
                None => {
                    return Some(Err(StdlibLoweringError::NonLiteralArg {
                        arg_index: 0,
                        method: "MmioOps::mmio_write_u32",
                    }));
                }
            };

            let val_val = match arena.literal_values().get(arg_ids[1]) {
                Some(val) => val,
                None => {
                    return Some(Err(StdlibLoweringError::NonLiteralArg {
                        arg_index: 1,
                        method: "MmioOps::mmio_write_u32",
                    }));
                }
            };

            // Validate that addr fits in i32
            if addr_val < i32::MIN as i64 || addr_val > i32::MAX as i64 {
                return Some(Err(StdlibLoweringError::NonLiteralArg {
                    arg_index: 0,
                    method: "MmioOps::mmio_write_u32",
                }));
            }

            let mut operands = SmallVec::new();
            operands.push(Operand::MemDisp {
                disp: addr_val as i32,
            });
            operands.push(Operand::Imm64(val_val));

            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::MovSized {
                        width: IntWidth::W32,
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
        // PA-v0.21-013 (#1289): MmioOps — u8 / u16 / u64 volatile lowering.
        // Mirrors the existing mmio_read_u32 / mmio_write_u32 shape with
        // MovSized{W8/W16/W64}. Address is a compile-time literal (Literal
        // convention); if a caller needs a runtime address, they lift the
        // address into a register and use raw asm — the recipe explicitly
        // rejects non-literal addresses so no silent fall-through occurs.
        "mmio_read_u8" => {
            mmio_read_recipe_literal(arg_ids, arena, mode, IntWidth::W8, "MmioOps::mmio_read_u8")
        }
        "mmio_read_u16" => {
            mmio_read_recipe_literal(arg_ids, arena, mode, IntWidth::W16, "MmioOps::mmio_read_u16")
        }
        "mmio_read_u64" => {
            mmio_read_recipe_literal(arg_ids, arena, mode, IntWidth::W64, "MmioOps::mmio_read_u64")
        }
        "mmio_write_u8" => {
            mmio_write_recipe_literal(arg_ids, arena, mode, IntWidth::W8, "MmioOps::mmio_write_u8")
        }
        "mmio_write_u16" => {
            mmio_write_recipe_literal(arg_ids, arena, mode, IntWidth::W16, "MmioOps::mmio_write_u16")
        }
        "mmio_write_u64" => {
            mmio_write_recipe_literal(arg_ids, arena, mode, IntWidth::W64, "MmioOps::mmio_write_u64")
        }
        _ => None,
    }
}

/// Shared helper for MmioOps::mmio_read_uN (N ∈ {8,16,64}) — Literal
/// convention with compile-time-literal address. Emits one MovSized{width}
/// with (RAX, [disp32]) operands. Extracted so the six new arms above stay
/// single-line and share their address-validation with each other and with
/// the u32 form immediately above (which is inlined for parity with the
/// v0.16 landing).
fn mmio_read_recipe_literal(
    arg_ids: &[IrNodeId],
    arena: &IrArena,
    mode: InstrMode,
    width: IntWidth,
    method: &'static str,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    if arg_ids.len() != 1 {
        return Some(Err(StdlibLoweringError::NonLiteralArg {
            arg_index: 0,
            method,
        }));
    }
    let addr_val = match arena.literal_values().get(arg_ids[0]) {
        Some(v) => v,
        None => {
            return Some(Err(StdlibLoweringError::NonLiteralArg {
                arg_index: 0,
                method,
            }));
        }
    };
    if addr_val < i32::MIN as i64 || addr_val > i32::MAX as i64 {
        return Some(Err(StdlibLoweringError::NonLiteralArg {
            arg_index: 0,
            method,
        }));
    }
    let mut operands = SmallVec::new();
    operands.push(Operand::Reg(abi::RAX));
    operands.push(Operand::MemDisp {
        disp: addr_val as i32,
    });
    Some(Ok(LoweringRecipe {
        instructions: vec![Instruction {
            mnemonic: Mnemonic::MovSized { width },
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

/// Shared helper for MmioOps::mmio_write_uN (N ∈ {8,16,64}) — Literal
/// convention with compile-time-literal address and value.
fn mmio_write_recipe_literal(
    arg_ids: &[IrNodeId],
    arena: &IrArena,
    mode: InstrMode,
    width: IntWidth,
    method: &'static str,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    if arg_ids.len() != 2 {
        return Some(Err(StdlibLoweringError::NonLiteralArg {
            arg_index: 0,
            method,
        }));
    }
    let addr_val = match arena.literal_values().get(arg_ids[0]) {
        Some(v) => v,
        None => {
            return Some(Err(StdlibLoweringError::NonLiteralArg {
                arg_index: 0,
                method,
            }));
        }
    };
    let val_val = match arena.literal_values().get(arg_ids[1]) {
        Some(v) => v,
        None => {
            return Some(Err(StdlibLoweringError::NonLiteralArg {
                arg_index: 1,
                method,
            }));
        }
    };
    if addr_val < i32::MIN as i64 || addr_val > i32::MAX as i64 {
        return Some(Err(StdlibLoweringError::NonLiteralArg {
            arg_index: 0,
            method,
        }));
    }
    let mut operands = SmallVec::new();
    operands.push(Operand::MemDisp {
        disp: addr_val as i32,
    });
    operands.push(Operand::Imm64(val_val));
    Some(Ok(LoweringRecipe {
        instructions: vec![Instruction {
            mnemonic: Mnemonic::MovSized { width },
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
