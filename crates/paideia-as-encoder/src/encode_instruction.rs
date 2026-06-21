//! Mnemonic ↔ encoder bridge.
//!
//! `encode_instruction(inst, &mut buf)` dispatches to the per-mnemonic
//! encoder primitives already shipping in encode.rs. Phase-3-m2-002
//! minimum: covers the 10-mnemonic catalog from instruction.rs; future
//! mnemonics drop into the match arm.

use crate::encode::*;
use paideia_as_ir::{Cond as IrCond, Instruction, Mnemonic, Operand, RegId, Scale};

/// Whether a 64-bit ADD with the given operand can be shortened to 32-bit.
///
/// True when the high 32 bits are known to be zero/unused (e.g., the
/// 32-bit form clears the high bits implicitly).
fn can_shorten_add_to_32bit(high_bits_used: bool) -> bool {
    !high_bits_used
}

/// Whether a Jcc rel32 can be shortened to rel8.
///
/// rel8 range: -128..=127 from the byte AFTER the jcc.
fn can_use_rel8(displacement: i64) -> bool {
    (-128..=127).contains(&displacement)
}

/// Statistics about instruction encoding, tracking tightening opportunities.
#[derive(Debug, Clone, Copy, Default)]
pub struct EncodeStats {
    /// Number of instructions tightened (used shorter encoding form).
    pub tightened: usize,
    /// Total number of instructions encoded.
    pub total: usize,
}

impl EncodeStats {
    /// Create a new empty stats structure.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a tightening event.
    pub fn record_tightening(&mut self) {
        self.tightened += 1;
    }

    /// Increment total instruction count.
    pub fn record_instruction(&mut self) {
        self.total += 1;
    }
}

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
///
/// Returns `Ok(stats)` with encoding statistics on success, or an error if encoding fails.
pub fn encode_instruction(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<(), EncodeError> {
    stats.record_instruction();
    match &inst.mnemonic {
        Mnemonic::Mov => encode_mov(inst, buf),
        Mnemonic::Add => encode_add(inst, buf, stats),
        Mnemonic::Sub => encode_sub(inst, buf),
        Mnemonic::Cmp => encode_cmp(inst, buf),
        Mnemonic::Jcc(cond) => encode_jcc(*cond, inst, buf, stats),
        Mnemonic::Jmp => encode_jmp(inst, buf),
        Mnemonic::Call => encode_call(inst, buf),
        Mnemonic::Ret => encode_ret(inst, buf),
        Mnemonic::RepMovsb => encode_rep_movsb(inst, buf),
        Mnemonic::Lea => encode_lea(inst, buf),
        // Phase-5 m2-002: zero-operand control + sync instructions
        Mnemonic::Cli => encode_cli(inst, buf),
        Mnemonic::Sti => encode_sti(inst, buf),
        Mnemonic::Hlt => encode_hlt(inst, buf),
        Mnemonic::Nop => encode_nop(inst, buf),
        Mnemonic::Swapgs => encode_swapgs(inst, buf),
        Mnemonic::Cpuid => encode_cpuid(inst, buf),
        Mnemonic::In { width } => encode_in(inst, buf, *width),
        Mnemonic::Out { width } => encode_out(inst, buf, *width),
        Mnemonic::Wrmsr => encode_wrmsr_inst(inst, buf),
        Mnemonic::Rdmsr => encode_rdmsr_inst(inst, buf),
        Mnemonic::Int => encode_int(inst, buf),
        Mnemonic::MovCr { write } => encode_mov_cr_inst(inst, buf, *write),
        Mnemonic::MovDr { write } => encode_mov_dr_inst(inst, buf, *write),
        Mnemonic::Lgdt => Err(EncodeError::Unsupported("phase-5 m2-007")),
        Mnemonic::Lidt => Err(EncodeError::Unsupported("phase-5 m2-007")),
        Mnemonic::Iret => Err(EncodeError::Unsupported("phase-5 m2-008")),
        Mnemonic::Iretq => Err(EncodeError::Unsupported("phase-5 m2-008")),
        Mnemonic::Sysret => Err(EncodeError::Unsupported("phase-5 m2-008")),
        Mnemonic::RepStosq => Err(EncodeError::Unsupported("phase-5 m2-009")),
        Mnemonic::FarJmp => Err(EncodeError::Unsupported("phase-5 m2-010")),
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

fn encode_add(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<(), EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // add r64, r64 → 48 01 <ModR/M>
            add_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let dest_reg = reg64_from(*dest)?;
            let imm_i64 = *imm;

            // Consult can_shorten_add_to_32bit: if the high 32 bits are zero/unused,
            // use 32-bit immediate form instead of 64-bit
            if can_shorten_add_to_32bit(false)
                && imm_i64 >= i32::MIN as i64
                && imm_i64 <= i32::MAX as i64
            {
                // High bits are not used and value fits in i32: use 32-bit form
                let imm_i32 = imm_i64 as i32;

                // Further tighten: if imm fits in i8, use 8-bit form for even shorter encoding
                if (-128..=127).contains(&imm_i32) {
                    add_reg64_imm8(buf, dest_reg, imm_i32 as i8);
                    stats.record_tightening();
                } else {
                    add_reg64_imm32(buf, dest_reg, imm_i32);
                    stats.record_tightening();
                }
            } else {
                // Value requires full 64-bit immediate: use mov + add pattern
                // For now, return unsupported as phase-3-m2-002 doesn't have this
                return Err(EncodeError::Unsupported(
                    "64-bit immediate add not yet supported",
                ));
            }
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
    stats: &mut EncodeStats,
) -> Result<(), EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // jcc can be encoded as rel32 or rel8 depending on displacement
            let cond = cond_from(ir_cond)?;
            let disp = *rel;

            // Consult can_use_rel8: if displacement fits in signed byte, use shorter encoding
            if can_use_rel8(disp) {
                // Use rel8 form (saves 4 bytes: 0x0F 0x8X <rel32> → 0x7X <rel8>)
                jcc_rel8(buf, cond, disp as i8);
                stats.record_tightening();
            } else {
                // Use rel32 form (standard 6-byte encoding)
                jcc_rel32(buf, cond, disp as i32);
            }
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

fn encode_cli(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Cli,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0xFA);
    Ok(())
}

fn encode_sti(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Sti,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0xFB);
    Ok(())
}

fn encode_hlt(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Hlt,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0xF4);
    Ok(())
}

fn encode_nop(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Nop,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x90);
    Ok(())
}

fn encode_swapgs(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Swapgs,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x81); // sentinel for SWAPGS
    Ok(())
}

fn encode_cpuid(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Cpuid,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_zero_operand(buf, 0x82); // sentinel for CPUID
    Ok(())
}

fn encode_in(inst: &Instruction, buf: &mut CodeBuffer, width: u8) -> Result<(), EncodeError> {
    // `in` expects exactly 1 operand: the data register (al/ax/eax, encoded as Rax)
    if inst.operands.len() != 1 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::In { width },
            expected: 1,
            got: inst.operands.len(),
        });
    }

    // Verify the operand is Rax (al, ax, or eax depending on width)
    match &inst.operands[0] {
        Operand::Reg(reg) => {
            if *reg != RegId(0) {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::In { width },
                });
            }
        }
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::In { width },
            });
        }
    }

    encode_in_dx(buf, width);
    Ok(())
}

fn encode_out(inst: &Instruction, buf: &mut CodeBuffer, width: u8) -> Result<(), EncodeError> {
    // `out` expects exactly 1 operand: the data register (al/ax/eax, encoded as Rax)
    if inst.operands.len() != 1 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Out { width },
            expected: 1,
            got: inst.operands.len(),
        });
    }

    // Verify the operand is Rax (al, ax, or eax depending on width)
    match &inst.operands[0] {
        Operand::Reg(reg) => {
            if *reg != RegId(0) {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::Out { width },
                });
            }
        }
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Out { width },
            });
        }
    }

    encode_out_dx(buf, width);
    Ok(())
}

fn encode_wrmsr_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    // wrmsr expects exactly 0 operands (MSR index in ECX, value in EDX:EAX)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Wrmsr,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_wrmsr(buf);
    Ok(())
}

fn encode_rdmsr_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    // rdmsr expects exactly 0 operands (MSR index in ECX, result in EDX:EAX)
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Rdmsr,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_rdmsr(buf);
    Ok(())
}

fn encode_int(inst: &Instruction, buf: &mut CodeBuffer) -> Result<(), EncodeError> {
    // int expects exactly 1 operand: an immediate value that fits in u8
    match inst.operands.as_slice() {
        [Operand::Imm64(imm)] => {
            // Check that the operand fits in u8
            if *imm > u8::MAX as i64 {
                return Err(EncodeError::Unsupported("int operand > u8"));
            }
            encode_int_imm8(buf, *imm as u8);
            Ok(())
        }
        _ => Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Int,
            expected: 1,
            got: inst.operands.len(),
        }),
    }
}

fn encode_mov_cr_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<(), EncodeError> {
    // mov cr_idx, gpr (write=true): first=CR, second=GPR
    // mov gpr, cr_idx (write=false): first=GPR, second=CR
    match inst.operands.as_slice() {
        [Operand::Reg(first_reg), Operand::Reg(second_reg)] => {
            let (cr_idx, gpr_idx) = if write {
                // mov cr_idx, gpr: first is CR, second is GPR
                (first_reg.0, second_reg.0)
            } else {
                // mov gpr, cr_idx: first is GPR, second is CR
                (second_reg.0, first_reg.0)
            };

            // Validate CR index: phase-5 supports CR0..CR4 + CR8 only
            match cr_idx {
                0 | 3 | 4 | 8 => {}
                _ => {
                    return Err(EncodeError::Unsupported("CR index not in phase-5 minimum"));
                }
            }

            // Validate GPR index: must be 0-15
            if gpr_idx > 15 {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::MovCr { write },
                });
            }

            // Emit the instruction using the low-level encoder
            encode_mov_cr(buf, write, cr_idx, gpr_idx);
            Ok(())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::MovCr { write },
        }),
    }
}

fn encode_mov_dr_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<(), EncodeError> {
    // mov dr_idx, gpr (write=true): first=DR, second=GPR
    // mov gpr, dr_idx (write=false): first=GPR, second=DR
    match inst.operands.as_slice() {
        [Operand::Reg(first_reg), Operand::Reg(second_reg)] => {
            let (dr_idx, gpr_idx) = if write {
                // mov dr_idx, gpr: first is DR, second is GPR
                (first_reg.0, second_reg.0)
            } else {
                // mov gpr, dr_idx: first is GPR, second is DR
                (second_reg.0, first_reg.0)
            };

            // Validate DR index: phase-5 supports DR0..DR7 only
            if dr_idx > 7 {
                return Err(EncodeError::Unsupported(
                    "DR index > 7 not supported in phase-5",
                ));
            }

            // Validate GPR index: must be 0-15
            if gpr_idx > 15 {
                return Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::MovDr { write },
                });
            }

            // Emit the instruction using the low-level encoder
            encode_mov_dr(buf, write, dr_idx, gpr_idx);
            Ok(())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::MovDr { write },
        }),
    }
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

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

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

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

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

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

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

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

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

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

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

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

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

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

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

        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_err());
        match result {
            Err(EncodeError::Unsupported(_)) => {}
            _ => panic!("expected Unsupported error"),
        }
    }

    // ── Tightened instruction encoding tests ────────────────────

    #[test]
    fn encode_add_with_small_imm_uses_8bit_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(42)],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use 8-bit immediate form (4 bytes: REX.W 83 /0 imm8)
        assert_eq!(buf.len(), 4);
        assert_eq!(stats.tightened, 1, "Expected one tightening for small imm8");

        // Verify with iced
        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Add);
    }

    #[test]
    fn encode_add_with_imm_fitting_in_i32_uses_32bit_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Add,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Imm64(0x1000)],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use 32-bit immediate form (7 bytes: REX.W 81 /0 imm32)
        assert_eq!(buf.len(), 7);
        assert_eq!(stats.tightened, 1, "Expected one tightening for i32 imm");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Add);
    }

    #[test]
    fn encode_jcc_with_rel8_disp_uses_rel8_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(paideia_as_ir::Cond::Eq),
            operands: smallvec::smallvec![Operand::Imm64(50)],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use rel8 form (2 bytes: 0x74 disp8)
        assert_eq!(buf.len(), 2);
        assert_eq!(stats.tightened, 1, "Expected one tightening for rel8 Jcc");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Je);
    }

    #[test]
    fn encode_jcc_with_large_disp_uses_rel32_form() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(paideia_as_ir::Cond::Ne),
            operands: smallvec::smallvec![Operand::Imm64(0x1000)],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should use rel32 form (6 bytes: 0x0F 0x85 disp32)
        assert_eq!(buf.len(), 6);
        assert_eq!(stats.tightened, 0, "Expected no tightening for large disp");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jne);
    }

    #[test]
    fn encode_stats_counts_tightening() {
        let mut stats = EncodeStats::new();
        assert_eq!(stats.tightened, 0);
        assert_eq!(stats.total, 0);

        stats.record_instruction();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.tightened, 0);

        stats.record_tightening();
        assert_eq!(stats.tightened, 1);
        assert_eq!(stats.total, 1);

        stats.record_instruction();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.tightened, 1);
    }

    // ── Phase-5 m2-002: zero-operand control + sync instructions ────────

    #[test]
    fn encode_nop_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Nop,
            operands: smallvec::smallvec![],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x90]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Nop);
    }

    #[test]
    fn encode_hlt_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Hlt,
            operands: smallvec::smallvec![],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xF4]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Hlt);
    }

    #[test]
    fn encode_cli_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Cli,
            operands: smallvec::smallvec![],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xFA]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cli);
    }

    #[test]
    fn encode_sti_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sti,
            operands: smallvec::smallvec![],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xFB]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sti);
    }

    #[test]
    fn encode_swapgs_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Swapgs,
            operands: smallvec::smallvec![],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0xF8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Swapgs);
    }

    #[test]
    fn encode_cpuid_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Cpuid,
            operands: smallvec::smallvec![],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0xA2]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cpuid);
    }

    // ── I/O port instruction tests (phase-5 m2-003) ──────────────

    #[test]
    fn encode_in_al_dx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::In { width: 1 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // al
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xEC]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_in_ax_dx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::In { width: 2 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // ax
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x66, 0xED]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_in_eax_dx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::In { width: 4 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // eax
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xED]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_out_dx_al_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Out { width: 1 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // al
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xEE]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    #[test]
    fn encode_out_dx_ax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Out { width: 2 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // ax
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x66, 0xEF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    #[test]
    fn encode_out_dx_eax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Out { width: 4 },
            operands: smallvec::smallvec![Operand::Reg(RegId(0))], // eax
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xEF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    // ── Phase-5 m2-004: MSR and interrupt instructions ────────────

    #[test]
    fn encode_wrmsr_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Wrmsr,
            operands: smallvec::smallvec![],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x30]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Wrmsr);
    }

    #[test]
    fn encode_rdmsr_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Rdmsr,
            operands: smallvec::smallvec![],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x32]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Rdmsr);
    }

    #[test]
    fn encode_int_0x20_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Int,
            operands: smallvec::smallvec![Operand::Imm64(0x20)],
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0xCD, 0x20]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Int);
    }

    // ── Phase-5 m2-005: control register MOV instruction encoding ────────

    // Write (mov cr_idx, rax) tests via encode_instruction dispatcher
    #[test]
    fn encode_instruction_mov_cr0_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(0))], // mov cr0, rax
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr3_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(3)), Operand::Reg(RegId(0))], // mov cr3, rax
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xD8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr4_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(4)), Operand::Reg(RegId(0))], // mov cr4, rax
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xE0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr8_rax_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(8)), Operand::Reg(RegId(0))], // mov cr8, rax
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x22, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_cr2_write_fails_validation() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: true },
            operands: smallvec::smallvec![Operand::Reg(RegId(2)), Operand::Reg(RegId(0))], // mov cr2, rax (not supported)
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_err(), "CR2 should not be supported in phase-5");
    }

    // Read (mov rax, cr_idx) tests via encode_instruction dispatcher
    #[test]
    fn encode_instruction_mov_rax_cr0_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(0))], // mov rax, cr0
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr3_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(3))], // mov rax, cr3
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xD8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr4_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(4))], // mov rax, cr4
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xE0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr8_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(8))], // mov rax, cr8
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x20, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_instruction_mov_rax_cr2_fails_validation() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::MovCr { write: false },
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(2))], // mov rax, cr2 (not supported)
            encoding_hint: None,
        };

        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_err(), "CR2 should not be supported in phase-5");
    }
}
