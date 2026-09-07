//! Bit-scan and population-count encoders: `popcnt`, `bsf`, `bsr`,
//! `tzcnt`, `crc32`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;


pub(super) fn encode_popcnt(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0039: popcnt only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            match width {
                IntWidth::W64 => {
                    popcnt_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let src_id = reg32_from(*src)? as u8;
                    popcnt_reg32_reg32(buf, dst_id, src_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            match width {
                IntWidth::W64 => {
                    popcnt_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let base_id = reg32_from(*base)? as u8;
                    popcnt_reg32_mem_base_disp(buf, dst_id, base_id, *disp);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Popcnt { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Popcnt { width },
        }),
    }
}

// bsf (bit scan forward) is encoded only in its 64-bit register form (PA-R16-008, #974).
pub(super) fn encode_bsf(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0050: bsf only supports W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            bsf_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            bsf_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Bsf { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bsf { width },
        }),
    }
}

// bsr (bit scan reverse) is encoded only in its 64-bit register form (PA-R16-008, #974).
pub(super) fn encode_bsr(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0051: bsr only supports W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            bsr_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            bsr_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Bsr { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bsr { width },
        }),
    }
}

// tzcnt (trailing zero count) is encoded only in its 64-bit register form (PA-R16-008, #974).
pub(super) fn encode_tzcnt(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0052: tzcnt only supports W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            tzcnt_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            tzcnt_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Tzcnt { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Tzcnt { width },
        }),
    }
}

// crc32 checksum is encoded only in its 64-bit register form (PA-R15-006, #1005).
pub(super) fn encode_crc32(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0053: crc32 only supports W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            crc32_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            crc32_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Crc32 { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Crc32 { width },
        }),
    }
}
