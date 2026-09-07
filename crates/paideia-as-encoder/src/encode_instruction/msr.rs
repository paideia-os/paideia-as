//! Model-specific-register and extended-control-register encoders:
//! `wrmsr`/`rdmsr`, `xgetbv`/`xsetbv`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

pub(super) fn encode_wrmsr_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // wrmsr expects exactly 0 operands (MSR index in ECX, value in EDX:EAX)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Wrmsr,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_wrmsr(buf);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_rdmsr_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // rdmsr expects exactly 0 operands (MSR index in ECX, result in EDX:EAX)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Rdmsr,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_rdmsr(buf);
    Ok(EncodeOutput::new())
}

/// v0.21-015 (paideia-as#1294): `xgetbv` — read extended control register.
/// XCR index in ECX; value returned in EDX:EAX. Encoding: `0F 01 D0` (3 bytes).
/// Zero explicit operands. Not privileged when OSXSAVE=1; #GP on ECX index
/// outside supported range. Mirrors `rdmsr` in operand-shape and diagnostics.
pub(super) fn encode_xgetbv_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Xgetbv,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0x01);
    buf.bytes.push(0xD0);
    Ok(EncodeOutput::new())
}

/// v0.21-015 (paideia-as#1294): `xsetbv` — write extended control register.
/// XCR index in ECX; value in EDX:EAX. Encoding: `0F 01 D1` (3 bytes).
/// Zero explicit operands. Privileged (ring 0); required to program XCR0
/// (state-component enable mask) before any XSAVE/XRSTOR variant executes.
/// Mirrors `wrmsr` in operand-shape and diagnostics.
pub(super) fn encode_xsetbv_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Xsetbv,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0x01);
    buf.bytes.push(0xD1);
    Ok(EncodeOutput::new())
}
