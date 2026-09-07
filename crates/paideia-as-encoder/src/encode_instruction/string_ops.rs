//! REP-prefixed string operations: `rep movsb`/`rep movsq`, `rep stosb`/
//! `rep stosq`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

pub(super) fn encode_rep_movsb(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::RepMovsb,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    buf.bytes.push(0xF3);
    buf.bytes.push(0xA4); // rep movsb
    Ok(EncodeOutput::new())
}


pub(super) fn encode_rep_stosq_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // rep stosq expects exactly 0 operands (RAX=value, RCX=count, RDI=destination implicit)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::RepStosq,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_rep_stosq(buf);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_rep_stosb_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // rep stosb expects exactly 0 operands (AL=value, RCX=count, RDI=destination implicit)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::RepStosb,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_rep_stosb(buf);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_rep_movsq_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // rep movsq expects exactly 0 operands (RSI=source, RDI=destination, RCX=count implicit)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::RepMovsq,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_rep_movsq(buf);
    Ok(EncodeOutput::new())
}
