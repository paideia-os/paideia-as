//! Interrupt / privilege-transition encoders: `int`, `iret`/`iretq`,
//! `sysret`/`syscall`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

pub(super) fn encode_int(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // int expects exactly 1 operand: an immediate value that fits in u8
    match inst.operands.as_slice() {
        [Operand::Imm64(imm)] => {
            // Check that the operand fits in u8
            if *imm > u8::MAX as i64 {
                return Err(EncodeError::Unsupported("int operand > u8"));
            }
            encode_int_imm8(buf, *imm as u8);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Int,
            expected: 1,
            got: inst.operands.len(),
        }),
    }
}

pub(super) fn encode_iret_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // iret expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Iret,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_iret(buf);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_iretq_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // iretq expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Iretq,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_iretq(buf);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_sysret_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // sysret expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Sysret,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_sysret(buf);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_syscall_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // syscall expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Syscall,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_syscall(buf);
    Ok(EncodeOutput::new())
}
