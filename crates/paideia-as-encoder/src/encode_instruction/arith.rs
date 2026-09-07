//! Two-operand arithmetic encoders: `add`, `sub`, `adc`, `sbb`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;


pub(super) fn encode_add(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // add r64, r64 → 48 01 <ModR/M>
            add_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        // Issue #1328: add r64, [base + disp] — load-direction ADD from memory.
        // Complements add r64, r64 by accepting a memory source operand, so callers
        // no longer need to r-to-r restage via a scratch register (workaround
        // pattern used in paideia-os sys_getdents.pdx pre-fix).
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            add_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        // Issue #1328: add r64, [base + index*scale + disp] — SIB-indexed memory source.
        [Operand::Reg(dest), Operand::MemSib { base, index: Some(index), scale, disp }] => {
            let scale_bits = sib_scale_bits(*scale);
            add_reg64_mem_sib_disp(
                buf,
                reg64_from(*dest)?,
                reg64_from(*base)?,
                reg64_from(*index)?,
                scale_bits,
                *disp,
            );
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let dest_reg = reg64_from(*dest)?;
            let imm_i64 = *imm;

            // Consult can_shorten_add_to_32bit: if the high 32 bits are zero/unused,
            // use 32-bit immediate form instead of 64-bit
            if can_shorten_add_to_32bit(false)
                && imm_i64 >= i32::MIN as i64
                && imm_i64 <= i32::MAX as i64
            {
                // High bits are not used and value fits in i32: use 32-bit form
                let imm_i32 = imm_i64 as i32;

                // Further tighten: if imm fits in i8, use 8-bit form for even shorter encoding
                if (-128..=127).contains(&imm_i32) {
                    add_reg64_imm8(buf, dest_reg, imm_i32 as i8);
                    stats.record_tightening();
                } else {
                    add_reg64_imm32(buf, dest_reg, imm_i32);
                    stats.record_tightening();
                }
            } else {
                // Value requires full 64-bit immediate: use mov + add pattern
                // For now, return unsupported as phase-3-m2-002 doesn't have this
                return Err(EncodeError::Unsupported(
                    "64-bit immediate add not yet supported",
                ));
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "add form not supported: expected reg64,reg64, reg64,[mem], or reg64,imm64",
        )),
    }
}

pub(super) fn encode_sub(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // sub r64, r64 → 48 29 <ModR/M>
            sub_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        // Issue #1328: sub r64, [base + disp] — load-direction SUB from memory.
        // Complements sub r64, r64 by accepting a memory source operand, so callers
        // no longer need to r-to-r restage via a scratch register (workaround
        // pattern used in paideia-os tmpfs/vops.pdx pre-fix).
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            sub_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        // Issue #1328: sub r64, [base + index*scale + disp] — SIB-indexed memory source.
        [Operand::Reg(dest), Operand::MemSib { base, index: Some(index), scale, disp }] => {
            let scale_bits = sib_scale_bits(*scale);
            sub_reg64_mem_sib_disp(
                buf,
                reg64_from(*dest)?,
                reg64_from(*base)?,
                reg64_from(*index)?,
                scale_bits,
                *disp,
            );
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let dest_reg = reg64_from(*dest)?;
            let imm_i64 = *imm;

            if can_shorten_add_to_32bit(false)
                && imm_i64 >= i32::MIN as i64
                && imm_i64 <= i32::MAX as i64
            {
                let imm_i32 = imm_i64 as i32;
                if (-128..=127).contains(&imm_i32) {
                    sub_reg64_imm8(buf, dest_reg, imm_i32 as i8);
                    stats.record_tightening();
                } else {
                    sub_reg64_imm32(buf, dest_reg, imm_i32);
                    stats.record_tightening();
                }
            } else {
                return Err(EncodeError::Unsupported(
                    "64-bit immediate sub not yet supported",
                ));
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "sub form not supported: expected reg64,reg64, reg64,[mem], or reg64,imm64",
        )),
    }
}


pub(super) fn encode_adc(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0037: adc only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            match width {
                IntWidth::W64 => {
                    adc_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let src_id = reg32_from(*src)? as u8;
                    adc_reg32_reg32(buf, dst_id, src_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            match width {
                IntWidth::W64 => {
                    adc_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let base_id = reg32_from(*base)? as u8;
                    adc_reg32_mem_base_disp(buf, dst_id, base_id, *disp);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Adc { width },
            })
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let imm_i64 = *imm;

            // Check if the immediate fits in i32
            if imm_i64 < i32::MIN as i64 || imm_i64 > i32::MAX as i64 {
                return Err(EncodeError::Unsupported(
                    "adc: immediate does not fit in i32",
                ));
            }

            let imm_i32 = imm_i64 as i32;

            // Choose between imm8 and imm32 forms
            if (-128..=127).contains(&imm_i32) {
                // Immediate fits in i8; use shorter encoding
                match width {
                    IntWidth::W64 => {
                        adc_reg64_imm8(buf, reg64_from(*dest)?, imm_i32 as i8);
                    }
                    IntWidth::W32 => {
                        let dst_id = reg32_from(*dest)? as u8;
                        adc_reg32_imm8(buf, dst_id, imm_i32 as i8);
                    }
                    _ => unreachable!(),
                }
            } else {
                // Immediate requires full i32 form
                match width {
                    IntWidth::W64 => {
                        adc_reg64_imm32(buf, reg64_from(*dest)?, imm_i32);
                    }
                    IntWidth::W32 => {
                        let dst_id = reg32_from(*dest)? as u8;
                        adc_reg32_imm32(buf, dst_id, imm_i32);
                    }
                    _ => unreachable!(),
                }
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Adc { width },
        }),
    }
}

pub(super) fn encode_sbb(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W32 | IntWidth::W64 => {}
        _ => {
            return Err(EncodeError::Unsupported(
                "E0038: sbb only supports W32 and W64",
            ))
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            match width {
                IntWidth::W64 => {
                    sbb_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let src_id = reg32_from(*src)? as u8;
                    sbb_reg32_reg32(buf, dst_id, src_id);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::MemSib { base, index: None, scale: _, disp }] => {
            match width {
                IntWidth::W64 => {
                    sbb_reg64_mem_base_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
                }
                IntWidth::W32 => {
                    let dst_id = reg32_from(*dest)? as u8;
                    let base_id = reg32_from(*base)? as u8;
                    sbb_reg32_mem_base_disp(buf, dst_id, base_id, *disp);
                }
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(_), Operand::MemSib { index: Some(_), .. }] => {
            Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Sbb { width },
            })
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Sbb { width },
        }),
    }
}
