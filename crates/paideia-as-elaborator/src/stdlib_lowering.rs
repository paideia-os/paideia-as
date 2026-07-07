//! Stdlib trait method → mnemonic sequence lowering.
//!
//! PA-r16-007-backtrack (#1036): a hardcoded registry that maps
//! `(trait_name, method_name)` pairs to the IR instruction sequences
//! they should lower to. Consulted by emit_call before its normal SysV
//! call-marshalling.
//!
//! PA-r16-007-followup (#1056): PerCpuOps::percpu_inc / percpu_add lowering.
//! Extends signature to accept arg_ids and arena so recipes can extract
//! integer-literal arguments at compile time (required for absolute-displacement
//! encoding).
//!
//! PA-r16-007-registry-runtime-args (#1062): ArgConvention enum distinguishes
//! Literal recipes (args baked into operands at compile time) from SysVRegs
//! recipes (args pre-marshalled into RDI/RSI/RDX/RCX/R8/R9).
//!
//! Scope: PauseOps::spin_hint(), PerCpuOps::percpu_inc/percpu_add,
//! MmioOps::mmio_read_u32/mmio_write_u32 in v0.16.
//! Follow-up issues track BytesOps, ChecksumOps retrofits.

use paideia_as_ir::{SmallVec, IrArena, IrNodeId, instruction::{InstrMode, Instruction, Mnemonic, Operand, SegPrefix, IntWidth}, abi};

/// Error returned by lower_stdlib_method when recipe matching succeeds
/// but argument extraction fails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StdlibLoweringError {
    /// A required argument is not an integer literal.
    NonLiteralArg {
        /// 0-based index of the failing argument.
        arg_index: usize,
        /// Qualified name like "PerCpuOps::percpu_inc".
        method: &'static str,
    },
}

/// Argument-passing convention for a lowering recipe.
///
/// PA-r16-007 (#1062): distinguishes recipes that bake args into their
/// operands at compile time (Literal) from those that expect args
/// pre-marshalled into SysV argument registers (SysVRegs).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ArgConvention {
    /// Recipe uses arg values as literals baked into instruction operands.
    /// emit_call skips SysV arg-marshalling entirely.
    Literal,
    /// Recipe references SysV argument registers (RDI, RSI, RDX, RCX, R8, R9)
    /// which emit_call must populate via normal SysV arg-marshalling BEFORE
    /// splicing the recipe.
    SysVRegs,
}

/// A lowering recipe: the instructions to splice + how args reach them.
#[derive(Debug, Clone)]
pub struct LoweringRecipe {
    /// Instructions to splice in place of the function call.
    pub instructions: Vec<Instruction>,
    /// Argument-passing convention: whether args are baked into operands (Literal)
    /// or pre-marshalled into SysV registers (SysVRegs).
    pub arg_convention: ArgConvention,
}

/// Look up the lowering recipe for `(trait_name, method_name)`.
/// Returns:
/// - `None` if the pair is not a known stdlib trait method, signalling
///   emit_call should fall through to normal call emission.
/// - `Some(Ok(recipe))` if the method matched and args were successfully
///   extracted (for Literal recipes) or matched (for SysVRegs recipes).
///   Recipe is spliced in place of the call, with arg_convention determining
///   whether arg-marshalling occurs before splicing.
/// - `Some(Err(NonLiteralArg))` if the method matched but at least one arg
///   is not an integer literal (only for Literal-convention recipes).
///   Caller should emit diagnostic and skip lowering.
///
/// The returned LoweringRecipe (on Ok) indicates both the instructions to splice
/// and whether they expect pre-marshalled SysV registers (SysVRegs) or have args
/// baked into operands (Literal). No `call target` is ever emitted; `ret` behavior
/// depends on the caller's function structure.
#[must_use]
pub fn lower_stdlib_method(
    trait_name: &str,
    method_name: &str,
    mode: InstrMode,
    arg_ids: &[IrNodeId],
    arena: &IrArena,
) -> Option<Result<LoweringRecipe, StdlibLoweringError>> {
    match (trait_name, method_name) {
        ("PauseOps", "spin_hint") => {
            // PauseOps::spin_hint takes no arguments, always succeeds.
            Some(Ok(LoweringRecipe {
                instructions: vec![Instruction {
                    mnemonic: Mnemonic::Pause,
                    operands: SmallVec::new(),
                    encoding_hint: None,
                    byte_offset_in_text: None,
                    mode,
                }],
                arg_convention: ArgConvention::Literal,
            }))
        }
        ("PerCpuOps", "percpu_inc") => {
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
                }],
                arg_convention: ArgConvention::Literal,
            }))
        }
        ("PerCpuOps", "percpu_add") => {
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
                }],
                arg_convention: ArgConvention::Literal,
            }))
        }
        ("MmioOps", "mmio_read_u32") => {
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
                }],
                arg_convention: ArgConvention::Literal,
            }))
        }
        ("MmioOps", "mmio_write_u32") => {
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
                }],
                arg_convention: ArgConvention::Literal,
            }))
        }
        // BytesOps typed accessors: SysVRegs convention
        // All getters/setters use buf→RDI, off→RSI, val→RDX (setters only)
        ("BytesOps", "get_u8") => {
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
                }],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "get_u16_le") => {
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
                }],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "get_u32_le") => {
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
                }],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "get_u32_be") => {
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
                    },
                    Instruction {
                        mnemonic: Mnemonic::Bswap32,
                        operands: bswap_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "get_u64_le") => {
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
                }],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "get_u64_be") => {
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
                    },
                    Instruction {
                        mnemonic: Mnemonic::Bswap,
                        operands: bswap_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "put_u8") => {
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
                    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W8,
                        },
                        operands: store_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "put_u16_le") => {
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
                    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W16,
                        },
                        operands: store_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "put_u32_le") => {
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
                    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W32,
                        },
                        operands: store_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "put_u32_be") => {
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
                    },
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W32,
                        },
                        operands: store_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "put_u64_le") => {
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
                    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W64,
                        },
                        operands: store_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        ("BytesOps", "put_u64_be") => {
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
                    },
                    Instruction {
                        mnemonic: Mnemonic::Add,
                        operands: add_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                    },
                    Instruction {
                        mnemonic: Mnemonic::MovSized {
                            width: IntWidth::W64,
                        },
                        operands: store_ops,
                        encoding_hint: None,
                        byte_offset_in_text: None,
                        mode,
                    },
                ],
                arg_convention: ArgConvention::SysVRegs,
            }))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{IrArena, IrNodeId};

    #[test]
    fn pause_ops_spin_hint_returns_pause_mnemonic() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("PauseOps", "spin_hint", InstrMode::Mode64, &[], &arena)
            .expect("pause recipe should exist")
            .expect("pause lowering should succeed");
        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Pause);
        assert!(recipe.instructions[0].operands.is_empty());
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);
    }

    #[test]
    fn unknown_trait_returns_none() {
        let arena = IrArena::new();
        assert!(lower_stdlib_method("UnknownTrait", "some_method", InstrMode::Mode64, &[], &arena).is_none());
    }

    #[test]
    fn known_trait_unknown_method_returns_none() {
        let arena = IrArena::new();
        assert!(lower_stdlib_method("PauseOps", "nonexistent", InstrMode::Mode64, &[], &arena).is_none());
    }

    #[test]
    fn percpu_inc_lowers_to_gs_lock_inc() {
        let mut arena = IrArena::new();
        let lit_id = IrNodeId::new(1).expect("valid node id");
        arena.literal_values_mut().insert(lit_id, 0x1000);

        let recipe = lower_stdlib_method(
            "PerCpuOps",
            "percpu_inc",
            InstrMode::Mode64,
            &[lit_id],
            &arena,
        )
        .expect("recipe should exist")
        .expect("lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::LockInc {
                width: paideia_as_ir::instruction::IntWidth::W64
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 1);
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);

        // Verify the operand is MemSeg { Gs, MemDisp { 0x1000 } }
        match &recipe.instructions[0].operands[0] {
            Operand::MemSeg { seg, inner } => {
                assert_eq!(*seg, SegPrefix::Gs);
                match inner.as_ref() {
                    Operand::MemDisp { disp } => {
                        assert_eq!(*disp, 0x1000);
                    }
                    _ => panic!("expected MemDisp inner operand"),
                }
            }
            _ => panic!("expected MemSeg operand"),
        }
    }

    #[test]
    fn percpu_add_lowers_to_gs_lock_add() {
        let mut arena = IrArena::new();
        let disp_id = IrNodeId::new(1).expect("valid node id");
        let val_id = IrNodeId::new(2).expect("valid node id");
        arena.literal_values_mut().insert(disp_id, 0x2000);
        arena.literal_values_mut().insert(val_id, 5);

        let recipe = lower_stdlib_method(
            "PerCpuOps",
            "percpu_add",
            InstrMode::Mode64,
            &[disp_id, val_id],
            &arena,
        )
        .expect("recipe should exist")
        .expect("lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::LockAdd {
                width: paideia_as_ir::instruction::IntWidth::W64
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);

        // Verify the first operand is MemSeg { Gs, MemDisp { 0x2000 } }
        match &recipe.instructions[0].operands[0] {
            Operand::MemSeg { seg, inner } => {
                assert_eq!(*seg, SegPrefix::Gs);
                match inner.as_ref() {
                    Operand::MemDisp { disp } => {
                        assert_eq!(*disp, 0x2000);
                    }
                    _ => panic!("expected MemDisp inner operand"),
                }
            }
            _ => panic!("expected MemSeg operand"),
        }

        // Verify the second operand is Imm64(5)
        match &recipe.instructions[0].operands[1] {
            Operand::Imm64(val) => {
                assert_eq!(*val, 5);
            }
            _ => panic!("expected Imm64 operand"),
        }
    }

    #[test]
    fn percpu_inc_non_literal_returns_err() {
        let arena = IrArena::new();
        // Pass an arg_id that's not in the literal_values table
        let missing_id = IrNodeId::new(999).expect("valid node id");

        let result = lower_stdlib_method(
            "PerCpuOps",
            "percpu_inc",
            InstrMode::Mode64,
            &[missing_id],
            &arena,
        )
        .expect("recipe should exist");

        match result {
            Err(StdlibLoweringError::NonLiteralArg { arg_index, method }) => {
                assert_eq!(arg_index, 0);
                assert_eq!(method, "PerCpuOps::percpu_inc");
            }
            Ok(_) => panic!("expected error for non-literal arg"),
        }
    }

    #[test]
    fn percpu_add_non_literal_arg1_returns_err() {
        let mut arena = IrArena::new();
        let disp_id = IrNodeId::new(1).expect("valid node id");
        let missing_id = IrNodeId::new(999).expect("valid node id");
        arena.literal_values_mut().insert(disp_id, 0x2000);

        let result = lower_stdlib_method(
            "PerCpuOps",
            "percpu_add",
            InstrMode::Mode64,
            &[disp_id, missing_id],
            &arena,
        )
        .expect("recipe should exist");

        match result {
            Err(StdlibLoweringError::NonLiteralArg { arg_index, method }) => {
                assert_eq!(arg_index, 1);
                assert_eq!(method, "PerCpuOps::percpu_add");
            }
            Ok(_) => panic!("expected error for non-literal arg"),
        }
    }

    #[test]
    fn mmio_ops_mmio_read_u32_lowers_to_mov_eax_mem_disp32() {
        let mut arena = IrArena::new();
        let addr_id = IrNodeId::new(1).expect("valid node id");
        arena.literal_values_mut().insert(addr_id, 0x1000);

        let recipe = lower_stdlib_method(
            "MmioOps",
            "mmio_read_u32",
            InstrMode::Mode64,
            &[addr_id],
            &arena,
        )
        .expect("recipe should exist")
        .expect("lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);

        // Verify first operand is Reg(RAX)
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => {
                assert_eq!(*reg, abi::RAX);
            }
            _ => panic!("expected Reg(RAX) operand"),
        }

        // Verify second operand is MemDisp { 0x1000 }
        match &recipe.instructions[0].operands[1] {
            Operand::MemDisp { disp } => {
                assert_eq!(*disp, 0x1000);
            }
            _ => panic!("expected MemDisp operand"),
        }
    }

    #[test]
    fn mmio_ops_mmio_write_u32_lowers_to_mov_mem_disp32_imm32() {
        let mut arena = IrArena::new();
        let addr_id = IrNodeId::new(1).expect("valid node id");
        let val_id = IrNodeId::new(2).expect("valid node id");
        arena.literal_values_mut().insert(addr_id, 0x1000);
        arena.literal_values_mut().insert(val_id, 0x12345678);

        let recipe = lower_stdlib_method(
            "MmioOps",
            "mmio_write_u32",
            InstrMode::Mode64,
            &[addr_id, val_id],
            &arena,
        )
        .expect("recipe should exist")
        .expect("lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::Literal);

        // Verify first operand is MemDisp { 0x1000 }
        match &recipe.instructions[0].operands[0] {
            Operand::MemDisp { disp } => {
                assert_eq!(*disp, 0x1000);
            }
            _ => panic!("expected MemDisp operand"),
        }

        // Verify second operand is Imm64(0x12345678)
        match &recipe.instructions[0].operands[1] {
            Operand::Imm64(val) => {
                assert_eq!(*val, 0x12345678);
            }
            _ => panic!("expected Imm64 operand"),
        }
    }

    #[test]
    fn mmio_ops_mmio_read_u32_non_literal_addr_returns_err() {
        let arena = IrArena::new();
        let missing_id = IrNodeId::new(999).expect("valid node id");

        let result = lower_stdlib_method(
            "MmioOps",
            "mmio_read_u32",
            InstrMode::Mode64,
            &[missing_id],
            &arena,
        )
        .expect("recipe should exist");

        match result {
            Err(StdlibLoweringError::NonLiteralArg { arg_index, method }) => {
                assert_eq!(arg_index, 0);
                assert_eq!(method, "MmioOps::mmio_read_u32");
            }
            Ok(_) => panic!("expected error for non-literal arg"),
        }
    }

    #[test]
    fn mmio_ops_mmio_write_u32_non_literal_val_returns_err() {
        let mut arena = IrArena::new();
        let addr_id = IrNodeId::new(1).expect("valid node id");
        let missing_val_id = IrNodeId::new(999).expect("valid node id");
        arena.literal_values_mut().insert(addr_id, 0x1000);

        let result = lower_stdlib_method(
            "MmioOps",
            "mmio_write_u32",
            InstrMode::Mode64,
            &[addr_id, missing_val_id],
            &arena,
        )
        .expect("recipe should exist");

        match result {
            Err(StdlibLoweringError::NonLiteralArg { arg_index, method }) => {
                assert_eq!(arg_index, 1);
                assert_eq!(method, "MmioOps::mmio_write_u32");
            }
            Ok(_) => panic!("expected error for non-literal val"),
        }
    }

    #[test]
    fn test_sysvregs_recipe_with_literal_synthesis() {
        // PA-r16-007 (#1062): synthetic SysVRegs recipe for testing.
        // This test verifies that a recipe can declare SysVRegs arg convention
        // and include instructions that reference SysV argument registers.

        // Manually construct a SysVRegs recipe (simulating what a future
        // SysVRegs-convention recipe would produce).
        let mut operands = SmallVec::new();
        operands.push(Operand::Reg(abi::RAX));
        operands.push(Operand::Reg(abi::RDI));  // Read SysV arg 0

        let recipe = LoweringRecipe {
            instructions: vec![Instruction {
                mnemonic: Mnemonic::Mov,
                operands,
                encoding_hint: None,
                byte_offset_in_text: None,
                mode: InstrMode::Mode64,
            }],
            arg_convention: ArgConvention::SysVRegs,
        };

        // Verify structure
        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Mov);
        assert_eq!(recipe.instructions[0].operands.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // Verify operands: RAX (dest), RDI (SysV arg0)
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RAX),
            _ => panic!("expected Reg(RAX)"),
        }
        match &recipe.instructions[0].operands[1] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RDI),
            _ => panic!("expected Reg(RDI)"),
        }
    }

    // BytesOps getter and setter recipe tests (#1063)

    #[test]
    fn bytes_ops_get_u8_lowers_to_movsized_w8() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u8", InstrMode::Mode64, &[], &arena)
            .expect("get_u8 recipe should exist")
            .expect("get_u8 lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W8
            }
        );
        assert_eq!(recipe.instructions[0].operands.len(), 2);

        // Verify operand 0: Reg(RAX)
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RAX),
            _ => panic!("expected Reg(RAX)"),
        }

        // Verify operand 1: MemSib{RDI, Some(RSI), X1, 0}
        match &recipe.instructions[0].operands[1] {
            Operand::MemSib { base, index, scale, disp } => {
                assert_eq!(*base, abi::RDI);
                assert_eq!(*index, Some(abi::RSI));
                assert_eq!(*scale, paideia_as_ir::instruction::Scale::X1);
                assert_eq!(*disp, 0);
            }
            _ => panic!("expected MemSib operand"),
        }
    }

    #[test]
    fn bytes_ops_get_u16_le_lowers_to_movsized_w16() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u16_le", InstrMode::Mode64, &[], &arena)
            .expect("get_u16_le recipe should exist")
            .expect("get_u16_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W16
            }
        );
    }

    #[test]
    fn bytes_ops_get_u32_le_lowers_to_movsized_w32() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u32_le", InstrMode::Mode64, &[], &arena)
            .expect("get_u32_le recipe should exist")
            .expect("get_u32_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );
    }

    #[test]
    fn bytes_ops_get_u32_be_lowers_to_movsized_w32_plus_bswap32() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u32_be", InstrMode::Mode64, &[], &arena)
            .expect("get_u32_be recipe should exist")
            .expect("get_u32_be lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: MovSized W32
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );

        // Second instruction: Bswap32
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Bswap32);
        assert_eq!(recipe.instructions[1].operands.len(), 1);
        match &recipe.instructions[1].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RAX),
            _ => panic!("expected Reg(RAX)"),
        }
    }

    #[test]
    fn bytes_ops_get_u64_le_lowers_to_movsized_w64() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u64_le", InstrMode::Mode64, &[], &arena)
            .expect("get_u64_le recipe should exist")
            .expect("get_u64_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 1);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W64
            }
        );
    }

    #[test]
    fn bytes_ops_get_u64_be_lowers_to_movsized_w64_plus_bswap() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "get_u64_be", InstrMode::Mode64, &[], &arena)
            .expect("get_u64_be recipe should exist")
            .expect("get_u64_be lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: MovSized W64
        assert_eq!(
            recipe.instructions[0].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W64
            }
        );

        // Second instruction: Bswap
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Bswap);
        assert_eq!(recipe.instructions[1].operands.len(), 1);
        match &recipe.instructions[1].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RAX),
            _ => panic!("expected Reg(RAX)"),
        }
    }

    #[test]
    fn bytes_ops_put_u8_lowers_to_add_plus_movsized_w8() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u8", InstrMode::Mode64, &[], &arena)
            .expect("put_u8 recipe should exist")
            .expect("put_u8 lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: Add RDI, RSI
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Add);
        assert_eq!(recipe.instructions[0].operands.len(), 2);

        // Second instruction: MovSized W8
        assert_eq!(
            recipe.instructions[1].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W8
            }
        );
        assert_eq!(recipe.instructions[1].operands.len(), 2);
    }

    #[test]
    fn bytes_ops_put_u16_le_lowers_to_add_plus_movsized_w16() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u16_le", InstrMode::Mode64, &[], &arena)
            .expect("put_u16_le recipe should exist")
            .expect("put_u16_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Add);
        assert_eq!(
            recipe.instructions[1].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W16
            }
        );
    }

    #[test]
    fn bytes_ops_put_u32_le_lowers_to_add_plus_movsized_w32() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u32_le", InstrMode::Mode64, &[], &arena)
            .expect("put_u32_le recipe should exist")
            .expect("put_u32_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Add);
        assert_eq!(
            recipe.instructions[1].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );
    }

    #[test]
    fn bytes_ops_put_u32_be_lowers_to_bswap32_plus_add_plus_movsized_w32() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u32_be", InstrMode::Mode64, &[], &arena)
            .expect("put_u32_be recipe should exist")
            .expect("put_u32_be lowering should succeed");

        assert_eq!(recipe.instructions.len(), 3);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: Bswap32 RDX
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Bswap32);
        assert_eq!(recipe.instructions[0].operands.len(), 1);
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RDX),
            _ => panic!("expected Reg(RDX)"),
        }

        // Second instruction: Add RDI, RSI
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Add);

        // Third instruction: MovSized W32
        assert_eq!(
            recipe.instructions[2].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W32
            }
        );
    }

    #[test]
    fn bytes_ops_put_u64_le_lowers_to_add_plus_movsized_w64() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u64_le", InstrMode::Mode64, &[], &arena)
            .expect("put_u64_le recipe should exist")
            .expect("put_u64_le lowering should succeed");

        assert_eq!(recipe.instructions.len(), 2);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Add);
        assert_eq!(
            recipe.instructions[1].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W64
            }
        );
    }

    #[test]
    fn bytes_ops_put_u64_be_lowers_to_bswap_plus_add_plus_movsized_w64() {
        let arena = IrArena::new();
        let recipe = lower_stdlib_method("BytesOps", "put_u64_be", InstrMode::Mode64, &[], &arena)
            .expect("put_u64_be recipe should exist")
            .expect("put_u64_be lowering should succeed");

        assert_eq!(recipe.instructions.len(), 3);
        assert_eq!(recipe.arg_convention, ArgConvention::SysVRegs);

        // First instruction: Bswap RDX
        assert_eq!(recipe.instructions[0].mnemonic, Mnemonic::Bswap);
        assert_eq!(recipe.instructions[0].operands.len(), 1);
        match &recipe.instructions[0].operands[0] {
            Operand::Reg(reg) => assert_eq!(*reg, abi::RDX),
            _ => panic!("expected Reg(RDX)"),
        }

        // Second instruction: Add RDI, RSI
        assert_eq!(recipe.instructions[1].mnemonic, Mnemonic::Add);

        // Third instruction: MovSized W64
        assert_eq!(
            recipe.instructions[2].mnemonic,
            Mnemonic::MovSized {
                width: IntWidth::W64
            }
        );
    }
}
