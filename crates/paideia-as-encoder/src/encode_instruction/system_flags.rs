//! Zero-operand system/flag/CPU-metadata encoders: `cli`/`cld`/`sti`/
//! `std`, `hlt`/`nop`, `swapgs`, `cpuid`, `ud2`, `endbr64`/`endbr32`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

pub(super) fn encode_cli(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Cli,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0xFA);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_cld(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Cld,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x84); // sentinel for CLD
    Ok(EncodeOutput::new())
}

pub(super) fn encode_sti(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Sti,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0xFB);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_std(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Std,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x85); // sentinel for STD
    Ok(EncodeOutput::new())
}

pub(super) fn encode_hlt(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Hlt,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0xF4);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_nop(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Nop,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x90);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_swapgs(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Swapgs,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x81); // sentinel for SWAPGS
    Ok(EncodeOutput::new())
}

pub(super) fn encode_cpuid(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Cpuid,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x82); // sentinel for CPUID
    Ok(EncodeOutput::new())
}

pub(super) fn encode_ud2(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Ud2,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x83); // sentinel for UD2
    Ok(EncodeOutput::new())
}

pub(super) fn encode_endbr64(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Endbr64,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x86); // sentinel for ENDBR64
    Ok(EncodeOutput::new())
}

pub(super) fn encode_endbr32(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Endbr32,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x87); // sentinel for ENDBR32
    Ok(EncodeOutput::new())
}
