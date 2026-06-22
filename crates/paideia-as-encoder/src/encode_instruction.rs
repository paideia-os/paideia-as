//! Mnemonic ↔ encoder bridge.
//!
//! `encode_instruction(inst, &mut buf)` dispatches to the per-mnemonic
//! encoder primitives already shipping in encode.rs. Phase-3-m2-002
//! minimum: covers the 10-mnemonic catalog from instruction.rs; future
//! mnemonics drop into the match arm.

use crate::dispatch::{DispatchKind, classify};
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

/// Kind of relocation for a symbol reference.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RelocKind {
    /// PC-relative 32-bit relocation (x86_64 R_X86_64_PC32).
    PcRel32,
    /// PLT 32-bit relocation (x86_64 R_X86_64_PLT32).
    Plt32,
    /// Absolute 64-bit relocation (x86_64 R_X86_64_64).
    Abs64,
}

/// A relocation site in the encoded instruction stream.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RelocSite {
    /// Byte offset into the instruction stream where the relocation applies.
    pub byte_offset: u32,
    /// Name of the symbol being referenced.
    pub symbol: String,
    /// Kind of relocation to apply.
    pub kind: RelocKind,
    /// Addend to apply to the symbol address.
    pub addend: i32,
}

/// Phase 6 m4-003: A label fixup site in the encoded instruction stream.
/// Records where a Jcc or Jmp instruction references a label (forward or backward),
/// allowing the linker to patch the rel32 displacement after all labels are resolved.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct LabelFixup {
    /// Byte offset into the instruction stream where the rel32 placeholder is located.
    pub byte_offset: u32,
    /// Name of the target label.
    pub label_name: String,
    /// Addend to apply to the label offset (typically 0).
    pub addend: i32,
    /// Size of the instruction (5 for jmp, 6 for jcc).
    pub instruction_size: u32,
}

/// Output from encoding an instruction, including relocation sites and label fixups.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EncodeOutput {
    /// Relocation sites to be processed by the linker.
    pub reloc_sites: Vec<RelocSite>,
    /// Label fixup sites for Jcc/Jmp instructions (phase 6 m4-003).
    pub label_fixups: Vec<LabelFixup>,
}

impl EncodeOutput {
    /// Create a new empty output.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a relocation site to the output.
    pub fn add_reloc(&mut self, site: RelocSite) {
        self.reloc_sites.push(site);
    }

    /// Phase 6 m4-003: Add a label fixup site to the output.
    pub fn add_label_fixup(&mut self, fixup: LabelFixup) {
        self.label_fixups.push(fixup);
    }
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
        IrCond::Below => Ok(Cond::Below),
        IrCond::BelowOrEqual => Ok(Cond::BelowOrEqual),
        IrCond::Above => Ok(Cond::Above),
        IrCond::AboveOrEqual => Ok(Cond::AboveOrEqual),
        IrCond::Zero => Ok(Cond::Eq),     // jz is alias for je (0x84)
        IrCond::NonZero => Ok(Cond::Neq), // jnz is alias for jne (0x85)
        IrCond::Sign => Ok(Cond::Sign),
        IrCond::NotSign => Ok(Cond::NotSign),
        IrCond::Overflow => Ok(Cond::Overflow),
        IrCond::NotOverflow => Ok(Cond::NotOverflow),
    }
}

/// Dispatch an Instruction to its mnemonic-specific encoder.
///
/// Returns `Ok(EncodeOutput)` with encoding output (including relocation sites) on success, or an error if encoding fails.
pub fn encode_instruction(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<EncodeOutput, EncodeError> {
    stats.record_instruction();
    match &inst.mnemonic {
        Mnemonic::Mov => encode_mov(inst, buf),
        Mnemonic::Add => encode_add(inst, buf, stats),
        Mnemonic::Sub => encode_sub(inst, buf),
        Mnemonic::Cmp => encode_cmp(inst, buf),
        Mnemonic::Test => encode_test(inst, buf),
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
        Mnemonic::Lgdt => encode_lgdt_inst(inst, buf),
        Mnemonic::Lidt => encode_lidt_inst(inst, buf),
        Mnemonic::Iret => encode_iret_inst(inst, buf),
        Mnemonic::Iretq => encode_iretq_inst(inst, buf),
        Mnemonic::Sysret => encode_sysret_inst(inst, buf),
        Mnemonic::Syscall => encode_syscall_inst(inst, buf),
        Mnemonic::RepStosq => encode_rep_stosq_inst(inst, buf),
        Mnemonic::FarJmp => encode_far_jmp_inst(inst, buf),
        Mnemonic::Movzx => encode_movzx(inst, buf),
    }
}

fn encode_mov(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase 6, m1-002 & m1-003: Classify MOV operands and dispatch to specialized encoders.
    let dispatch_kind = classify(inst);

    // Route CR moves through encode_mov_cr_dispatcher.
    match dispatch_kind {
        DispatchKind::MovToCr => {
            return encode_mov_cr_dispatcher(inst, buf, true);
        }
        DispatchKind::MovFromCr => {
            return encode_mov_cr_dispatcher(inst, buf, false);
        }
        // Phase 6, m1-003: Route DR moves through encode_mov_dr_dispatcher.
        DispatchKind::MovToDr => {
            return encode_mov_dr_dispatcher(inst, buf, true);
        }
        DispatchKind::MovFromDr => {
            return encode_mov_dr_dispatcher(inst, buf, false);
        }
        // All other dispatch kinds (MovGeneric, Generic) fall through to the rest of this function.
        _ => {}
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // mov r64, r64 → 48 89 <ModR/M>
            mov_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            // mov r64, imm64 → REX.W B8+rd <imm64>
            mov_reg64_imm64(buf, reg64_from(*dest)?, *imm as u64);
            Ok(EncodeOutput::new())
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
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::SymbolRef { name, addend }] => {
            // mov r64, [symbol + addend] → 48 8B /r [rip-relative ModR/M] [disp32_placeholder]
            let dest_id = reg64_from(*dest)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8B); // mov r64, r/m64 opcode

            // RIP-relative addressing: mod=00, r/m=5
            buf.bytes.push(0x05 | ((dest_id & 7) << 3)); // ModR/M with rip-relative form

            let reloc_offset = buf.bytes.len() as u32;
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: reloc_offset,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: *addend,
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "mov form not in phase-3-m2-002 minimum",
        )),
    }
}

/// Dispatcher for MOV to/from control register (Phase 6, m1-002).
///
/// Extracts CR and GPR indices from operands and routes to encode_mov_cr.
/// - write=true: mov cr_idx, gpr (destination is CR, source is GPR)
/// - write=false: mov gpr, cr_idx (destination is GPR, source is CR)
fn encode_mov_cr_dispatcher(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(first), Operand::Reg(second)] => {
            let (cr_id, gpr_id) = if write {
                // mov cr, gpr: first is CR, second is GPR
                (first.0, second.0)
            } else {
                // mov gpr, cr: first is GPR, second is CR
                (second.0, first.0)
            };

            // Convert CR ID to CR index: cr_idx = RegId - 16
            let cr_idx = cr_id - 16;

            // GPR index is directly the reg_id (0-15)
            let gpr_idx = gpr_id;

            // Encode using the low-level helper
            encode_mov_cr(buf, write, cr_idx, gpr_idx);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Mov,
        }),
    }
}

/// Dispatcher for MOV to/from debug register (Phase 6, m1-003).
///
/// Extracts DR and GPR indices from operands and routes to encode_mov_dr.
/// - write=true: mov dr_idx, gpr (destination is DR, source is GPR)
/// - write=false: mov gpr, dr_idx (destination is GPR, source is DR)
fn encode_mov_dr_dispatcher(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    write: bool,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(first), Operand::Reg(second)] => {
            let (dr_id, gpr_id) = if write {
                // mov dr, gpr: first is DR, second is GPR
                (first.0, second.0)
            } else {
                // mov gpr, dr: first is GPR, second is DR
                (second.0, first.0)
            };

            // Convert DR ID to DR index: dr_idx = RegId - 25 (compact encoding)
            let dr_idx = dr_id - 25;

            // GPR index is directly the reg_id (0-15)
            let gpr_idx = gpr_id;

            // Encode using the low-level helper
            encode_mov_dr(buf, write, dr_idx, gpr_idx);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Mov,
        }),
    }
}

fn encode_add(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // add r64, r64 → 48 01 <ModR/M>
            add_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
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
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "add form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_sub(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // sub r64, r64 → 48 29 <ModR/M>
            sub_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "sub form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_cmp(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // cmp r64, r64 → 48 39 <ModR/M>
            cmp_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
            Operand::Reg(src),
        ] => {
            // cmp [base + disp], r64 → 48 39 <ModR/M> [disp]
            cmp_mem_reg64_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let dest_reg = reg64_from(*dest)?;
            let imm_i64 = *imm;

            // Determine the best encoding form for the immediate
            if (-128..=127).contains(&imm_i64) {
                // 8-bit immediate: use 83 /7 ib
                cmp_reg64_imm8(buf, dest_reg, imm_i64 as i8);
            } else if imm_i64 >= i32::MIN as i64 && imm_i64 <= i32::MAX as i64 {
                // 32-bit immediate: use 81 /7 id
                cmp_reg64_imm32(buf, dest_reg, imm_i64 as i32);
            } else {
                // imm64 out-of-range: unsupported
                return Err(EncodeError::Unsupported(
                    "cmp imm64 not supported; load into reg first",
                ));
            }
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "cmp shape not in phase-6-m4-001 minimum",
        )),
    }
}

fn encode_test(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase 7 m1-001: test r64, r64 for condition testing.
    // Operands: [register, register] for "test rdi, rdi" shape.
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // test r64, r64 → 48 85 <ModR/M>
            test_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "test shape not in phase-7-m1-001 minimum",
        )),
    }
}

fn encode_jcc(
    ir_cond: IrCond,
    inst: &Instruction,
    buf: &mut CodeBuffer,
    stats: &mut EncodeStats,
) -> Result<EncodeOutput, EncodeError> {
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
            Ok(EncodeOutput::new())
        }
        [Operand::LabelRef { name, addend }] => {
            // Phase 6 m4-003: Label reference (forward or backward).
            // Emit placeholder rel32 and record fixup for linker resolution.
            let cond = cond_from(ir_cond)?;
            let offset_before = buf.len() as u32;

            // Emit jcc rel32 with zero placeholder
            jcc_rel32(buf, cond, 0);

            let mut output = EncodeOutput::new();
            output.add_label_fixup(LabelFixup {
                byte_offset: offset_before + 2, // offset of rel32 (after 0F XX)
                label_name: name.clone(),
                addend: *addend,
                instruction_size: 6,
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "jcc form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_jmp(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // jmp rel32 → E9 <rel32>
            jmp_rel32(buf, *rel as i32);
            Ok(EncodeOutput::new())
        }
        [Operand::LabelRef { name, addend }] => {
            // Phase 6 m4-003: Label reference (forward or backward).
            // Emit placeholder rel32 and record fixup for linker resolution.
            let offset_before = buf.len() as u32;

            // Emit jmp rel32 with zero placeholder
            jmp_rel32(buf, 0);

            let mut output = EncodeOutput::new();
            output.add_label_fixup(LabelFixup {
                byte_offset: offset_before + 1, // offset of rel32 (after E9)
                label_name: name.clone(),
                addend: *addend,
                instruction_size: 5,
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "jmp form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_call(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // call rel32 → E8 <rel32>
            call_rel32(buf, *rel as i32);
            Ok(EncodeOutput::new())
        }
        [Operand::SymbolRef { name, addend }] => {
            // call symbol → E8 <disp32_placeholder> + RelocSite with Plt32
            // Phase 7 m1-001: Use RelocKind::Plt32 for PLT relocations
            // Phase 7 m1-003: Use byte_offset_in_text for precise relocation offset
            // instead of buf.bytes.len(), which can be off-by-one in multi-call bodies.
            let reloc_offset = inst
                .byte_offset_in_text
                .expect("byte_offset_in_text must be set before encoding");
            buf.bytes.push(0xE8); // call rel32 opcode
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: reloc_offset + 1,
                symbol: name.clone(),
                kind: RelocKind::Plt32,
                addend: *addend,
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "call form not in phase-3-m2-002 minimum",
        )),
    }
}

fn encode_ret(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Ret,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    ret(buf);
    Ok(EncodeOutput::new())
}

fn encode_rep_movsb(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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

fn encode_lea(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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
            Ok(EncodeOutput::new())
        }
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: Some(index),
                scale,
                disp,
            },
        ] => {
            // lea r64, [base + index * scale + disp]
            // Uses SIB (Scale-Index-Base) byte format: SIB = scale (2 bits) | index (3 bits) | base (3 bits)
            let dest_id = reg64_from(*dest)? as u8;
            let base_id = reg64_from(*base)? as u8;
            let index_id = reg64_from(*index)? as u8;

            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };

            let rex_byte = rex(
                true,
                (dest_id >> 3) != 0,
                (index_id >> 3) != 0,
                (base_id >> 3) != 0,
            );

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode

            if *disp == 0 {
                // Use mod=00 (no displacement) with SIB
                buf.bytes.push(0x04 | ((dest_id & 7) << 3)); // mod=00, r/m=100 (SIB follows)
                let sib = ((scale_bits & 3) << 6) | ((index_id & 7) << 3) | (base_id & 7);
                buf.bytes.push(sib);
            } else if (-128..=127).contains(disp) {
                // Use mod=01, disp8 with SIB
                buf.bytes.push(0x44 | ((dest_id & 7) << 3)); // mod=01, r/m=100 (SIB follows)
                let sib = ((scale_bits & 3) << 6) | ((index_id & 7) << 3) | (base_id & 7);
                buf.bytes.push(sib);
                buf.bytes.push(*disp as u8);
            } else {
                // Use mod=10, disp32 with SIB
                buf.bytes.push(0x84 | ((dest_id & 7) << 3)); // mod=10, r/m=100 (SIB follows)
                let sib = ((scale_bits & 3) << 6) | ((index_id & 7) << 3) | (base_id & 7);
                buf.bytes.push(sib);
                buf.bytes.extend(disp.to_le_bytes());
            }
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::SymbolRef { name, addend }] => {
            // lea r64, [symbol] → 48 8D /r [rip-relative ModR/M] [disp32_placeholder]
            let dest_id = reg64_from(*dest)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8D); // LEA opcode

            // RIP-relative addressing: mod=00, r/m=5
            buf.bytes.push(0x05 | ((dest_id & 7) << 3)); // ModR/M with rip-relative form

            let reloc_offset = buf.bytes.len() as u32;
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: reloc_offset,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: *addend,
            });
            Ok(output)
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

fn encode_cli(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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

fn encode_sti(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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

fn encode_hlt(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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

fn encode_nop(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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

fn encode_swapgs(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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

fn encode_cpuid(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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

fn encode_in(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: u8,
) -> Result<EncodeOutput, EncodeError> {
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
    Ok(EncodeOutput::new())
}

fn encode_out(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: u8,
) -> Result<EncodeOutput, EncodeError> {
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
    Ok(EncodeOutput::new())
}

fn encode_wrmsr_inst(
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

fn encode_rdmsr_inst(
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

fn encode_int(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // int expects exactly 1 operand: an immediate value that fits in u8
    match inst.operands.as_slice() {
        [Operand::Imm64(imm)] => {
            // Check that the operand fits in u8
            if *imm > u8::MAX as i64 {
                return Err(EncodeError::Unsupported("int operand > u8"));
            }
            encode_int_imm8(buf, *imm as u8);
            Ok(EncodeOutput::new())
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
) -> Result<EncodeOutput, EncodeError> {
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
            Ok(EncodeOutput::new())
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
) -> Result<EncodeOutput, EncodeError> {
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
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::MovDr { write },
        }),
    }
}

fn encode_lgdt_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // lgdt [base + disp] - load GDT descriptor
    match inst.operands.as_slice() {
        [
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // Valid form: [base] with optional displacement, no index
            let base_reg = reg64_from(*base)?;
            encode_descriptor_table_load(buf, base_reg, *disp, 2); // 2 = /2 for lgdt
            Ok(EncodeOutput::new())
        }
        [Operand::SymbolRef { name, addend }] => {
            // lgdt [symbol] → 0F 01 [rip-relative ModR/M] [disp32_placeholder]
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            // RIP-relative addressing: mod=00, /2 for lgdt
            buf.bytes.push(0x15); // 0x05 | (2 << 3) = rip-relative with /2

            let reloc_offset = buf.bytes.len() as u32;
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: reloc_offset,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: *addend,
            });
            Ok(output)
        }
        [
            Operand::MemSib {
                base: _,
                index: Some(_),
                scale: _,
                disp: _,
            },
        ] => {
            // Indexed form not supported
            Err(EncodeError::Unsupported("lgdt/lidt indexed form"))
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Lgdt,
        }),
    }
}

fn encode_lidt_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // lidt [base + disp] - load IDT descriptor
    match inst.operands.as_slice() {
        [
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // Valid form: [base] with optional displacement, no index
            let base_reg = reg64_from(*base)?;
            encode_descriptor_table_load(buf, base_reg, *disp, 3); // 3 = /3 for lidt
            Ok(EncodeOutput::new())
        }
        [Operand::SymbolRef { name, addend }] => {
            // lidt [symbol] → 0F 01 [rip-relative ModR/M] [disp32_placeholder]
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            // RIP-relative addressing: mod=00, /3 for lidt
            buf.bytes.push(0x1D); // 0x05 | (3 << 3) = rip-relative with /3

            let reloc_offset = buf.bytes.len() as u32;
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: reloc_offset,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: *addend,
            });
            Ok(output)
        }
        [
            Operand::MemSib {
                base: _,
                index: Some(_),
                scale: _,
                disp: _,
            },
        ] => {
            // Indexed form not supported
            Err(EncodeError::Unsupported("lgdt/lidt indexed form"))
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Lidt,
        }),
    }
}

fn encode_iret_inst(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // iret expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Iret,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_iret(buf);
    Ok(EncodeOutput::new())
}

fn encode_iretq_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // iretq expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Iretq,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_iretq(buf);
    Ok(EncodeOutput::new())
}

fn encode_sysret_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // sysret expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Sysret,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_sysret(buf);
    Ok(EncodeOutput::new())
}

fn encode_syscall_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // syscall expects exactly 0 operands
    if !inst.operands.is_empty() {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Syscall,
            expected: 0,
            got: inst.operands.len(),
        });
    }
    encode_syscall(buf);
    Ok(EncodeOutput::new())
}

fn encode_rep_stosq_inst(
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

fn encode_far_jmp_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // jmp far expects exactly 1 operand: memory (SIB or RIP-relative)
    if inst.operands.len() != 1 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::FarJmp,
            expected: 1,
            got: inst.operands.len(),
        });
    }

    match &inst.operands[0] {
        Operand::MemSib {
            base,
            index: None,
            scale: Scale::X1,
            disp,
        } => {
            // [base + disp] form
            encode_far_jmp(buf, Some(reg64_from(*base)?), *disp);
            Ok(EncodeOutput::new())
        }
        Operand::MemRipRel { disp } => {
            // [rip + disp32] form
            encode_far_jmp(buf, None, *disp);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::FarJmp,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Movsb);
    }

    #[test]
    fn encode_rep_stosq_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::RepStosq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Verify byte sequence: F3 48 AB
        assert_eq!(buf.as_slice(), &[0xF3, 0x48, 0xAB]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Stosq);
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
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
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        let result = encode_instruction(&inst, &mut buf, &mut stats);
        assert!(result.is_err(), "CR2 should not be supported in phase-5");
    }

    // ── Phase-5 m2-007: descriptor-table load (lgdt/lidt) ────────

    #[test]
    fn encode_lgdt_rdi_disp0_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 17 (3 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x17]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lgdt_rdi_disp8_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 57 08 (4 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x57, 0x08]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lgdt_rdi_disp_neg128_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: -128,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 57 80 (4 bytes, -128 as u8 = 0x80)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x57, 0x80]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lgdt);
    }

    #[test]
    fn encode_lidt_rdi_disp0_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 1F (3 bytes, encoding: 0F 01 /3)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x1F]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    #[test]
    fn encode_lidt_rdi_disp16_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 16,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 5F 10 (4 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x5F, 0x10]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    #[test]
    fn encode_lidt_rdi_disp_neg128_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lidt,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: -128,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 5F 80 (4 bytes)
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x5F, 0x80]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Lidt);
    }

    // ── Phase-5 m2-008: interrupt-return + system-return instructions ────────

    #[test]
    fn encode_iret_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Iret,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: CF (1 byte)
        assert_eq!(buf.as_slice(), &[0xCF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Note: in 64-bit decoder, bare CF is decoded as Iretd (32-bit form)
        assert_eq!(instr.mnemonic(), IcedMnem::Iretd);
    }

    #[test]
    fn encode_iretq_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Iretq,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 CF (2 bytes, REX.W prefix)
        assert_eq!(buf.as_slice(), &[0x48, 0xCF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Iretq);
    }

    #[test]
    fn encode_sysret_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Sysret,
            operands: smallvec::smallvec![],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 0F 07 (3 bytes, REX.W prefix + two-byte opcode)
        assert_eq!(buf.as_slice(), &[0x48, 0x0F, 0x07]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Note: in 64-bit decoder, 48 0F 07 is decoded as Sysretq (64-bit form)
        assert_eq!(instr.mnemonic(), IcedMnem::Sysretq);
    }

    #[test]
    fn encode_far_jmp_mem_rdi_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 FF 2F (3 bytes)
        // 48 = REX.W
        // FF = opcode
        // 2F = ModR/M with mod=00, reg=5, rm=7 (rdi)
        assert_eq!(buf.as_slice(), &[0x48, 0xFF, 0x2F]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    #[test]
    fn encode_far_jmp_mem_rdi_plus_8_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![Operand::MemSib {
                base: RegId(7), // rdi
                index: None,
                scale: Scale::X1,
                disp: 8,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 FF 6F 08 (4 bytes)
        // 48 = REX.W
        // FF = opcode
        // 6F = ModR/M with mod=01, reg=5, rm=7 (rdi + disp8)
        // 08 = disp8
        assert_eq!(buf.as_slice(), &[0x48, 0xFF, 0x6F, 0x08]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    #[test]
    fn encode_far_jmp_mem_rip_relative_round_trips() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::FarJmp,
            operands: smallvec::smallvec![Operand::MemRipRel { disp: 0x1000 }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 FF 2D 00 10 00 00 (7 bytes)
        // 48 = REX.W
        // FF = opcode
        // 2D = ModR/M with mod=00, reg=5, rm=5 (rip-relative marker)
        // 00 10 00 00 = 0x1000 in little-endian
        assert_eq!(buf.as_slice(), &[0x48, 0xFF, 0x2D, 0x00, 0x10, 0x00, 0x00]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jmp);
    }

    // ── Phase-5 m5-002: SymbolRef tests ───────────────────────────────

    #[test]
    fn encode_lea_rax_symbol_ref_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lea,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::SymbolRef {
                    name: "gdt_descriptor".to_string(),
                    addend: 0,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8D 05 00 00 00 00 (7 bytes)
        // 48 = REX.W
        // 8D = LEA opcode
        // 05 = ModR/M with mod=00, reg=0 (rax), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x48, 0x8D, 0x05, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "gdt_descriptor");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, 0);
    }

    #[test]
    fn encode_lgdt_symbol_ref_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: smallvec::smallvec![Operand::SymbolRef {
                name: "gdt_descriptor".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 0F 01 15 00 00 00 00 (7 bytes)
        // 0F 01 = two-byte opcode
        // 15 = ModR/M with mod=00, reg=2 (/2 for lgdt), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0x15, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "gdt_descriptor");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, 0);
    }

    #[test]
    fn encode_mov_rax_symbol_ref_with_addend_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![
                Operand::Reg(RegId(0)), // rax
                Operand::SymbolRef {
                    name: "table".to_string(),
                    addend: 8,
                }
            ],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: 48 8B 05 00 00 00 00 (7 bytes)
        // 48 = REX.W
        // 8B = mov r64, r/m64 opcode
        // 05 = ModR/M with mod=00, reg=0 (rax), rm=5 (rip-relative)
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site with addend
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 3);
        assert_eq!(output.reloc_sites[0].symbol, "table");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::PcRel32);
        assert_eq!(output.reloc_sites[0].addend, 8);
    }

    #[test]
    fn encode_call_symbol_ref_produces_reloc_site() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Call,
            operands: smallvec::smallvec![Operand::SymbolRef {
                name: "kernel_main_64".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: Some(0),
        };

        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Expect: E8 00 00 00 00 (5 bytes)
        // E8 = call rel32 opcode
        // 00 00 00 00 = placeholder disp32
        assert_eq!(buf.as_slice(), &[0xE8, 0x00, 0x00, 0x00, 0x00]);

        // Verify relocation site (Phase 7 m1-001: uses Plt32)
        assert_eq!(output.reloc_sites.len(), 1);
        assert_eq!(output.reloc_sites[0].byte_offset, 1);
        assert_eq!(output.reloc_sites[0].symbol, "kernel_main_64");
        assert_eq!(output.reloc_sites[0].kind, RelocKind::Plt32);
        assert_eq!(output.reloc_sites[0].addend, 0);
    }

    // Phase 6 m1-002: CR move dispatch tests
    // These tests verify that MOV instructions with CR operands are correctly
    // classified and routed through encode_mov_cr_dispatcher, emitting the correct bytes.

    /// Test: mov cr3, rdi → 0F 22 DF
    /// CR3 = 16 + 3 = 19, RDI = 7
    #[test]
    fn encode_mov_cr3_rdi_emits_0f22df() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(19)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xDF]);
    }

    /// Test: mov cr4, rcx → 0F 22 E1
    /// CR4 = 16 + 4 = 20, RCX = 1
    #[test]
    fn encode_mov_cr4_rcx_emits_0f22e1() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(20)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xE1]);
    }

    /// Test: mov cr0, rax → 0F 22 C0
    /// CR0 = 16 + 0 = 16, RAX = 0
    #[test]
    fn encode_mov_cr0_rax_via_dispatch_emits_0f22c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(16)), Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xC0]);
    }

    /// Test: mov rdi, cr3 → 0F 20 DF (read from CR3)
    /// RDI = 7, CR3 = 16 + 3 = 19
    #[test]
    fn encode_mov_rdi_cr3_emits_0f20df() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(7)), Operand::Reg(RegId(19))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xDF]);
    }

    /// Test: mov rcx, cr4 → 0F 20 E1 (read from CR4)
    /// RCX = 1, CR4 = 16 + 4 = 20
    #[test]
    fn encode_mov_rcx_cr4_emits_0f20e1() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Reg(RegId(20))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xE1]);
    }

    /// Test: mov cr8, rax → 44 0F 22 C0 (CR8 requires REX.R)
    /// CR8 = 16 + 8 = 24, RAX = 0
    #[test]
    fn encode_mov_cr8_rax_via_dispatch_emits_440f22c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(24)), Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x22, 0xC0]);
    }

    // Phase 6 m1-003: DR move dispatch tests
    // These tests verify that MOV instructions with DR operands are correctly
    // classified and routed through encode_mov_dr_dispatcher, emitting the correct bytes.
    // DR encoding: dr_idx = RegId - 25 (compact encoding), opcodes 0F 23 (write), 0F 21 (read).

    /// Test: mov dr0, rax → 0F 23 C0
    /// DR0 = 25 + 0 = 25, RAX = 0
    #[test]
    fn encode_mov_dr0_rax_emits_0f23c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(25)), Operand::Reg(RegId(0))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xC0]);
    }

    /// Test: mov dr1, rdi → 0F 23 CF
    /// DR1 = 25 + 1 = 26, RDI = 7
    #[test]
    fn encode_mov_dr1_rdi_emits_0f23cf() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(26)), Operand::Reg(RegId(7))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xCF]);
    }

    /// Test: mov dr7, rcx → 0F 23 F9
    /// DR7 = 25 + 7 = 32, RCX = 1
    #[test]
    fn encode_mov_dr7_rcx_emits_0f23f9() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(32)), Operand::Reg(RegId(1))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF9]);
    }

    /// Test: mov rax, dr0 → 0F 21 C0 (read from DR0)
    /// RAX = 0, DR0 = 25 + 0 = 25
    #[test]
    fn encode_mov_rax_dr0_emits_0f21c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(0)), Operand::Reg(RegId(25))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);
    }

    /// Test: mov rdi, dr1 → 0F 21 CF (read from DR1)
    /// RDI = 7, DR1 = 25 + 1 = 26
    #[test]
    fn encode_mov_rdi_dr1_emits_0f21cf() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(7)), Operand::Reg(RegId(26))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xCF]);
    }

    /// Test: mov rcx, dr7 → 0F 21 F9 (read from DR7)
    /// RCX = 1, DR7 = 25 + 7 = 32
    #[test]
    fn encode_mov_rcx_dr7_emits_0f21f9() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(1)), Operand::Reg(RegId(32))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF9]);
    }

    /// Test: mov r8, dr0 → 0F 21 C0 (read from DR0 into r8, GPR 8)
    /// R8 = 8, DR0 = 25 + 0 = 25
    #[test]
    fn encode_mov_r8_dr0_emits_0f21c0() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Mov,
            operands: smallvec::smallvec![Operand::Reg(RegId(8)), Operand::Reg(RegId(25))],
            encoding_hint: None,
            byte_offset_in_text: None,
        };

        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);
    }
}

/// Phase 6 m3-002: Encode MOVZX (move with zero-extend) instruction.
///
/// MOVZX r64, r/m8/r/m16/r/m32 — zero-extends smaller operand to 64-bit.
/// For now, we only support movzx rax, byte [rdi+offset] pattern used in field access.
///
/// Opcode: 0F B6 for r/m8 → r64, 0F B7 for r/m16 → r64, etc.
/// This is a placeholder implementation; full support deferred to future phase.
fn encode_movzx(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase 6 m3-002: Placeholder MOVZX encoder.
    // For field access lowering, we expect: movzx rax, byte [rdi + offset]
    // Operands: [0] = rax (Reg), [1] = [rdi + offset] (MemSib)
    //
    // Opcode: 0F B6 /r (MOVZX r64, r/m8)
    // REX.W prefix: 48
    // ModR/M: calculate based on addressing mode
    //
    // For the common case: 48 0F B6 47 NN (movzx rax, byte [rdi + disp8])

    if inst.operands.len() != 2 {
        return Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::Movzx,
            expected: 2,
            got: inst.operands.len(),
        });
    }

    // Extract destination (should be rax).
    let dest_reg = match &inst.operands[0] {
        Operand::Reg(reg) => *reg,
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Movzx,
            });
        }
    };

    if dest_reg.0 != 0 {
        // For now, only support rax as destination.
        return Err(EncodeError::Unsupported(
            "MOVZX: only rax destination supported in phase 6",
        ));
    }

    // Extract source (should be [rdi + offset]).
    let (base_reg, disp) = match &inst.operands[1] {
        Operand::MemSib {
            base, index, disp, ..
        } => {
            if index.is_some() {
                return Err(EncodeError::Unsupported(
                    "MOVZX: indexed addressing not supported",
                ));
            }
            (*base, *disp)
        }
        _ => {
            return Err(EncodeError::OperandShape {
                mnemonic: Mnemonic::Movzx,
            });
        }
    };

    if base_reg.0 != 7 {
        // For now, only support rdi as base.
        return Err(EncodeError::Unsupported(
            "MOVZX: only rdi base register supported in phase 6",
        ));
    }

    // Emit: 48 0F B6 47 NN (movzx rax, byte [rdi + disp8])
    // or:   48 0F B6 87 NNNNNNNN (movzx rax, byte [rdi + disp32])

    buf.bytes.push(0x48); // REX.W (64-bit)
    buf.bytes.push(0x0F); // Two-byte opcode
    buf.bytes.push(0xB6); // MOVZX r64, r/m8

    // ModR/M: mod=01 (disp8) or mod=10 (disp32), reg=000 (rax), r/m=111 (rdi)
    if disp >= -128 && disp <= 127 {
        // disp8 mode: mod=01
        buf.bytes.push(0x47); // mod=01, reg=000, r/m=111
        buf.bytes.push(disp as u8);
    } else {
        // disp32 mode: mod=10
        buf.bytes.push(0x87); // mod=10, reg=000, r/m=111
        buf.bytes.push((disp & 0xFF) as u8);
        buf.bytes.push(((disp >> 8) & 0xFF) as u8);
        buf.bytes.push(((disp >> 16) & 0xFF) as u8);
        buf.bytes.push(((disp >> 24) & 0xFF) as u8);
    }
    Ok(EncodeOutput::new())
}

// Phase 6 m4-003: Jcc encoder tests (16 condition variants + label support)

#[cfg(test)]
mod jcc_tests {
    use super::*;
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};
    use paideia_as_ir::{Cond as IrCond, Instruction, Mnemonic, Operand};

    // Test 1: Je with immediate (rel32) round-trips through iced-x86
    #[test]
    fn jcc_je_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Eq),
            operands: smallvec::smallvec![Operand::Imm64(0x100)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should be 6 bytes: 0F 84 <rel32_le>
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[0], 0x0F);
        assert_eq!(buf.as_slice()[1], 0x84);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Je);
    }

    // Test 2: Jne with immediate (rel32) round-trips
    #[test]
    fn jcc_jne_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Ne),
            operands: smallvec::smallvec![Operand::Imm64(0x1000)], // Large displacement
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[0], 0x0F);
        assert_eq!(buf.as_slice()[1], 0x85);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jne);
    }

    // Test 3: Jl (signed less than) round-trips
    #[test]
    fn jcc_jl_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Lt),
            operands: smallvec::smallvec![Operand::Imm64(0x200)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8C);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jl);
    }

    // Test 4: Jg (signed greater than) round-trips
    #[test]
    fn jcc_jg_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Gt),
            operands: smallvec::smallvec![Operand::Imm64(0x300)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8F);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jg);
    }

    // Test 5: Jle round-trips
    #[test]
    fn jcc_jle_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Le),
            operands: smallvec::smallvec![Operand::Imm64(0x400)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8E);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jle);
    }

    // Test 6: Jge round-trips
    #[test]
    fn jcc_jge_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Ge),
            operands: smallvec::smallvec![Operand::Imm64(-5000i64)], // Large negative displacement
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x8D);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jge);
    }

    // Test 7: Jb (below, unsigned) round-trips
    #[test]
    fn jcc_jb_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Below),
            operands: smallvec::smallvec![Operand::Imm64(0x500)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x82);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jb);
    }

    // Test 8: Jbe round-trips
    #[test]
    fn jcc_jbe_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::BelowOrEqual),
            operands: smallvec::smallvec![Operand::Imm64(0x600)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x86);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jbe);
    }

    // Test 9: Ja round-trips
    #[test]
    fn jcc_ja_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Above),
            operands: smallvec::smallvec![Operand::Imm64(0x700)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x87);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Ja);
    }

    // Test 10: Jae round-trips
    #[test]
    fn jcc_jae_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::AboveOrEqual),
            operands: smallvec::smallvec![Operand::Imm64(0x800)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x83);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jae);
    }

    // Test 11: Jz (alias for Je) round-trips
    #[test]
    fn jcc_jz_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Zero),
            operands: smallvec::smallvec![Operand::Imm64(0x100)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x84); // Same opcode as Je

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Decoder will recognize as Je (iced-x86 canonicalizes)
        assert_eq!(instr.mnemonic(), IcedMnem::Je);
    }

    // Test 12: Jnz (alias for Jne) round-trips
    #[test]
    fn jcc_jnz_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::NonZero),
            operands: smallvec::smallvec![Operand::Imm64(0x200)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x85); // Same opcode as Jne

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        // Decoder will recognize as Jne (iced-x86 canonicalizes)
        assert_eq!(instr.mnemonic(), IcedMnem::Jne);
    }

    // Test 13: Js round-trips
    #[test]
    fn jcc_js_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Sign),
            operands: smallvec::smallvec![Operand::Imm64(0x300)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x88);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Js);
    }

    // Test 14: Jns round-trips
    #[test]
    fn jcc_jns_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::NotSign),
            operands: smallvec::smallvec![Operand::Imm64(0x400)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x89);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jns);
    }

    // Test 15: Jo round-trips
    #[test]
    fn jcc_jo_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Overflow),
            operands: smallvec::smallvec![Operand::Imm64(0x500)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x80);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jo);
    }

    // Test 16: Jno round-trips
    #[test]
    fn jcc_jno_imm_rel32_round_trips() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::NotOverflow),
            operands: smallvec::smallvec![Operand::Imm64(0x600)],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[1], 0x81);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Jno);
    }

    // Test 17: Je with label reference records fixup correctly
    #[test]
    fn jcc_je_label_records_fixup() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jcc(IrCond::Eq),
            operands: smallvec::smallvec![Operand::LabelRef {
                name: "fail".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should emit 6 bytes with zero placeholder
        assert_eq!(buf.len(), 6);
        assert_eq!(buf.as_slice()[0], 0x0F);
        assert_eq!(buf.as_slice()[1], 0x84);
        assert_eq!(&buf.as_slice()[2..6], &[0, 0, 0, 0]);

        // Should record fixup
        assert_eq!(output.label_fixups.len(), 1);
        let fixup = &output.label_fixups[0];
        assert_eq!(fixup.label_name, "fail");
        assert_eq!(fixup.byte_offset, 2);
        assert_eq!(fixup.addend, 0);
        assert_eq!(fixup.instruction_size, 6);
    }

    // Test 18: Jmp with label reference records fixup correctly
    #[test]
    fn jmp_label_records_fixup() {
        let mut buf = CodeBuffer::new();
        let inst = Instruction {
            mnemonic: Mnemonic::Jmp,
            operands: smallvec::smallvec![Operand::LabelRef {
                name: "end".to_string(),
                addend: 0,
            }],
            encoding_hint: None,
            byte_offset_in_text: None,
        };
        let mut stats = EncodeStats::new();
        let output = encode_instruction(&inst, &mut buf, &mut stats).expect("encoding failed");

        // Should emit 5 bytes: E9 + zero rel32
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.as_slice()[0], 0xE9);
        assert_eq!(&buf.as_slice()[1..5], &[0, 0, 0, 0]);

        // Should record fixup
        assert_eq!(output.label_fixups.len(), 1);
        let fixup = &output.label_fixups[0];
        assert_eq!(fixup.label_name, "end");
        assert_eq!(fixup.byte_offset, 1);
        assert_eq!(fixup.addend, 0);
        assert_eq!(fixup.instruction_size, 5);
    }
}
