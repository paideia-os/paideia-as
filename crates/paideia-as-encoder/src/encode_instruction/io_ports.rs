//! I/O port encoders: `in` and `out` (immediate and DX-indirect forms).
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

pub(super) fn encode_in(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: u8,
) -> Result<EncodeOutput, EncodeError> {
    // `in` expects exactly 1 operand: the data register (al/ax/eax, encoded as Rax)
    if inst.operands.len() != 1 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::In { width },
            expected: 1,
            got: inst.operands.len(),
        });
    }

    // Verify the operand is Rax (al, ax, or eax depending on width)
    match &inst.operands[0] {
        Operand::Reg(reg) => {
            if *reg != RegId(0) {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::In { width },
                });
            }
        }
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::In { width },
            });
        }
    }

    encode_in_dx(buf, width);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_out(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: u8,
) -> Result<EncodeOutput, EncodeError> {
    // `out` expects exactly 1 operand: the data register (al/ax/eax, encoded as Rax)
    if inst.operands.len() != 1 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Out { width },
            expected: 1,
            got: inst.operands.len(),
        });
    }

    // Verify the operand is Rax (al, ax, or eax depending on width)
    match &inst.operands[0] {
        Operand::Reg(reg) => {
            if *reg != RegId(0) {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Out { width },
                });
            }
        }
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Out { width },
            });
        }
    }

    encode_out_dx(buf, width);
    Ok(EncodeOutput::new())
}
