//! Bit-test encoders: `bt`, `bts`, `btr`, `btc`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;


pub(super) fn encode_bt(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0040: bt only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(bitmap), Operand::Reg(index)] => {
            match width {
                IntWidth::W64 => {
                    bt_reg64_reg64(buf, reg64_from(*bitmap)?, reg64_from(*index)?);
                }
                IntWidth::W32 => {
                    let bitmap_id = reg32_from(*bitmap)? as u8;
                    let index_id = reg32_from(*index)? as u8;
                    bt_reg32_reg32(buf, bitmap_id, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { base, index: None, scale: _, disp }, Operand::Reg(index_reg)] => {
            match width {
                IntWidth::W64 => {
                    bt_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*index_reg)?);
                }
                IntWidth::W32 => {
                    let base_id = reg32_from(*base)? as u8;
                    let index_id = reg32_from(*index_reg)? as u8;
                    bt_mem_base_disp_reg32(buf, base_id, *disp, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { index: Some(_), .. }, _] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Bt { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bt { width },
        }),
    }
}

pub(super) fn encode_bts(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0041: bts only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(bitmap), Operand::Reg(index)] => {
            match width {
                IntWidth::W64 => {
                    bts_reg64_reg64(buf, reg64_from(*bitmap)?, reg64_from(*index)?);
                }
                IntWidth::W32 => {
                    let bitmap_id = reg32_from(*bitmap)? as u8;
                    let index_id = reg32_from(*index)? as u8;
                    bts_reg32_reg32(buf, bitmap_id, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { base, index: None, scale: _, disp }, Operand::Reg(index_reg)] => {
            match width {
                IntWidth::W64 => {
                    bts_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*index_reg)?);
                }
                IntWidth::W32 => {
                    let base_id = reg32_from(*base)? as u8;
                    let index_id = reg32_from(*index_reg)? as u8;
                    bts_mem_base_disp_reg32(buf, base_id, *disp, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { index: Some(_), .. }, _] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Bts { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bts { width },
        }),
    }
}

pub(super) fn encode_btr(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0042: btr only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(bitmap), Operand::Reg(index)] => {
            match width {
                IntWidth::W64 => {
                    btr_reg64_reg64(buf, reg64_from(*bitmap)?, reg64_from(*index)?);
                }
                IntWidth::W32 => {
                    let bitmap_id = reg32_from(*bitmap)? as u8;
                    let index_id = reg32_from(*index)? as u8;
                    btr_reg32_reg32(buf, bitmap_id, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { base, index: None, scale: _, disp }, Operand::Reg(index_reg)] => {
            match width {
                IntWidth::W64 => {
                    btr_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*index_reg)?);
                }
                IntWidth::W32 => {
                    let base_id = reg32_from(*base)? as u8;
                    let index_id = reg32_from(*index_reg)? as u8;
                    btr_mem_base_disp_reg32(buf, base_id, *disp, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { index: Some(_), .. }, _] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Btr { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Btr { width },
        }),
    }
}

pub(super) fn encode_btc(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0043: btc only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(bitmap), Operand::Reg(index)] => {
            match width {
                IntWidth::W64 => {
                    btc_reg64_reg64(buf, reg64_from(*bitmap)?, reg64_from(*index)?);
                }
                IntWidth::W32 => {
                    let bitmap_id = reg32_from(*bitmap)? as u8;
                    let index_id = reg32_from(*index)? as u8;
                    btc_reg32_reg32(buf, bitmap_id, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { base, index: None, scale: _, disp }, Operand::Reg(index_reg)] => {
            match width {
                IntWidth::W64 => {
                    btc_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*index_reg)?);
                }
                IntWidth::W32 => {
                    let base_id = reg32_from(*base)? as u8;
                    let index_id = reg32_from(*index_reg)? as u8;
                    btc_mem_base_disp_reg32(buf, base_id, *disp, index_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { index: Some(_), .. }, _] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Btc { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Btc { width },
        }),
    }
}
