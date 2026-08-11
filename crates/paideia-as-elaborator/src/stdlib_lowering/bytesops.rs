//! `BytesOps` stdlib-lowering recipes.
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

/// Dispatch a `BytesOps::<method_name>` call to its lowering recipe.
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
        // BytesOps typed accessors: SysVRegs convention
        // All getters/setters use buf→RDI, off→RSI, val→RDX (setters only)
        "get_u8" => {
            // get_u8(buf, off) -> u8: MovSized{W8} [RAX], MemSib{RDI, Some(RSI), X1, 0}
            let mut operands = SmallVec::new();
            operands.push(Operand::Reg(abi::RAX));
            operands.push(Operand::MemSib {
                base: abi::RDI,
                index: Some(abi::RSI),
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::MovSized {
                        width: IntWidth::W8,
                    },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
    }],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "get_u16_le" => {
            // get_u16_le(buf, off) -> u16: MovSized{W16} [RAX], MemSib{RDI, Some(RSI), X1, 0}
            let mut operands = SmallVec::new();
            operands.push(Operand::Reg(abi::RAX));
            operands.push(Operand::MemSib {
                base: abi::RDI,
                index: Some(abi::RSI),
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::MovSized {
                        width: IntWidth::W16,
                    },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
    }],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "get_u16_be" => {
            // get_u16_be(buf, off) -> u16: MovSized{W16} [RAX], MemSib + Rol{W16} [RAX, 8]
            let mut load_ops = SmallVec::new();
            load_ops.push(Operand::Reg(abi::RAX));
            load_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: Some(abi::RSI),
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });

            let mut rol_ops = SmallVec::new();
            rol_ops.push(Operand::Reg(abi::RAX));
            rol_ops.push(Operand::Imm64(8));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W16,
                        },
                        operands: load_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::Rol {
                            width: IntWidth::W16,
                        },
                        operands: rol_ops,
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
        "get_u32_le" => {
            // get_u32_le(buf, off) -> u32: MovSized{W32} [RAX], MemSib{RDI, Some(RSI), X1, 0}
            let mut operands = SmallVec::new();
            operands.push(Operand::Reg(abi::RAX));
            operands.push(Operand::MemSib {
                base: abi::RDI,
                index: Some(abi::RSI),
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
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
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "get_u32_be" => {
            // get_u32_be(buf, off) -> u32: MovSized{W32} [RAX], MemSib + Bswap32 [RAX]
            let mut load_ops = SmallVec::new();
            load_ops.push(Operand::Reg(abi::RAX));
            load_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: Some(abi::RSI),
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });

            let mut bswap_ops = SmallVec::new();
            bswap_ops.push(Operand::Reg(abi::RAX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W32,
                        },
                        operands: load_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::Bswap32,
                        operands: bswap_ops,
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
        "get_u64_le" => {
            // get_u64_le(buf, off) -> u64: MovSized{W64} [RAX], MemSib{RDI, Some(RSI), X1, 0}
            let mut operands = SmallVec::new();
            operands.push(Operand::Reg(abi::RAX));
            operands.push(Operand::MemSib {
                base: abi::RDI,
                index: Some(abi::RSI),
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::MovSized {
                        width: IntWidth::W64,
                    },
                    operands,
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                    emission_order: 0,
    }],
                arg_convention: ArgConvention::SysVRegs,
                labels: vec![],
            }))
        }
        "get_u64_be" => {
            // get_u64_be(buf, off) -> u64: MovSized{W64} [RAX], MemSib + Bswap [RAX]
            let mut load_ops = SmallVec::new();
            load_ops.push(Operand::Reg(abi::RAX));
            load_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: Some(abi::RSI),
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });

            let mut bswap_ops = SmallVec::new();
            bswap_ops.push(Operand::Reg(abi::RAX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W64,
                        },
                        operands: load_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::Bswap,
                        operands: bswap_ops,
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
        "put_u8" => {
            // put_u8(buf, off, val) -> (): Add RDI, RSI + MovSized{W8} [RDI+0], DX
            let mut add_ops = SmallVec::new();
            add_ops.push(Operand::Reg(abi::RDI));
            add_ops.push(Operand::Reg(abi::RSI));

            let mut store_ops = SmallVec::new();
            store_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            store_ops.push(Operand::Reg(abi::RDX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W8,
                        },
                        operands: store_ops,
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
        "put_u16_le" => {
            // put_u16_le(buf, off, val) -> (): Add RDI, RSI + MovSized{W16} [RDI+0], DX
            let mut add_ops = SmallVec::new();
            add_ops.push(Operand::Reg(abi::RDI));
            add_ops.push(Operand::Reg(abi::RSI));

            let mut store_ops = SmallVec::new();
            store_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            store_ops.push(Operand::Reg(abi::RDX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W16,
                        },
                        operands: store_ops,
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
        "put_u16_be" => {
            // put_u16_be(buf, off, val) -> (): Rol{W16} DX, 8 + Add RDI, RSI + MovSized{W16} [RDI+0], DX
            let mut rol_ops = SmallVec::new();
            rol_ops.push(Operand::Reg(abi::RDX));
            rol_ops.push(Operand::Imm64(8));

            let mut add_ops = SmallVec::new();
            add_ops.push(Operand::Reg(abi::RDI));
            add_ops.push(Operand::Reg(abi::RSI));

            let mut store_ops = SmallVec::new();
            store_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            store_ops.push(Operand::Reg(abi::RDX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Rol {
                            width: IntWidth::W16,
                        },
                        operands: rol_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W16,
                        },
                        operands: store_ops,
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
        "put_u32_le" => {
            // put_u32_le(buf, off, val) -> (): Add RDI, RSI + MovSized{W32} [RDI+0], EDX
            let mut add_ops = SmallVec::new();
            add_ops.push(Operand::Reg(abi::RDI));
            add_ops.push(Operand::Reg(abi::RSI));

            let mut store_ops = SmallVec::new();
            store_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            store_ops.push(Operand::Reg(abi::RDX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W32,
                        },
                        operands: store_ops,
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
        "put_u32_be" => {
            // put_u32_be(buf, off, val) -> (): Bswap32 RDX + Add RDI, RSI + MovSized{W32} [RDI+0], EDX
            let mut bswap_ops = SmallVec::new();
            bswap_ops.push(Operand::Reg(abi::RDX));

            let mut add_ops = SmallVec::new();
            add_ops.push(Operand::Reg(abi::RDI));
            add_ops.push(Operand::Reg(abi::RSI));

            let mut store_ops = SmallVec::new();
            store_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            store_ops.push(Operand::Reg(abi::RDX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Bswap32,
                        operands: bswap_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W32,
                        },
                        operands: store_ops,
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
        "put_u64_le" => {
            // put_u64_le(buf, off, val) -> (): Add RDI, RSI + MovSized{W64} [RDI+0], RDX
            let mut add_ops = SmallVec::new();
            add_ops.push(Operand::Reg(abi::RDI));
            add_ops.push(Operand::Reg(abi::RSI));

            let mut store_ops = SmallVec::new();
            store_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            store_ops.push(Operand::Reg(abi::RDX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W64,
                        },
                        operands: store_ops,
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
        "put_u64_be" => {
            // put_u64_be(buf, off, val) -> (): Bswap RDX + Add RDI, RSI + MovSized{W64} [RDI+0], RDX
            let mut bswap_ops = SmallVec::new();
            bswap_ops.push(Operand::Reg(abi::RDX));

            let mut add_ops = SmallVec::new();
            add_ops.push(Operand::Reg(abi::RDI));
            add_ops.push(Operand::Reg(abi::RSI));

            let mut store_ops = SmallVec::new();
            store_ops.push(Operand::MemSib {
                base: abi::RDI,
                index: None,
                scale: paideia_as_ir::instruction::Scale::X1,
                disp: 0,
            });
            store_ops.push(Operand::Reg(abi::RDX));

            Some(Ok(LoweringRecipe {
                instructions: vec![
                    Instruction {
                        mnemonic: Mnemonic::Bswap,
                        operands: bswap_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                        emission_order: 0,
    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W64,
                        },
                        operands: store_ops,
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
