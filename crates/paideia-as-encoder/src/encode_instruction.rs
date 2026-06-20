//! Mnemonic ↔ encoder bridge.
//!
//! `encode_instruction(inst, &mut buf)` dispatches to the per-mnemonic
//! encoder primitives already shipping in encode.rs. Phase-3-m2-002
//! minimum: covers the 10-mnemonic catalog from instruction.rs; future
//! mnemonics drop into the match arm.

use crate::encode::*;
use paideia_as_ir::{Cond as IrCond, Instruction, Mnemonic, Operand, RegId, Scale};

#[derive(Debug, thiserror::Error)]
/// Errors that can occur during instruction encoding.
pub enum EncodeError {
    /// Operand count mismatch for a mnemonic.
    #[error("operand mismatch for {mnemonic:?}: expected {expected}, got {got}")]
    OperandCount {
        /// The mnemonic that had the operand count mismatch.
        mnemonic: Mnemonic,
        /// Expected operand count.
        expected: usize,
        /// Actual operand count.
        got: usize,
    },
    /// Operand shape mismatch for a mnemonic.
    #[error("operand shape mismatch for {mnemonic:?}")]
    OperandShape {
        /// The mnemonic that had the operand shape mismatch.
        mnemonic: Mnemonic,
    },
    /// Feature not yet supported in phase 3 m2-002.
    #[error("unsupported in phase 3 m2-002: {0}")]
    Unsupported(&'static str),
}

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

/// Convert an IR Scale to a numeric byte width for indexed loads.
fn scale_to_bytes(scale: Scale) -> u32 {
    match scale {
        Scale::X1 => 1,
        Scale::X2 => 2,
        Scale::X4 => 4,
        Scale::X8 => 8,
    }
}

/// Convert an IR Cond to an encoder Cond.
fn cond_from(ir_cond: IrCond) -> Result<Cond, EncodeError> {
    match ir_cond {
        IrCond::Eq => Ok(Cond::Eq),
        IrCond::Ne => Ok(Cond::Neq),
        IrCond::Lt => Ok(Cond::Lt),
        IrCond::Ge => Ok(Cond::Ge),
        IrCond::Le => Ok(Cond::Le),
        IrCond::Gt => Ok(Cond::Gt),
        _ => Err(EncodeError::Unsupported(
            "conditional code not in phase-3-m2-002 minimum",
        )),
    }
}

/// Dispatch an Instruction to its mnemonic-specific encoder.
pub fn encode_instruction(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    match &inst.mnemonic {
        Mnemonic::Mov => encode_mov(inst, buf),
        Mnemonic::Add => encode_add(inst, buf),
        Mnemonic::Sub => encode_sub(inst, buf),
        Mnemonic::Cmp => encode_cmp(inst, buf),
        Mnemonic::Jcc(cond) => encode_jcc(*cond, inst, buf),
        Mnemonic::Jmp => encode_jmp(inst, buf),
        Mnemonic::Call => encode_call(inst, buf),
        Mnemonic::Ret => encode_ret(inst, buf),
        Mnemonic::RepMovsb => encode_rep_movsb(inst, buf),
        Mnemonic::Lea => encode_lea(inst, buf),
    }
}

fn encode_mov(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // mov r64, r64 → 48 89 <ModR/M>
            mov_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            // mov r64, imm64 → REX.W B8+rd <imm64>
            mov_reg64_imm64(buf, reg64_from(*dest)?, *imm as u64);
            Ok(())
        }
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: Some(index),
                scale,
                disp: 0,
            },
        ] => {
            // mov r64, [base + index * scale]
            emit_indexed_load(
                buf,
                reg64_from(*dest)?,
                reg64_from(*base)?,
                reg64_from(*index)?,
                scale_to_bytes(*scale),
                false,
            );
            Ok(())
        }
        _ => Err(EncodeError::Unsupported(
            "mov form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_add(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // add r64, r64 → 48 01 <ModR/M>
            add_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(())
        }
        _ => Err(EncodeError::Unsupported(
            "add form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_sub(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // sub r64, r64 → 48 29 <ModR/M>
            sub_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(())
        }
        _ => Err(EncodeError::Unsupported(
            "sub form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_cmp(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // cmp r64, r64 → 48 39 <ModR/M>
            cmp_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(())
        }
        _ => Err(EncodeError::Unsupported(
            "cmp form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_jcc(
    ir_cond: IrCond,
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<(), EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // jcc rel32 → 0F 8X <rel32>
            let cond = cond_from(ir_cond)?;
            jcc_rel32(buf, cond, *rel as i32);
            Ok(())
        }
        _ => Err(EncodeError::Unsupported(
            "jcc form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_jmp(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // jmp rel32 → E9 <rel32>
            jmp_rel32(buf, *rel as i32);
            Ok(())
        }
        _ => Err(EncodeError::Unsupported(
            "jmp form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_call(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // call rel32 → E8 <rel32>
            call_rel32(buf, *rel as i32);
            Ok(())
        }
        _ => Err(EncodeError::Unsupported(
            "call form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_ret(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Ret,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    ret(buf);
    Ok(())
}

fn encode_rep_movsb(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::RepMovsb,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    buf.bytes.push(0xF3);
    buf.bytes.push(0xA4); // rep movsb
    Ok(())
}

fn encode_lea(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    match inst.operands.as_slice() {
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // lea r64, [base + disp]
            // LEA uses MOV encoding but with different semantics
            // lea r64, [rbp + disp] → 48 8D /r [ModR/M] [disp]
            let dest_id = reg64_from(*dest)? as u8;
            let base_id = reg64_from(*base)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, (base_id >> 3) != 0);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode

            if (-128..=127).contains(disp) {
                // Use mod=01, disp8
                buf.bytes.push(0x40 | ((dest_id & 7) << 3) | (base_id & 7));
                buf.bytes.push(*disp as u8);
            } else {
                // Use mod=10, disp32
                buf.bytes.push(0x80 | ((dest_id & 7) << 3) | (base_id & 7));
                buf.bytes.extend(disp.to_le_bytes());
            }
            Ok(())
        }
        _ => Err(EncodeError::Unsupported(
            "lea form not in phase-3-m2-002 minimum",
        )),
    }
}

// Helper to emit a REX prefix byte (copied from encode.rs for use in encode_lea).
fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | (u8::from(w) << 3) | (u8::from(r) << 2) | (u8::from(x) << 1) | u8::from(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paideia_as_ir::{Instruction, Mnemonic, Operand, RegId, Scale};

    #[test]
    fn encode_mov_rax_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(7))],
            encoding_hint: None,
        };

        encode_instruction(&inst, &mut buf).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_rax_imm64_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)),
                Operand::Imm64(0x1234567890ABCDEF)
            ],
            encoding_hint: None,
        };

        encode_instruction(&inst, &mut buf).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_add_rax_rdi_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(7))],
            encoding_hint: None,
        };

        encode_instruction(&inst, &mut buf).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Add);
    }

    #[test]
    fn encode_sub_rax_rdi_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sub,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(7))],
            encoding_hint: None,
        };

        encode_instruction(&inst, &mut buf).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sub);
    }

    #[test]
    fn encode_ret_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Ret,
            operands: smallvec::smallvec![],
            encoding_hint: None,
        };

        encode_instruction(&inst, &mut buf).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Ret);
    }

    #[test]
    fn encode_rep_movsb_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepMovsb,
            operands: smallvec::smallvec![],
            encoding_hint: None,
        };

        encode_instruction(&inst, &mut buf).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Movsb);
    }

    #[test]
    fn encode_indexed_load_via_mov_dispatches_correctly() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::MemSib {
                    base: RegId(6),        // rsi
                    index: Some(RegId(7)), // rdi
                    scale: Scale::X8,
                    disp: 0,
                }
            ],
            encoding_hint: None,
        };

        encode_instruction(&inst, &mut buf).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_unsupported_mov_shape_returns_error() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)),
                Operand::MemDisp { disp: 0x1000 },
            ],
            encoding_hint: None,
        };

        let result = encode_instruction(&inst, &mut buf);
        assert!(result.is_err());
        match result {
            Err(EncodeError::Unsupported(_)) => {}
            _ => panic!("expected Unsupported error"),
        }
    }
}
