//! Shift and rotate encoders: `shl`/`shr`/`sar`, `rol`/`ror`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

/// Phase 8 m1-001d: Encode shift-left instruction.
/// Supports: shl r64, imm8 or shl r64, rcx (via r64 operand)
pub(super) fn encode_shl(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            // shl r64, imm8 → 48 C1 E0 NN
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xC1);
            buf.bytes.push(0xE0 | (dst_id & 7));
            buf.bytes.push(*imm as u8);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dst), Operand::Reg(src)] => {
            // shl r64, rcx (variable shift count, src must be RCX)
            if reg64_from(*src)? != Reg64::Rcx {
                return Err(EncodeError::Unsupported(
                    "shl with variable count requires CL register",
                ));
            }
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xD3);
            buf.bytes.push(0xE0 | (dst_id & 7));
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "shl form not supported: expected reg64,imm8 or reg64,cl",
        )),
    }
}

/// Phase 8 m1-001d: Encode shift-right (logical) instruction.
pub(super) fn encode_shr(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            // shr r64, imm8 → 48 C1 E8 NN
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xC1);
            buf.bytes.push(0xE8 | (dst_id & 7));
            buf.bytes.push(*imm as u8);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dst), Operand::Reg(src)] => {
            // shr r64, rcx (variable shift count)
            if reg64_from(*src)? != Reg64::Rcx {
                return Err(EncodeError::Unsupported(
                    "shr with variable count requires CL register",
                ));
            }
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xD3);
            buf.bytes.push(0xE8 | (dst_id & 7));
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "shr form not supported: expected reg64,imm8 or reg64,cl",
        )),
    }
}

/// Phase 8 m1-001d: Encode arithmetic shift-right instruction.
pub(super) fn encode_sar(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            // sar r64, imm8 → 48 C1 F8 NN
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xC1);
            buf.bytes.push(0xF8 | (dst_id & 7));
            buf.bytes.push(*imm as u8);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dst), Operand::Reg(src)] => {
            // sar r64, rcx (variable shift count)
            if reg64_from(*src)? != Reg64::Rcx {
                return Err(EncodeError::Unsupported(
                    "sar with variable count requires CL register",
                ));
            }
            let dst_id = reg64_from(*dst)? as u8;
            let rex_byte = rex(true, false, false, (dst_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0xD3);
            buf.bytes.push(0xF8 | (dst_id & 7));
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "sar form not supported: expected reg64,imm8 or reg64,cl",
        )),
    }
}

/// Phase R15 PA-R15-004: Encode rotate-left instruction.
pub(super) fn encode_rol(inst: &Instruction, buf: &mut CodeBuffer, width: IntWidth) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {
            match inst.operands.as_slice() {
                [Operand::Reg(dst), Operand::Imm64(imm)] => {
                    // rol r64, imm8
                    let dst_reg = reg64_from(*dst)?;
                    let imm_i8 = i8::try_from(*imm)
                        .map_err(|_| EncodeError::Unsupported("E0035: rol imm must fit in i8"))?;
                    rol_reg64_imm8(buf, dst_reg, imm_i8 as u8);
                    Ok(EncodeOutput::new())
                }
                [Operand::Reg(dst), Operand::Reg(src)] => {
                    // rol r64, cl (rotate count in CL)
                    let dst_reg = reg64_from(*dst)?;
                    if reg64_from(*src)? != Reg64::Rcx {
                        return Err(EncodeError::Unsupported("E0036: rol variable count requires CL"));
                    }
                    rol_reg64_cl(buf, dst_reg);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Rol { width },
                }),
            }
        }
        IntWidth::W32 => {
            match inst.operands.as_slice() {
                [Operand::Reg(dst), Operand::Imm64(imm)] => {
                    // rol r32, imm8
                    let _dst_reg = reg32_from(*dst)?;
                    let dst_id = dst.0;
                    let imm_i8 = i8::try_from(*imm)
                        .map_err(|_| EncodeError::Unsupported("E0035: rol imm must fit in i8"))?;
                    rol_reg32_imm8(buf, dst_id, imm_i8 as u8);
                    Ok(EncodeOutput::new())
                }
                [Operand::Reg(dst), Operand::Reg(src)] => {
                    // rol r32, cl
                    let _dst_reg = reg32_from(*dst)?;
                    let _src_reg = reg32_from(*src)?;
                    if reg32_from(*src)? != Reg32::Ecx {
                        return Err(EncodeError::Unsupported("E0036: rol variable count requires CL"));
                    }
                    let dst_id = dst.0;
                    rol_reg32_cl(buf, dst_id);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Rol { width },
                }),
            }
        }
        IntWidth::W16 => {
            match inst.operands.as_slice() {
                [Operand::Reg(dst), Operand::Imm64(imm)] => {
                    // rol r16, imm8
                    let dst_id = dst.0;
                    let imm_i8 = i8::try_from(*imm)
                        .map_err(|_| EncodeError::Unsupported("E0035: rol imm must fit in i8"))?;
                    rol_reg16_imm8(buf, dst_id, imm_i8 as u8);
                    Ok(EncodeOutput::new())
                }
                [Operand::Reg(dst), Operand::Reg(src)] => {
                    // rol r16, cl
                    if src.0 != 1 {
                        return Err(EncodeError::Unsupported("E0036: rol variable count requires CL"));
                    }
                    let dst_id = dst.0;
                    rol_reg16_cl(buf, dst_id);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Rol { width },
                }),
            }
        }
        _ => Err(EncodeError::Unsupported("E0034: rol only supports W16, W32, and W64")),
    }
}

/// Phase R15 PA-R15-004: Encode rotate-right instruction.
pub(super) fn encode_ror(inst: &Instruction, buf: &mut CodeBuffer, width: IntWidth) -> Result<EncodeOutput, EncodeError> {
    match width {
        IntWidth::W64 => {
            match inst.operands.as_slice() {
                [Operand::Reg(dst), Operand::Imm64(imm)] => {
                    // ror r64, imm8
                    let dst_reg = reg64_from(*dst)?;
                    let imm_i8 = i8::try_from(*imm)
                        .map_err(|_| EncodeError::Unsupported("E0035: ror imm must fit in i8"))?;
                    ror_reg64_imm8(buf, dst_reg, imm_i8 as u8);
                    Ok(EncodeOutput::new())
                }
                [Operand::Reg(dst), Operand::Reg(src)] => {
                    // ror r64, cl (rotate count in CL)
                    let dst_reg = reg64_from(*dst)?;
                    if reg64_from(*src)? != Reg64::Rcx {
                        return Err(EncodeError::Unsupported("E0036: ror variable count requires CL"));
                    }
                    ror_reg64_cl(buf, dst_reg);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Ror { width },
                }),
            }
        }
        IntWidth::W32 => {
            match inst.operands.as_slice() {
                [Operand::Reg(dst), Operand::Imm64(imm)] => {
                    // ror r32, imm8
                    let _dst_reg = reg32_from(*dst)?;
                    let dst_id = dst.0;
                    let imm_i8 = i8::try_from(*imm)
                        .map_err(|_| EncodeError::Unsupported("E0035: ror imm must fit in i8"))?;
                    ror_reg32_imm8(buf, dst_id, imm_i8 as u8);
                    Ok(EncodeOutput::new())
                }
                [Operand::Reg(dst), Operand::Reg(src)] => {
                    // ror r32, cl
                    let _dst_reg = reg32_from(*dst)?;
                    let _src_reg = reg32_from(*src)?;
                    if reg32_from(*src)? != Reg32::Ecx {
                        return Err(EncodeError::Unsupported("E0036: ror variable count requires CL"));
                    }
                    let dst_id = dst.0;
                    ror_reg32_cl(buf, dst_id);
                    Ok(EncodeOutput::new())
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Ror { width },
                }),
            }
        }
        _ => Err(EncodeError::Unsupported("E0034: ror only supports W32 and W64")),
    }
}
