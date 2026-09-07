//! Atomic and LOCK-prefixed encoders: `xchg`, `lock cmpxchg[16b/32]`,
//! `lock xadd`, `lock add/sub/inc`, `lock bts/btr/btc`, `lock and/or/xor`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

/// Phase R13 PA-R13-003: Encode xchg [base + disp], src instruction.
/// Expects [MemSib with base and disp, Reg]. Emits via `xchg_mem_base_disp_reg64`.
pub(super) fn encode_xchg_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            xchg_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Xchg }),
    }
}

/// Phase R13 PA-R13-004: Encode lock cmpxchg [base + disp], src instruction.
/// Expects [MemSib with base and disp, Reg]. Emits via `lock_cmpxchg_mem_base_disp_reg64`.
pub(super) fn encode_lock_cmpxchg_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            lock_cmpxchg_mem_base_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg }),
    }
}

/// Phase R16 PA-R16-003 (issue #969): Encode lock cmpxchg32 [base + disp], src instruction.
/// Expects [MemSib with base and disp, Reg]. Emits via `lock_cmpxchg_mem_base_disp_reg32`.
pub(super) fn encode_lock_cmpxchg32_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            lock_cmpxchg_mem_base_disp_reg32(buf, reg64_from(*base)?, *disp, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg32 }),
    }
}

pub(super) fn encode_lock_cmpxchg16b_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            lock_cmpxchg16b_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockCmpxchg16b }),
    }
}

/// Phase R15 PA-R15-002 (issue #957): Encode lock xadd instruction.
/// Expects [MemSib { index: None, scale: Scale::X1, disp }, Reg(src)].
/// W32/W64 dispatch. Other widths → Unsupported. Any other shape → OperandShape.
pub(super) fn encode_lock_xadd(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg = reg64_from(*src)?;
            match width {
                IntWidth::W32 => lock_xadd_mem_base_disp_reg32(buf, base_reg, *disp, src_reg),
                IntWidth::W64 => lock_xadd_mem_base_disp_reg64(buf, base_reg, *disp, src_reg),
                _ => {
                    return Err(EncodeError::Unsupported(
                        "E0031: lock_xadd only supports W32 and W64",
                    ))
                }
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockXadd { width } }),
    }
}

/// Phase R15 PA-R15-003: Encode lock add instruction.
/// Supports: [mem], imm8/imm32/r32/r64
pub(super) fn encode_lock_add(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W32 && width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0032: lock_add only supports W32 and W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock add [mem], reg form (base + disp)
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg = reg64_from(*src)?;
            match width {
                IntWidth::W32 => lock_add_mem_base_disp_reg32(buf, base_reg, *disp, src_reg),
                IntWidth::W64 => lock_add_mem_base_disp_reg64(buf, base_reg, *disp, src_reg),
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        // lock add [mem], imm form (base + disp)
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Imm64(imm_val)] => {
            let base_reg = reg64_from(*base)?;
            let imm = *imm_val;

            // Try imm8 first
            if let Ok(imm8) = i8::try_from(imm) {
                match width {
                    IntWidth::W32 => {
                        lock_add_mem_base_disp_imm8_w32(buf, base_reg, *disp, imm8);
                    }
                    IntWidth::W64 => {
                        lock_add_mem_base_disp_imm8(buf, base_reg, *disp, imm8);
                    }
                    _ => unreachable!(),
                }
                return Ok(EncodeOutput::new());
            }

            // Try imm32
            if let Ok(imm32) = i32::try_from(imm) {
                match width {
                    IntWidth::W32 => {
                        lock_add_mem_base_disp_imm32_w32(buf, base_reg, *disp, imm32);
                    }
                    IntWidth::W64 => {
                        lock_add_mem_base_disp_imm32(buf, base_reg, *disp, imm32);
                    }
                    _ => unreachable!(),
                }
                return Ok(EncodeOutput::new());
            }

            // imm out of range
            Err(EncodeError::Unsupported(
                "E0033: lock_add imm out of i32 range",
            ))
        }
        // lock add [disp32], imm form (absolute displacement, SIB no-base)
        [Operand::MemDisp { disp }, Operand::Imm64(imm_val)] => {
            let imm = *imm_val;

            // Try imm8 first
            if let Ok(imm8) = i8::try_from(imm) {
                lock_add_mem_abs_disp32_imm8(buf, width, *disp, imm8);
                return Ok(EncodeOutput::new());
            }

            // Try imm32
            if let Ok(imm32) = i32::try_from(imm) {
                lock_add_mem_abs_disp32_imm32(buf, width, *disp, imm32);
                return Ok(EncodeOutput::new());
            }

            // imm out of range
            Err(EncodeError::Unsupported(
                "E0033: lock_add imm out of i32 range",
            ))
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockAdd { width } }),
    }
}

/// Phase R15 PA-R15-003: Encode lock sub instruction.
/// Supports: [mem], imm8/imm32/r32/r64
pub(super) fn encode_lock_sub(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W32 && width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0032: lock_sub only supports W32 and W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock sub [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg = reg64_from(*src)?;
            match width {
                IntWidth::W32 => lock_sub_mem_base_disp_reg32(buf, base_reg, *disp, src_reg),
                IntWidth::W64 => lock_sub_mem_base_disp_reg64(buf, base_reg, *disp, src_reg),
                _ => unreachable!(),
            }
            Ok(EncodeOutput::new())
        }
        // lock sub [mem], imm form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Imm64(imm_val)] => {
            let base_reg = reg64_from(*base)?;
            let imm = *imm_val;

            // Try imm8 first
            if let Ok(imm8) = i8::try_from(imm) {
                match width {
                    IntWidth::W32 => {
                        lock_sub_mem_base_disp_imm8_w32(buf, base_reg, *disp, imm8);
                    }
                    IntWidth::W64 => {
                        lock_sub_mem_base_disp_imm8(buf, base_reg, *disp, imm8);
                    }
                    _ => unreachable!(),
                }
                return Ok(EncodeOutput::new());
            }

            // Try imm32
            if let Ok(imm32) = i32::try_from(imm) {
                match width {
                    IntWidth::W32 => {
                        lock_sub_mem_base_disp_imm32_w32(buf, base_reg, *disp, imm32);
                    }
                    IntWidth::W64 => {
                        lock_sub_mem_base_disp_imm32(buf, base_reg, *disp, imm32);
                    }
                    _ => unreachable!(),
                }
                return Ok(EncodeOutput::new());
            }

            // imm out of range
            Err(EncodeError::Unsupported(
                "E0033: lock_sub imm out of i32 range",
            ))
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockSub { width } }),
    }
}

/// Phase R16 PA-R16-007: Encode lock inc instruction.
/// Supports: [mem] (one operand only).
/// Both W32 and W64 forms supported; uses SIB no-base for absolute displacement.
pub(super) fn encode_lock_inc(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W32 && width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0034: lock_inc only supports W32 and W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock inc [mem] form with absolute displacement (SIB no-base)
        [Operand::MemDisp { disp }] => {
            lock_inc_mem_abs_disp32(buf, width, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockInc { width } }),
    }
}

/// Phase R16 PA-R16-002: Encode lock bts instruction.
/// Supports: [mem], imm8/r64 (W64 only).
pub(super) fn encode_lock_bts(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0044: lock_bts only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock bts [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(index_reg)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg_val = reg64_from(*index_reg)?;
            lock_bts_mem_base_disp_reg64(buf, base_reg, *disp, index_reg_val);
            Ok(EncodeOutput::new())
        }
        // lock bts [mem], imm form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Imm64(imm_val)] => {
            let base_reg = reg64_from(*base)?;
            let imm = u8::try_from(*imm_val)
                .map_err(|_| EncodeError::Unsupported("E0044: lock_bts imm8 out of u8 range"))?;
            lock_bts_mem_base_disp_imm8(buf, base_reg, *disp, imm);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockBts { width } }),
    }
}

/// Phase R16 PA-R16-002: Encode lock btr instruction.
/// Supports: [mem], imm8/r64 (W64 only).
pub(super) fn encode_lock_btr(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0045: lock_btr only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock btr [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(index_reg)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg_val = reg64_from(*index_reg)?;
            lock_btr_mem_base_disp_reg64(buf, base_reg, *disp, index_reg_val);
            Ok(EncodeOutput::new())
        }
        // lock btr [mem], imm form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Imm64(imm_val)] => {
            let base_reg = reg64_from(*base)?;
            let imm = u8::try_from(*imm_val)
                .map_err(|_| EncodeError::Unsupported("E0045: lock_btr imm8 out of u8 range"))?;
            lock_btr_mem_base_disp_imm8(buf, base_reg, *disp, imm);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockBtr { width } }),
    }
}

/// Phase R16 PA-R16-002: Encode lock btc instruction.
/// Supports: [mem], imm8/r64 (W64 only).
pub(super) fn encode_lock_btc(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0046: lock_btc only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock btc [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(index_reg)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg_val = reg64_from(*index_reg)?;
            lock_btc_mem_base_disp_reg64(buf, base_reg, *disp, index_reg_val);
            Ok(EncodeOutput::new())
        }
        // lock btc [mem], imm form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Imm64(imm_val)] => {
            let base_reg = reg64_from(*base)?;
            let imm = u8::try_from(*imm_val)
                .map_err(|_| EncodeError::Unsupported("E0046: lock_btc imm8 out of u8 range"))?;
            lock_btc_mem_base_disp_imm8(buf, base_reg, *disp, imm);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockBtc { width } }),
    }
}

/// Phase R16 PA-R16-006: Encode lock and instruction.
/// Supports: [mem], r64 (W64 only).
pub(super) fn encode_lock_and(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0047: lock_and only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock and [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src_reg)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg_val = reg64_from(*src_reg)?;
            lock_and_mem_base_disp_reg64(buf, base_reg, *disp, src_reg_val);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockAnd { width } }),
    }
}

/// Phase R16 PA-R16-006: Encode lock or instruction.
/// Supports: [mem], r64 (W64 only).
pub(super) fn encode_lock_or(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0048: lock_or only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock or [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src_reg)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg_val = reg64_from(*src_reg)?;
            lock_or_mem_base_disp_reg64(buf, base_reg, *disp, src_reg_val);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockOr { width } }),
    }
}

/// Phase R16 PA-R16-006: Encode lock xor instruction.
/// Supports: [mem], r64 (W64 only).
pub(super) fn encode_lock_xor(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    if width != IntWidth::W64 {
        return Err(EncodeError::Unsupported(
            "E0049: lock_xor only supports W64",
        ));
    }

    match inst.operands.as_slice() {
        // lock xor [mem], reg form
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src_reg)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg_val = reg64_from(*src_reg)?;
            lock_xor_mem_base_disp_reg64(buf, base_reg, *disp, src_reg_val);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::LockXor { width } }),
    }
}
