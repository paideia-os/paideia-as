//! Timing and TLB-shootdown encoders: `rdtsc`, `invlpg`, `invpcid`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

pub(super) fn encode_rdtsc_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // rdtsc expects exactly 0 operands (implicitly reads time-stamp counter into RDX:RAX)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Rdtsc,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_rdtsc(buf);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_invlpg_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // invlpg expects exactly 1 operand: memory address
    if inst.operands.len() != 1 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Invlpg,
            expected: 1,
            got: inst.operands.len(),
        });
    }

    match &inst.operands[0] {
        Operand::MemSib {
            base,
            index: None,
            scale: Scale::X1,
            disp,
        } => {
            // [base + disp] form
            encode_invlpg(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Invlpg,
        }),
    }
}

/// v0.21-009-followup (#1297): Encode `invpcid r64, m128` instruction.
/// Two operands: [Reg(type_reg), MemSib{base, disp}]. The register holds
/// the INVPCID type in its low 2 bits; the m128 memory operand supplies
/// the 128-bit descriptor `[pcid_low12:64][linear_addr:64]`.
///
/// Encoding: 66 [REX] 0F 38 82 /r per Intel SDM Vol 2A INVPCID.
pub(super) fn encode_invpcid_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    if inst.operands.len() != 2 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Invpcid,
            expected: 2,
            got: inst.operands.len(),
        });
    }
    match inst.operands.as_slice() {
        [
            Operand::Reg(reg),
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            crate::encode::invpcid_reg_mem_base_disp(
                buf,
                reg64_from(*reg)?,
                reg64_from(*base)?,
                *disp,
            );
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Invpcid,
        }),
    }
}
