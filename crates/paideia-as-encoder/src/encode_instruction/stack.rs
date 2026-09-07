//! Stack-hierarchy encoders: `push`/`pop`, `pushfq`/`popfq`, `int3`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

/// Phase R9 m2-001 (PA-R9-001): Encode push 64-bit register instruction.
/// Extends to handle immediate operands (imm8 via sign-extension, imm32 via sign-extension).
/// Expects exactly one operand. Rejects Mode32. Emits via `push_reg64`, `push_imm8`, or `push_imm32`.
pub(super) fn encode_push(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase R9 m2-001: reject Mode32
    if inst.mode == InstrMode::Mode32 {
        return Err(EncodeError::Unsupported(
            "E0020: push not supported in 32-bit mode",
        ));
    }
    match inst.operands.as_slice() {
        [Operand::Reg(src)] => {
            push_reg64(buf, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Imm64(imm)] => {
            if let Ok(i) = i8::try_from(*imm) {
                push_imm8(buf, i);
            } else if let Ok(i) = i32::try_from(*imm) {
                push_imm32(buf, i);
            } else {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Push,
                });
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Push,
        }),
    }
}

/// Phase R9 m2-001 (PA-R9-001): Encode pop 64-bit register instruction.
/// Expects exactly one register operand. Rejects Mode32. Emits via `pop_reg64`.
pub(super) fn encode_pop(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase R9 m2-001: reject Mode32
    if inst.mode == InstrMode::Mode32 {
        return Err(EncodeError::Unsupported(
            "E0021: pop r64 not supported in 32-bit mode",
        ));
    }
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            pop_reg64(buf, reg64_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Pop,
        }),
    }
}

/// Phase R9 m2-002 (PA-R9-002): Encode pushfq instruction.
/// Push flags register onto stack: `pushfq` (0x9C). Zero operands.
pub(super) fn encode_pushfq(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Pushfq,
        });
    }
    buf.bytes.push(0x9C);
    Ok(EncodeOutput::new())
}

/// Phase R9 m2-002 (PA-R9-002): Encode popfq instruction.
/// Pop flags register from stack: `popfq` (0x9D). Zero operands.
pub(super) fn encode_popfq(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Popfq,
        });
    }
    buf.bytes.push(0x9D);
    Ok(EncodeOutput::new())
}

/// Phase R9 m2-003 (PA-R9-003): Encode int3 instruction.
/// Breakpoint interrupt: `int3` (0xCC). Zero operands.
pub(super) fn encode_int3(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Int3,
        });
    }
    buf.bytes.push(0xCC);
    Ok(EncodeOutput::new())
}
