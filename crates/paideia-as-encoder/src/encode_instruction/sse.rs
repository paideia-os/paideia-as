//! Scalar SSE encoders reachable from the dispatcher: the two-operand
//! `xmm dst, xmm src` family (movsd/movss/addsd/…/comiss), the sign
//! conversions (`cvtsi2ss/sd`, `cvttss/sd2si`), and the movd/movq
//! bitcasts between GPR and XMM.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

/// paideia-os #1333, paideia-as#1333: dispatch a two-operand `[xmm dst, xmm src]`
/// scalar SSE instruction (movsd/movss/addsd/.../ucomiss/comiss family) to
/// `encode_sse::encode_xmm_xmm`.
pub(super) fn encode_sse_xmm_xmm(
    inst: &Instruction,
    mnemonic: Mnemonic,
    prefix: Option<u8>,
    opcode: u8,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src)] => {
            let dst_xmm = crate::encode_sse::xmm_id_from_regid(dst.0)
                .ok_or(EncodeError::OperandShape { mnemonic })?;
            let src_xmm = crate::encode_sse::xmm_id_from_regid(src.0)
                .ok_or(EncodeError::OperandShape { mnemonic })?;
            crate::encode_sse::encode_xmm_xmm(buf, prefix, opcode, dst_xmm, src_xmm);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic }),
    }
}

/// Dispatch `cvtsi2sd`/`cvtsi2ss xmm dst, r64 src`.
pub(super) fn encode_cvtsi2s_inst(
    inst: &Instruction,
    mnemonic: Mnemonic,
    prefix: u8,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src)] => {
            let dst_xmm = crate::encode_sse::xmm_id_from_regid(dst.0)
                .ok_or(EncodeError::OperandShape { mnemonic })?;
            if src.0 >= 16 {
                return Err(EncodeError::OperandShape { mnemonic });
            }
            crate::encode_sse::encode_cvtsi2s(buf, prefix, dst_xmm, src.0);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic }),
    }
}

/// Dispatch `cvttsd2si`/`cvttss2si r64 dst, xmm src`.
pub(super) fn encode_cvtts2si_inst(
    inst: &Instruction,
    mnemonic: Mnemonic,
    prefix: u8,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src)] => {
            if dst.0 >= 16 {
                return Err(EncodeError::OperandShape { mnemonic });
            }
            let src_xmm = crate::encode_sse::xmm_id_from_regid(src.0)
                .ok_or(EncodeError::OperandShape { mnemonic })?;
            crate::encode_sse::encode_cvtts2si(buf, prefix, dst.0, src_xmm);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic }),
    }
}

/// Dispatch `movd xmm, r32` (`to_xmm=true`) / `movd r32, xmm` (`to_xmm=false`).
pub(super) fn encode_movd_bitcast_inst(
    inst: &Instruction,
    to_xmm: bool,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    let mnemonic = Mnemonic::MovdBitcast { to_xmm };
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src)] => {
            let (xmm_id, gpr_id) = if to_xmm {
                let xmm = crate::encode_sse::xmm_id_from_regid(dst.0)
                    .ok_or(EncodeError::OperandShape { mnemonic })?;
                if src.0 >= 16 {
                    return Err(EncodeError::OperandShape { mnemonic });
                }
                (xmm, src.0)
            } else {
                if dst.0 >= 16 {
                    return Err(EncodeError::OperandShape { mnemonic });
                }
                let xmm = crate::encode_sse::xmm_id_from_regid(src.0)
                    .ok_or(EncodeError::OperandShape { mnemonic })?;
                (xmm, dst.0)
            };
            crate::encode_sse::encode_movd_movq_bitcast(buf, false, to_xmm, xmm_id, gpr_id);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic }),
    }
}

/// Dispatch `movq xmm, r64` (`to_xmm=true`) / `movq r64, xmm` (`to_xmm=false`).
pub(super) fn encode_movq_bitcast_inst(
    inst: &Instruction,
    to_xmm: bool,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    let mnemonic = Mnemonic::MovqBitcast { to_xmm };
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src)] => {
            let (xmm_id, gpr_id) = if to_xmm {
                let xmm = crate::encode_sse::xmm_id_from_regid(dst.0)
                    .ok_or(EncodeError::OperandShape { mnemonic })?;
                if src.0 >= 16 {
                    return Err(EncodeError::OperandShape { mnemonic });
                }
                (xmm, src.0)
            } else {
                if dst.0 >= 16 {
                    return Err(EncodeError::OperandShape { mnemonic });
                }
                let xmm = crate::encode_sse::xmm_id_from_regid(src.0)
                    .ok_or(EncodeError::OperandShape { mnemonic })?;
                (xmm, dst.0)
            };
            crate::encode_sse::encode_movd_movq_bitcast(buf, true, to_xmm, xmm_id, gpr_id);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic }),
    }
}
