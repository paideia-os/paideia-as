//! Special MOV forms: CR/DR register dispatch and low-level encoders,
//! plus the non-temporal store `movnti`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

/// Dispatcher for MOV to/from control register (Phase 6, m1-002).
///
/// Extracts CR and GPR indices from operands and routes to encode_mov_cr.
/// - write=true: mov cr_idx, gpr (destination is CR, source is GPR)
/// - write=false: mov gpr, cr_idx (destination is GPR, source is CR)
pub(super) fn encode_mov_cr_dispatcher(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(first), Operand::Reg(second)] => {
            let (cr_id, gpr_id) = if write {
                // mov cr, gpr: first is CR, second is GPR
                (first.0, second.0)
            } else {
                // mov gpr, cr: first is GPR, second is CR
                (second.0, first.0)
            };

            // Convert CR ID to CR index: cr_idx = RegId - 16
            let cr_idx = cr_id - 16;

            // GPR index is directly the reg_id (0-15)
            let gpr_idx = gpr_id;

            // Encode using the low-level helper
            encode_mov_cr(buf, write, cr_idx, gpr_idx);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Mov,
        }),
    }
}

/// Dispatcher for MOV to/from debug register (Phase 6, m1-003).
///
/// Extracts DR and GPR indices from operands and routes to encode_mov_dr.
/// - write=true: mov dr_idx, gpr (destination is DR, source is GPR)
/// - write=false: mov gpr, dr_idx (destination is GPR, source is DR)
pub(super) fn encode_mov_dr_dispatcher(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(first), Operand::Reg(second)] => {
            let (dr_id, gpr_id) = if write {
                // mov dr, gpr: first is DR, second is GPR
                (first.0, second.0)
            } else {
                // mov gpr, dr: first is GPR, second is DR
                (second.0, first.0)
            };

            // Convert DR ID to DR index: dr_idx = RegId - 25 (compact encoding)
            let dr_idx = dr_id - 25;

            // GPR index is directly the reg_id (0-15)
            let gpr_idx = gpr_id;

            // Encode using the low-level helper
            encode_mov_dr(buf, write, dr_idx, gpr_idx);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Mov,
        }),
    }
}


pub(super) fn encode_mov_cr_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<EncodeOutput, EncodeError> {
    // mov cr_idx, gpr (write=true): first=CR, second=GPR
    // mov gpr, cr_idx (write=false): first=GPR, second=CR
    match inst.operands.as_slice() {
        [Operand::Reg(first_reg), Operand::Reg(second_reg)] => {
            let (cr_idx, gpr_idx) = if write {
                // mov cr_idx, gpr: first is CR, second is GPR
                (first_reg.0, second_reg.0)
            } else {
                // mov gpr, cr_idx: first is GPR, second is CR
                (second_reg.0, first_reg.0)
            };

            // Validate CR index: CR0, CR2, CR3, CR4, CR8 supported.
            match cr_idx {
                0 | 2 | 3 | 4 | 8 => {}
                _ => {
                    return Err(EncodeError::Unsupported("CR index not supported"));
                }
            }

            // Validate GPR index: must be 0-15
            if gpr_idx > 15 {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::MovCr { write },
                });
            }

            // Emit the instruction using the low-level encoder
            encode_mov_cr(buf, write, cr_idx, gpr_idx);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::MovCr { write },
        }),
    }
}

pub(super) fn encode_mov_dr_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<EncodeOutput, EncodeError> {
    // mov dr_idx, gpr (write=true): first=DR, second=GPR
    // mov gpr, dr_idx (write=false): first=GPR, second=DR
    match inst.operands.as_slice() {
        [Operand::Reg(first_reg), Operand::Reg(second_reg)] => {
            let (dr_idx, gpr_idx) = if write {
                // mov dr_idx, gpr: first is DR, second is GPR
                (first_reg.0, second_reg.0)
            } else {
                // mov gpr, dr_idx: first is GPR, second is DR
                (second_reg.0, first_reg.0)
            };

            // Validate DR index: only DR0..DR7 exist
            if dr_idx > 7 {
                return Err(EncodeError::Unsupported(
                    "DR index > 7 not supported",
                ));
            }

            // Validate GPR index: must be 0-15
            if gpr_idx > 15 {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::MovDr { write },
                });
            }

            // Emit the instruction using the low-level encoder
            encode_mov_dr(buf, write, dr_idx, gpr_idx);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::MovDr { write },
        }),
    }
}

/// Phase R14 PA-R14-003 (issue #946): Encode non-temporal store movnti [mem], r32/r64.
/// Expects [MemSib, Reg]. Dispatches per width (W32 or W64) to store-form encoders.
pub(super) fn encode_movnti(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        // Base + disp, no index (Scale::X1 sentinel)
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg = reg64_from(*src)?;
            match width {
                IntWidth::W32 => movnti_mem_base_disp_reg32(buf, base_reg, *disp, src_reg),
                IntWidth::W64 => movnti_mem_base_disp_reg64(buf, base_reg, *disp, src_reg),
                _ => {
                    return Err(EncodeError::Unsupported(
                        "E0030: movnti only supports W32 and W64 widths",
                    ))
                }
            }
            Ok(EncodeOutput::new())
        }
        // Base + index*scale + disp (SIB form)
        [Operand::MemSib { base, index: Some(index), scale, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg = reg64_from(*index)?;
            let src_reg = reg64_from(*src)?;
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            match width {
                IntWidth::W32 => movnti_mem_sib_disp_reg32(buf, base_reg, index_reg, scale_bits, *disp, src_reg),
                IntWidth::W64 => movnti_mem_sib_disp_reg64(buf, base_reg, index_reg, scale_bits, *disp, src_reg),
                _ => {
                    return Err(EncodeError::Unsupported(
                        "E0030: movnti only supports W32 and W64 widths",
                    ))
                }
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Movnti { width } }),
    }
}
