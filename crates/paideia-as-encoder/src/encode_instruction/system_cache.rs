//! System / cache-hierarchy encoders: memory fences (`mfence`/`sfence`/
//! `lfence`), `pause`, `wbinvd`/`invd`, `fxsave`/`fxrstor`, `xsaveopt`/
//! `xrstor`, `clflush`/`clflushopt`, and the `prefetch{nta,t0,t1,t2}`
//! family.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

/// Phase R13 PA-R13-005: Encode mfence instruction.
/// Expects zero operands. Emits via `mfence`.
pub(super) fn encode_mfence_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Mfence, expected: 0, got: inst.operands.len(),
        });
    }
    mfence(buf);
    Ok(EncodeOutput::new())
}

/// Phase R14 PA-R14-004: Encode sfence instruction.
/// Expects zero operands. Emits via `sfence`.
pub(super) fn encode_sfence_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Sfence, expected: 0, got: inst.operands.len(),
        });
    }
    sfence(buf);
    Ok(EncodeOutput::new())
}

/// Phase R14 PA-R14-004: Encode lfence instruction.
/// Expects zero operands. Emits via `lfence`.
pub(super) fn encode_lfence_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Lfence, expected: 0, got: inst.operands.len(),
        });
    }
    lfence(buf);
    Ok(EncodeOutput::new())
}

/// Phase R16 PA-R16-007: Encode pause instruction.
/// Expects zero operands. Emits via `pause`.
pub(super) fn encode_pause_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Pause, expected: 0, got: inst.operands.len(),
        });
    }
    pause(buf);
    Ok(EncodeOutput::new())
}

/// Phase R14 PA-R14-005: Encode wbinvd instruction.
/// Expects zero operands. Emits via `wbinvd`.
pub(super) fn encode_wbinvd_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Wbinvd, expected: 0, got: inst.operands.len(),
        });
    }
    wbinvd(buf);
    Ok(EncodeOutput::new())
}

/// Phase R14 PA-R14-005: Encode invd instruction.
/// Expects zero operands. Emits via `invd`.
pub(super) fn encode_invd_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Invd, expected: 0, got: inst.operands.len(),
        });
    }
    invd(buf);
    Ok(EncodeOutput::new())
}

/// Phase R13 PA-R13-007: Encode fxsave instruction.
/// Expects one memory operand [base + disp]. Emits via `fxsave_mem_base_disp`.
pub(super) fn encode_fxsave_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            fxsave_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Fxsave }),
    }
}

/// Phase R13 PA-R13-007: Encode fxrstor instruction.
/// Expects one memory operand [base + disp]. Emits via `fxrstor_mem_base_disp`.
pub(super) fn encode_fxrstor_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            fxrstor_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Fxrstor }),
    }
}

/// Phase R15 PA-R15-m4-005 (issue #1022): Encode xsaveopt instruction.
/// Expects one memory operand [base + disp]. Emits via `xsaveopt_mem_base_disp`.
pub(super) fn encode_xsaveopt_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            xsaveopt_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Xsaveopt }),
    }
}

/// Phase R15 PA-R15-m4-005 (issue #1022): Encode xrstor instruction.
/// Expects one memory operand [base + disp]. Emits via `xrstor_mem_base_disp`.
pub(super) fn encode_xrstor_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            xrstor_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Xrstor }),
    }
}

/// Phase R14 PA-R14-005: Encode clflush instruction.
/// Expects one memory operand [base + disp]. Emits via `clflush_mem_base_disp`.
pub(super) fn encode_clflush_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            clflush_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Clflush }),
    }
}

/// Phase R14 PA-R14-005: Encode clflushopt instruction.
/// Expects one memory operand [base + disp]. Emits via `clflushopt_mem_base_disp`.
pub(super) fn encode_clflushopt_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            clflushopt_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Clflushopt }),
    }
}

/// Phase R14 PA-R14-006: Encode prefetchnta instruction.
/// Expects one memory operand [base + disp]. Emits via `prefetchnta_mem_base_disp`.
pub(super) fn encode_prefetchnta_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            prefetchnta_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Prefetchnta }),
    }
}

/// Phase R14 PA-R14-006: Encode prefetcht0 instruction.
/// Expects one memory operand [base + disp]. Emits via `prefetcht0_mem_base_disp`.
pub(super) fn encode_prefetcht0_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            prefetcht0_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Prefetcht0 }),
    }
}

/// Phase R14 PA-R14-006: Encode prefetcht1 instruction.
/// Expects one memory operand [base + disp]. Emits via `prefetcht1_mem_base_disp`.
pub(super) fn encode_prefetcht1_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            prefetcht1_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Prefetcht1 }),
    }
}

/// Phase R14 PA-R14-006: Encode prefetcht2 instruction.
/// Expects one memory operand [base + disp]. Emits via `prefetcht2_mem_base_disp`.
pub(super) fn encode_prefetcht2_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            prefetcht2_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape { mnemonic: Mnemonic::Prefetcht2 }),
    }
}
