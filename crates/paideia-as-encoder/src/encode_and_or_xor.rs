//! Encoders for bitwise AND, OR, XOR instructions (PA10-003).
//!
//! These instructions support the following operand forms:
//! - [Reg, Reg]: reg-reg binary operation
//! - [Reg, Imm64]: register and immediate (with sign-extension trap guards)
//! - [Reg, MemSib]: register and memory (indexed addressing)
//!
//! All immediate forms implement the sign-extension trap guard:
//! only imm8 if `imm == (imm as i8 as i64)`, only imm32 if
//! `imm == (imm as i32 as i64)`, else error.

use crate::encode::*;
use paideia_as_ir::{Instruction, Operand, RegId};

use super::encode_instruction::{EncodeError, EncodeOutput};

/// Convert an IR register ID to an encoder Reg64.
fn reg64_from(id: RegId) -> Result<Reg64, EncodeError> {
    match id.0 {
        0 => Ok(Reg64::Rax),
        1 => Ok(Reg64::Rcx),
        2 => Ok(Reg64::Rdx),
        3 => Ok(Reg64::Rbx),
        4 => Ok(Reg64::Rsp),
        5 => Ok(Reg64::Rbp),
        6 => Ok(Reg64::Rsi),
        7 => Ok(Reg64::Rdi),
        8 => Ok(Reg64::R8),
        9 => Ok(Reg64::R9),
        10 => Ok(Reg64::R10),
        11 => Ok(Reg64::R11),
        12 => Ok(Reg64::R12),
        13 => Ok(Reg64::R13),
        14 => Ok(Reg64::R14),
        15 => Ok(Reg64::R15),
        _ => Err(EncodeError::Unsupported("invalid register id")),
    }
}

/// Encode bitwise AND instruction.
///
/// Operand forms:
/// - [Reg, Reg]: and r64, r64
/// - [Reg, Imm64]: and r64, imm (with sign-extension trap guards for imm8/imm32)
/// - [Reg, MemSib]: and r64, [mem] (simplified to [base+disp] for phase-1)
pub fn encode_and(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src)] => {
            // and r64, r64 → 48 21 /r
            and_reg64_reg64(buf, reg64_from(*dst)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            let dst_reg = reg64_from(*dst)?;
            let imm_i64 = *imm;

            // Try imm8 form first: REX.W 83 /4 ib
            if imm_i64 == (imm_i64 as i8 as i64) {
                and_reg64_imm8(buf, dst_reg, imm_i64 as i8);
                return Ok(EncodeOutput::new());
            }

            // Try imm32 form: REX.W 81 /4 id
            if imm_i64 == (imm_i64 as i32 as i64) {
                and_reg64_imm32(buf, dst_reg, imm_i64 as i32);
                return Ok(EncodeOutput::new());
            }

            // imm64 out-of-range
            Err(EncodeError::Unsupported(
                "and r64, imm64: x86_64 has no AND r/m64, imm64 form — load to register and use AND r/m64, r64 instead",
            ))
        }
        [
            Operand::Reg(dst),
            Operand::MemSib {
                base,
                index: None,
                scale: _,
                disp,
            },
        ] => {
            // Simplified mem-form: and r64, [base+disp] (no index, no scale in phase-1)
            and_reg64_mem_reg64_disp(buf, reg64_from(*dst)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        operands if operands.iter().any(|op| matches!(op, Operand::Var { .. })) => {
            unreachable!("Operand::Var reached encoder — resolve_var_operands pass was skipped")
        }
        _ => Err(EncodeError::Unsupported(
            "and: unsupported operand shape (mem,mem not encodable on x86)",
        )),
    }
}

/// Encode bitwise OR instruction.
///
/// Operand forms:
/// - [Reg, Reg]: or r64, r64
/// - [Reg, Imm64]: or r64, imm (with sign-extension trap guards for imm8/imm32)
/// - [Reg, MemSib]: or r64, [mem]
///
/// The imm8/imm32 handling replicates PA10-006g's sign-extension trap pattern verbatim.
pub fn encode_or(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src)] => {
            // or r64, r64 → 48 09 /r
            or_reg64_reg64(buf, reg64_from(*dst)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            let dst_reg = reg64_from(*dst)?;
            let imm_i64 = *imm;

            // Mode32: operate on 32-bit register operand, no REX.W
            if inst.mode == paideia_as_ir::InstrMode::Mode32 {
                // Try imm8 form first: 83 /1 ib
                if imm_i64 == (imm_i64 as i8 as i64) {
                    or_reg32_imm8(buf, dst_reg, imm_i64 as i8);
                    return Ok(EncodeOutput::new());
                }

                // Try imm32 form: 81 /1 id
                if imm_i64 == (imm_i64 as i32 as i64) {
                    or_reg32_imm32(buf, dst_reg, imm_i64 as i32);
                    return Ok(EncodeOutput::new());
                }

                // imm out-of-range for 32-bit
                return Err(EncodeError::Unsupported(
                    "or r32, imm: immediate out of 32-bit range",
                ));
            }

            // Mode64: operate on 64-bit register operand, REX.W required
            // Apply the sign-extension trap guard: only use the shorter form
            // if the immediate round-trips correctly through the intermediate type.
            // This prevents silent truncation that would change the semantic meaning.

            // Try imm8 form first: REX.W 83 /1 ib
            // The immediate is sign-extended to 64 bits, so it must round-trip through i8.
            if imm_i64 == (imm_i64 as i8 as i64) {
                or_reg64_imm8(buf, dst_reg, imm_i64 as i8);
                return Ok(EncodeOutput::new());
            }

            // Try imm32 form: REX.W 81 /1 id
            // The immediate is sign-extended to 64 bits, so it must round-trip through i32.
            if imm_i64 == (imm_i64 as i32 as i64) {
                or_reg64_imm32(buf, dst_reg, imm_i64 as i32);
                return Ok(EncodeOutput::new());
            }

            // imm64 out-of-range: x86_64 has no OR r/m64, imm64 form
            Err(EncodeError::Unsupported(
                "or r64, imm64: x86_64 has no OR r/m64, imm64 form — load to register and use OR r/m64, r64 instead",
            ))
        }
        [
            Operand::Reg(dst),
            Operand::MemSib {
                base,
                index: None,
                scale: _,
                disp,
            },
        ] => {
            // Simplified mem-form: or r64, [base+disp]
            or_reg64_mem_reg64_disp(buf, reg64_from(*dst)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        operands if operands.iter().any(|op| matches!(op, Operand::Var { .. })) => {
            unreachable!("Operand::Var reached encoder — resolve_var_operands pass was skipped")
        }
        _ => Err(EncodeError::Unsupported(
            "or: unsupported operand shape (mem,mem not encodable on x86)",
        )),
    }
}

/// Encode bitwise XOR instruction.
///
/// Operand forms:
/// - [Reg, Reg]: xor r64, r64
/// - [Reg, Imm64]: xor r64, imm (with sign-extension trap guards for imm8/imm32)
/// - [Reg, MemSib]: xor r64, [mem]
pub fn encode_xor(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src)] => {
            // xor r64, r64 → 48 31 /r
            xor_reg64_reg64(buf, reg64_from(*dst)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            let dst_reg = reg64_from(*dst)?;
            let imm_i64 = *imm;

            // Try imm8 form first: REX.W 83 /6 ib
            if imm_i64 == (imm_i64 as i8 as i64) {
                xor_reg64_imm8(buf, dst_reg, imm_i64 as i8);
                return Ok(EncodeOutput::new());
            }

            // Try imm32 form: REX.W 81 /6 id
            if imm_i64 == (imm_i64 as i32 as i64) {
                xor_reg64_imm32(buf, dst_reg, imm_i64 as i32);
                return Ok(EncodeOutput::new());
            }

            // imm64 out-of-range
            Err(EncodeError::Unsupported(
                "xor r64, imm64: x86_64 has no XOR r/m64, imm64 form — load to register and use XOR r/m64, r64 instead",
            ))
        }
        [
            Operand::Reg(dst),
            Operand::MemSib {
                base,
                index: None,
                scale: _,
                disp,
            },
        ] => {
            // Simplified mem-form: xor r64, [base+disp]
            xor_reg64_mem_reg64_disp(buf, reg64_from(*dst)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        operands if operands.iter().any(|op| matches!(op, Operand::Var { .. })) => {
            unreachable!("Operand::Var reached encoder — resolve_var_operands pass was skipped")
        }
        _ => Err(EncodeError::Unsupported(
            "xor: unsupported operand shape (mem,mem not encodable on x86)",
        )),
    }
}
