//! Encoder for IMUL (multiply) instruction (PA10-003).
//!
//! IMUL supports three operand forms:
//! - [Reg, Reg]: 2-operand imul r64, r64 → 48 0F AF /r
//! - [Reg, Reg, Imm64]: 3-operand imul r64, r64, imm → 48 6B/69 /r id/ib
//! - [Reg, MemSib]: 2-operand with memory → 48 0F AF /r [mem]
//!
//! CRITICAL CAVEAT: IMUL ModR/M layout is INVERTED from standard instructions.
//! - Standard r/m←r: ModR/M = 0xC0 | (src<<3) | dst, REX.R on src, REX.B on dst
//! - IMUL (inverted): ModR/M = 0xC0 | (dst<<3) | src, REX.R on dst, REX.B on src
//!
//! Sign-extension trap guards apply to imm8/imm32 forms: immediate must round-trip
//! through the intermediate type (i8/i32) to avoid semantic changes.

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

/// Encode IMUL (multiply) instruction.
///
/// Operand forms:
/// - [Reg, Reg]: imul r64, r64 (2-operand)
/// - [Reg, Reg, Imm64]: imul r64, r64, imm (3-operand with sign-extension trap guards)
/// - [Reg, MemSib]: imul r64, [mem] (2-operand with memory)
///
/// Note: The parser may not yet produce [Reg, Reg, Imm64] for 3-operand IMUL;
/// if so, the tests will construct Instruction directly per PA10-XXX follow-up.
pub fn encode_imul(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Reg(src)] => {
            // imul r64, r64 → 48 0F AF /r (2-operand form)
            imul_reg64_reg64(buf, reg64_from(*dst)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dst), Operand::Reg(src), Operand::Imm64(imm)] => {
            // imul r64, r64, imm → 48 6B/69 /r id/ib (3-operand form)
            let dst_reg = reg64_from(*dst)?;
            let src_reg = reg64_from(*src)?;
            let imm_i64 = *imm;

            // Try imm8 form first: REX.W 6B /r ib
            if imm_i64 == (imm_i64 as i8 as i64) {
                imul_reg64_reg64_imm8(buf, dst_reg, src_reg, imm_i64 as i8);
                return Ok(EncodeOutput::new());
            }

            // Try imm32 form: REX.W 69 /r id
            if imm_i64 == (imm_i64 as i32 as i64) {
                imul_reg64_reg64_imm32(buf, dst_reg, src_reg, imm_i64 as i32);
                return Ok(EncodeOutput::new());
            }

            // imm64 out-of-range
            Err(EncodeError::Unsupported(
                "imul r64, r64, imm64: immediate out of range for 32-bit sign extension",
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
            // Simplified mem-form: imul r64, [base+disp] (no index, no scale in phase-1)
            imul_reg64_mem_reg64_disp(buf, reg64_from(*dst)?, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::MemSib { .. }, Operand::MemSib { .. }] => {
            // Explicit error for mem,mem case
            Err(EncodeError::Unsupported("imul mem,mem"))
        }
        operands if operands.iter().any(|op| matches!(op, Operand::Var { .. })) => {
            unreachable!("Operand::Var reached encoder — resolve_var_operands pass was skipped")
        }
        _ => Err(EncodeError::Unsupported("imul: unsupported operand shape")),
    }
}
