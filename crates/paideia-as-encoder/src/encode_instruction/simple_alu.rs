//! Small integer-arithmetic mnemonics: `not`, `bswap`/`bswap32`,
//! `div`/`idiv`/`mul`, `inc`/`dec`, and `ltr`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

/// Encode `not r64` (bitwise NOT / one's complement) — Phase 7 m4-001.
///
/// Expects exactly one register operand. Emits `REX.W F7 /2` via `not_reg64`.
pub(super) fn encode_not(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            not_reg64(buf, reg64_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Not,
        }),
    }
}

/// Phase R13 PA-R13-014: Encode byte-swap 64-bit register instruction.
/// Expects exactly one register operand. Emits `REX.W 0F C8+rd` via `bswap_reg64`.
pub(super) fn encode_bswap(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            bswap_reg64(buf, reg64_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bswap,
        }),
    }
}

/// Phase R15 PA-R15-001: Encode byte-swap 32-bit register instruction.
/// Expects exactly one register operand. Emits `0F C8+rd` via `bswap_reg32`.
pub(super) fn encode_bswap32(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            bswap_reg32(buf, reg32_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Bswap32,
        }),
    }
}

/// Phase R11 PA-R11-006: Encode unsigned 64-bit divide instruction.
/// Expects exactly one register operand (the divisor). Emits via `div_reg64`.
pub(super) fn encode_div(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(src)] => {
            div_reg64(buf, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Div,
        }),
    }
}

/// Phase R11 PA-R11-006: Encode signed 64-bit divide instruction.
/// Expects exactly one register operand (the divisor). Emits via `idiv_reg64`.
pub(super) fn encode_idiv(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(src)] => {
            idiv_reg64(buf, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Idiv,
        }),
    }
}

/// paideia-as#1398: Encode unsigned 64-bit multiply instruction.
///
/// Expects exactly one register operand (the multiplier). Emits via `mul_reg64`.
/// The multiplicand is implicit in rax; the 128-bit product lands in rdx:rax.
/// Complements `imul` (signed low-64) and `div` (128÷64) for wide-integer
/// software emulation (postui#43 Fixed64 32×32-split multiply).
pub(super) fn encode_mul(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(src)] => {
            mul_reg64(buf, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Mul,
        }),
    }
}

/// Phase R13 PA-R13-005 (issue #934): Encode `inc r64`.
/// Expects exactly one register operand. Emits `REX.W FF /0` via `inc_reg64`.
pub(super) fn encode_inc(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            inc_reg64(buf, reg64_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Inc,
        }),
    }
}

/// Phase R13 PA-R13-005 (issue #934): Encode `dec r64`.
/// Expects exactly one register operand. Emits `REX.W FF /1` via `dec_reg64`.
pub(super) fn encode_dec(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            dec_reg64(buf, reg64_from(*dst)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Dec,
        }),
    }
}

/// Phase R13 PA-R13-001: Encode load task register instruction.
/// Expects exactly one register operand. Emits via `ltr_reg16`.
pub(super) fn encode_ltr(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(src)] => {
            ltr_reg16(buf, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Ltr,
        }),
    }
}
