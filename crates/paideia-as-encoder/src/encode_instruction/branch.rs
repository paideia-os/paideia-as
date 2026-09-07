//! Control-transfer encoders: `jcc`, `setcc`, `jmp`, `call`, `ret`,
//! and the far-jump form (`far_jmp`).
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

pub(super) fn encode_jcc(
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

            // Emit jcc rel32 with zero placeholder
            jcc_rel32(buf, cond, 0);

            let mut output = EncodeOutput::new();
            output.add_label_fixup(LabelFixup {
                byte_offset: 2, // offset of rel32 relative to instruction start (after 0F XX)
                label_name: name.clone(),
                addend: *addend,
                instruction_size: 6,
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "jcc form not supported: expected an immediate displacement or a label reference",
        )),
    }
}

/// Encode `setcc r8` — set byte on condition.
///
/// Expects `[Operand::Reg(dst)]` where dst is an 8-bit register.
/// Emits via `setcc_reg8` with REX handling for extended registers.
pub(super) fn encode_setcc(
    ir_cond: IrCond,
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst)] => {
            let cond = cond_from(ir_cond)?;
            let (reg_id, needs_rex) = resolve_reg8(*dst);
            setcc_reg8(buf, cond, reg_id, needs_rex);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::Setcc(ir_cond),
        }),
    }
}

pub(super) fn encode_jmp(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // jmp rel32 → E9 <rel32>
            jmp_rel32(buf, *rel as i32);
            Ok(EncodeOutput::new())
        }
        [Operand::LabelRef { name, addend }] => {
            // Phase 6 m4-003: Label reference (forward or backward).
            // Emit placeholder rel32 and record fixup for linker resolution.

            // Emit jmp rel32 with zero placeholder
            jmp_rel32(buf, 0);

            let mut output = EncodeOutput::new();
            output.add_label_fixup(LabelFixup {
                byte_offset: 1, // offset of rel32 relative to instruction start (after E9)
                label_name: name.clone(),
                addend: *addend,
                instruction_size: 5,
            });
            Ok(output)
        }
        [Operand::SymbolRef { name, addend }] => {
            // paideia-os#1271: cross-module `jmp <symbol>` — mirrors encode_call's
            // SymbolRef arm but with opcode E9 (near jmp rel32) instead of E8.
            // Emits placeholder disp32 + RelocSite::Plt32 for link-time resolution.
            let _ = inst.byte_offset_in_text; // unused; translator owns the math
            buf.bytes.push(0xE9); // jmp rel32 opcode
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 1, // rel32 starts at byte +1 of the instruction
                symbol: name.clone(),
                kind: RelocKind::Plt32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        [Operand::MemSymIndexed { name, addend, index, scale }] => {
            // PA-R15-009a: jmp [sym + index*scale] with absolute addressing.
            // Emit FF 24 SIB disp32 (absolute form, not RIP-relative).
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            let index_reg = reg64_from(*index)?;
            let disp_offset = jmp_mem_sib_no_base_indexed(buf, index_reg, scale_bits, 0)?;

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: disp_offset as u32,
                symbol: name.clone(),
                kind: RelocKind::Abs32,
                addend: *addend,
            });
            Ok(output)
        }
        [Operand::MemDispIndexed { disp, index, scale }] => {
            // PA-R20-006: jmp [disp + index*scale] with absolute addressing (resolved form).
            // Emitted by resolve_symbols from MemSymIndexed after address resolution.
            // Emit FF 24 SIB disp32 (absolute form, not RIP-relative), no relocation.
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            let index_reg = reg64_from(*index)?;
            let _ = jmp_mem_sib_no_base_indexed(buf, index_reg, scale_bits, *disp)?;
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "jmp operand shape not supported by this encoder",
        )),
    }
}

pub(super) fn encode_call(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Imm64(rel)] => {
            // call rel32 → E8 <rel32>
            call_rel32(buf, *rel as i32);
            Ok(EncodeOutput::new())
        }
        [Operand::SymbolRef { name, addend }] => {
            // call symbol → E8 <disp32_placeholder> + RelocSite with Plt32
            // Phase 7 m1-001: Use RelocKind::Plt32 for PLT relocations.
            // Phase 7 m1-003: RelocSite::byte_offset is INSTRUCTION-LOCAL.
            // emit_text_from_instructions translates it to .text-relative by
            // adding `offset_before` (the buffer length at the start of this
            // instruction). The rel32 displacement begins at byte +1 of the E8
            // opcode. Previous code returned `byte_offset_in_text + 1`
            // (already .text-relative), then the translator added
            // `offset_before` again — double-counted, putting the reloc at
            // 2*offset_before + 1 instead of offset_before + 1.
            let _ = inst.byte_offset_in_text; // unused; translator owns the math
            buf.bytes.push(0xE8); // call rel32 opcode
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 1, // rel32 starts at byte +1 of the instruction
                symbol: name.clone(),
                kind: RelocKind::Plt32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: call reg64
        [Operand::Reg(r)] => {
            call_reg64(buf, reg64_from(*r)?);
            Ok(EncodeOutput::new())
        }
        // PA-R13-003: call [base + disp]
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp }] => {
            call_mem_base_disp(buf, reg64_from(*base)?, *disp);
            Ok(EncodeOutput::new())
        }
        // PA-R13-003: call [base + index*scale + disp]
        [Operand::MemSib { base, index: Some(idx), scale, disp }] => {
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            call_mem_sib_disp(buf, reg64_from(*base)?, reg64_from(*idx)?, scale_bits, *disp);
            Ok(EncodeOutput::new())
        }
        // PA-R13-003: call [rip + disp32]
        [Operand::MemRipRel { disp }] => {
            call_mem_rip_rel(buf, *disp);
            Ok(EncodeOutput::new())
        }
        // PA-R13-003: call [rip + sym + addend] → FF 15 <disp32> + PcRel32 reloc
        [Operand::MemRipRelSym { name, addend }] => {
            call_mem_rip_rel(buf, 0); // placeholder disp32
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 2, // rel32 starts at byte +2 of the FF 15 prefix
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        _ => Err(EncodeError::Unsupported(
            "call operand shape not supported by this encoder",
        )),
    }
}

pub(super) fn encode_ret(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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


pub(super) fn encode_far_jmp_inst(
    inst: &Instruction,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // ljmp supports two forms:
    // 1. Memory indirect: [base + disp] or [rip + disp] — expects 1 operand
    // 2. Direct immediate: ljmp selector:offset — expects 2 operands (selector, offset)
    match inst.operands.len() {
        1 => {
            // Memory indirect form
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
        2 => {
            // Direct immediate form: ljmp selector:offset
            // Operand[0] = selector (imm16, encoded as Imm64)
            // Operand[1] = offset (imm32, encoded as Imm64 or symbol reference)
            let selector = match &inst.operands[0] {
                Operand::Imm64(imm) => *imm as u16,
                _ => {
                    return Err(EncodeError::OperandShape {
                        mnemonic: Mnemonic::FarJmp,
                    });
                }
            };

            match &inst.operands[1] {
                Operand::Imm64(imm) => {
                    // Direct immediate: opcode EA + imm32 offset + imm16 selector
                    encode_far_jmp_imm(buf, *imm as u32, selector);
                    Ok(EncodeOutput::new())
                }
                Operand::SymbolRef { name, addend } => {
                    // Symbol reference: emit R_X86_64_32 relocation
                    let mut output = EncodeOutput::new();
                    encode_far_jmp_imm_sym(buf, selector);
                    output.add_reloc(RelocSite {
                        byte_offset: 1, // imm32 starts at byte +1 of the EA instruction (instruction-local); translator adds offset_before
                        symbol: name.clone(),
                        kind: RelocKind::Abs32,
                        addend: *addend,
                    });
                    Ok(output)
                }
                _ => Err(EncodeError::OperandShape {
                    mnemonic: Mnemonic::FarJmp,
                }),
            }
        }
        _ => Err(EncodeError::OperandCount {
            mnemonic: Mnemonic::FarJmp,
            expected: 1,
            got: inst.operands.len(),
        }),
    }
}
