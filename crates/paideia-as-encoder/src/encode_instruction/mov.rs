//! Core MOV encoders: the width-parametric `encode_mov_sized`
//! (byte/word/dword generic path) and the address-form-directed
//! `encode_mov` (register/memory/immediate/RIP-relative, plus CR/DR
//! dispatch and 64-bit immediate short forms).
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;

/// Encode a width-threaded immediate-to-register move — Phase 7 m4-003.
///
/// Expects `[Operand::Reg(dst), Operand::Imm64(imm)]`. The `width` selects the
/// encoded form:
/// - W64 → delegates to the generic `encode_mov` path (`48 C7`/`48 B8`),
///   preserving the existing 64-bit behaviour.
/// - W32 → `B8+rd imm32` (5 bytes, no REX.W; implicit zero-extend to r64).
/// - W16 → `66 B8+rd imm16` (4 bytes).
/// - W8  → `B0+rb imm8` (2 bytes; 3 with REX.B for r8–r15).
///
/// The immediate is truncated to the operand width before encoding, matching
/// the semantics of a typed integer-literal binding.
pub(super) fn encode_mov_sized(
    inst: &Instruction,
    buf: &mut CodeBuffer,
    width: IntWidth,
) -> Result<EncodeOutput, EncodeError> {
    match inst.operands.as_slice() {
        [Operand::Reg(dst), Operand::Imm64(imm)] => {
            let dst_reg = reg64_from(*dst)?;
            let imm = *imm as u64;
            match width {
                // W64 reuses the established 64-bit move path verbatim.
                IntWidth::W64 => {
                    // Phase 15 m3-001: reject 64-bit destination in 32-bit mode
                    if inst.mode == InstrMode::Mode32 {
                        return Err(EncodeError::Unsupported(
                            "E0019: 64-bit destination in 32-bit mode",
                        ));
                    }
                    mov_reg64_imm64(buf, dst_reg, imm)
                }
                IntWidth::W32 => mov_reg32_imm32(buf, dst_reg, imm as u32),
                IntWidth::W16 => mov_reg16_imm16(buf, dst_reg, imm as u16),
                IntWidth::W8 => mov_reg8_imm8(buf, dst_reg, imm as u8),
            }
            Ok(EncodeOutput::new())
        }
        // PA13-001 (#930): narrow-width load from memory [base + disp] (no index)
        [Operand::Reg(dst), Operand::MemSib { base, index: None, .. }] => {
            let dst_reg = reg64_from(*dst)?;
            let base_reg = reg64_from(*base)?;
            // MemSib carries the displacement in the disp field (always present in MemSib)
            let disp = inst
                .operands
                .get(1)
                .and_then(|op| {
                    if let Operand::MemSib { disp, .. } = op {
                        Some(*disp)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            match width {
                IntWidth::W64 => mov_reg64_mem_reg64_disp(buf, dst_reg, base_reg, disp),
                IntWidth::W32 => mov_reg32_mem_base_disp(buf, dst_reg, base_reg, disp),
                IntWidth::W16 => mov_reg16_mem_base_disp(buf, dst_reg, base_reg, disp),
                IntWidth::W8 => mov_reg8_mem_base_disp(buf, dst_reg, base_reg, disp),
            }
            Ok(EncodeOutput::new())
        }
        // PA13-001 (#930): narrow-width load from memory [base + index*scale + disp]
        [Operand::Reg(dst), Operand::MemSib { base, index: Some(index), scale, disp }] => {
            let dst_reg = reg64_from(*dst)?;
            let base_reg = reg64_from(*base)?;
            let index_reg = reg64_from(*index)?;
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };

            match width {
                IntWidth::W64 => mov_reg64_mem_sib_disp(buf, dst_reg, base_reg, index_reg, scale_bits, *disp),
                _ => mov_reg_mem_sib_disp_sized(buf, width, dst_reg, base_reg, index_reg, scale_bits, *disp),
            }
            Ok(EncodeOutput::new())
        }
        // PA-R14-001 (#944): narrow-width store to memory [base + disp], imm
        [Operand::MemSib { base, index: None, disp, .. }, Operand::Imm64(imm)] => {
            let base_reg = reg64_from(*base)?;
            let disp32 = *disp;
            match width {
                IntWidth::W8 => mov_mem_base_disp_imm8(buf, base_reg, disp32, *imm as u8),
                IntWidth::W16 => mov_mem_base_disp_imm16(buf, base_reg, disp32, *imm as u16),
                IntWidth::W32 => mov_mem_base_disp_imm32(buf, base_reg, disp32, *imm as u32),
                IntWidth::W64 => {
                    if *imm < i32::MIN as i64 || *imm > i32::MAX as i64 {
                        return Err(EncodeError::Unsupported(
                            "mov_q [mem], imm64 requires imm ∈ i32 sign-ext range; use movabs r11, imm64 + mov [mem], r11",
                        ));
                    }
                    mov_mem_base_disp_imm32_sxt(buf, base_reg, disp32, *imm as i32);
                }
            }
            Ok(EncodeOutput::new())
        }
        // pa-r17-006 (#984): narrow-width register-source STORE, [base + disp], reg
        [Operand::MemSib { base, index: None, scale: Scale::X1, disp, .. }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let src_reg = reg64_from(*src)?;
            match width {
                IntWidth::W8 => mov_mem_base_disp_reg8(buf, base_reg, *disp, src_reg),
                IntWidth::W16 => mov_mem_base_disp_reg16(buf, base_reg, *disp, src_reg),
                IntWidth::W32 => mov_mem_base_disp_reg32(buf, base_reg, *disp, src_reg),
                IntWidth::W64 => mov_mem_reg64_disp_reg64(buf, base_reg, *disp, src_reg),
            }
            Ok(EncodeOutput::new())
        }
        // #1269: narrow-width register-source STORE, [base + index*scale + disp], reg
        // Complements the SIB-indexed IMMEDIATE-store arm below; without this the
        // 3-operand SIB store `mov_d [rax+rcx*4], edi` fell through to the generic
        // OperandShape handler and silently widened to REX.W (64-bit) form.
        [Operand::MemSib { base, index: Some(idx), scale, disp }, Operand::Reg(src)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg = reg64_from(*idx)?;
            let src_reg = reg64_from(*src)?;
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            match width {
                IntWidth::W8 => mov_mem_sib_disp_reg8(buf, base_reg, index_reg, scale_bits, *disp, src_reg),
                IntWidth::W16 => mov_mem_sib_disp_reg16(buf, base_reg, index_reg, scale_bits, *disp, src_reg),
                IntWidth::W32 => mov_mem_sib_disp_reg32(buf, base_reg, index_reg, scale_bits, *disp, src_reg),
                IntWidth::W64 => mov_mem_sib_disp_reg64(buf, base_reg, index_reg, scale_bits, *disp, src_reg),
            }
            Ok(EncodeOutput::new())
        }
        // PA-R14-001 (#944): narrow-width store to memory [base + index*scale + disp], imm
        [Operand::MemSib { base, index: Some(idx), scale, disp }, Operand::Imm64(imm)] => {
            let base_reg = reg64_from(*base)?;
            let index_reg = reg64_from(*idx)?;
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            let disp32 = *disp;
            match width {
                IntWidth::W8 => mov_mem_sib_disp_imm8(buf, base_reg, index_reg, scale_bits, disp32, *imm as u8),
                IntWidth::W16 => mov_mem_sib_disp_imm16(buf, base_reg, index_reg, scale_bits, disp32, *imm as u16),
                IntWidth::W32 => mov_mem_sib_disp_imm32(buf, base_reg, index_reg, scale_bits, disp32, *imm as u32),
                IntWidth::W64 => {
                    if *imm < i32::MIN as i64 || *imm > i32::MAX as i64 {
                        return Err(EncodeError::Unsupported(
                            "mov_q [mem], imm64 requires imm ∈ i32 sign-ext range; use movabs r11, imm64 + mov [mem], r11",
                        ));
                    }
                    mov_mem_sib_disp_imm32_sxt(buf, base_reg, index_reg, scale_bits, disp32, *imm as i32);
                }
            }
            Ok(EncodeOutput::new())
        }
        // PA-R14-002b (#1030): narrow-width load from RIP-relative memory [rip + disp]
        [Operand::Reg(dst), Operand::MemRipRel { disp }] => {
            let dst_id = dst.0;
            mov_reg_mem_rip_rel_sized(buf, width, dst_id, *disp);
            Ok(EncodeOutput::new())
        }
        // PA-R14-002b (#1030): narrow-width load from RIP-relative memory with symbol [rip + sym]
        [Operand::Reg(dst), Operand::MemRipRelSym { name, addend }] => {
            let dst_id = dst.0;
            // Calculate bytes before disp32: prefix (if W16) + REX (if needed) + opcode (1) + ModRM (1)
            let prefix_len = if matches!(width, IntWidth::W16) { 1 } else { 0 };
            let rex_len = if (dst_id >> 3) != 0 || matches!(width, IntWidth::W64) { 1 } else { 0 };
            // #1143: instruction-local offset; text_emitter adds offset_before.
            let byte_offset = prefix_len + rex_len + 2;
            mov_reg_mem_rip_rel_sized(buf, width, dst_id, 0);
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R17-006b (#1046): narrow-width store to RIP-relative memory with symbol [rip + sym], src
        [Operand::MemRipRelSym { name, addend }, Operand::Reg(src)] => {
            let src_id = src.0;
            // #1143: capture start so disp_offset stays instruction-local.
            let start = buf.bytes.len() as u32;
            // Emit width-appropriate mov [rip+disp32], src instruction
            // Prefix (if W16) + REX (if W64 or REX.R) + opcode (1) + ModRM (1) + disp32 (4)
            if matches!(width, IntWidth::W16) {
                buf.bytes.push(0x66);
            }
            let rex_w = matches!(width, IntWidth::W64);
            let rex_r = (src_id >> 3) != 0;
            if rex_w || rex_r {
                buf.bytes.push(rex(rex_w, rex_r, false, false));
            }
            let opcode = if matches!(width, IntWidth::W8) { 0x88 } else { 0x89 };
            buf.bytes.push(opcode);
            // ModR/M: mod=00, reg=src[2:0], r/m=101 (RIP-relative)
            buf.bytes.push(0x05 | ((src_id & 7) << 3));
            // 4-byte disp32 placeholder; relocation will patch it
            let disp_offset = buf.bytes.len() as u32 - start;
            buf.bytes.extend([0, 0, 0, 0]);
            // Emit PC32 relocation site
            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: disp_offset,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R16-007: mov reg, [disp32] with width from MovSized mnemonic
        [Operand::Reg(dst), Operand::MemDisp { disp }] => {
            let dst_reg = reg64_from(*dst)?;
            mov_reg_mem_abs_disp32(buf, width, dst_reg, *disp);
            Ok(EncodeOutput::new())
        }
        // PA-R16-007: mov [disp32], reg with width from MovSized mnemonic
        [Operand::MemDisp { disp }, Operand::Reg(src)] => {
            let src_reg = reg64_from(*src)?;
            mov_mem_abs_disp32_reg(buf, width, *disp, src_reg);
            Ok(EncodeOutput::new())
        }
        // PA-R16-007: mov [disp32], imm with width from MovSized mnemonic
        [Operand::MemDisp { disp }, Operand::Imm64(imm)] => {
            match width {
                IntWidth::W8 | IntWidth::W16 | IntWidth::W32 => {
                    mov_mem_abs_disp32_imm(buf, width, *disp, *imm);
                }
                IntWidth::W64 => {
                    // For W64, the immediate must fit in i32 range (sign-extended)
                    if *imm < i32::MIN as i64 || *imm > i32::MAX as i64 {
                        return Err(EncodeError::Unsupported(
                            "mov_q [disp32], imm64 requires imm ∈ i32 sign-ext range; use movabs r11, imm64 + mov [disp32], r11",
                        ));
                    }
                    mov_mem_abs_disp32_imm(buf, width, *disp, *imm);
                }
            }
            Ok(EncodeOutput::new())
        }
        operands if operands.iter().any(|op| matches!(op, Operand::Var { .. })) => {
            unreachable!("Operand::Var reached encoder — resolve_var_operands pass was skipped")
        }
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::MovSized { width },
        }),
    }
}


pub(super) fn encode_mov(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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

    // Phase 15 m5-002: Handle segment register MOV (mov sreg, r16).
    // Opcode: 8E /r (no REX prefix)
    match inst.operands.as_slice() {
        [Operand::SegReg(sreg), Operand::Reg(src)] => {
            let sreg_id = sreg.id();
            let src_reg = reg64_from(*src)?;
            mov_sreg_reg16(buf, sreg_id, src_reg);
            return Ok(EncodeOutput::new());
        }
        _ => {}
    }

    // Phase 15 m3-001: Mode32 dispatch for mov r32, imm32 and mov r32, r32
    if inst.mode == InstrMode::Mode32 {
        match inst.operands.as_slice() {
            [Operand::Reg(dst), Operand::Imm64(imm)] => {
                let imm_u = *imm as u64;
                let fits_u32 = imm_u <= u32::MAX as u64;
                let fits_i32 = (*imm as i64) >= i32::MIN as i64 && (*imm as i64) <= i32::MAX as i64;
                if !(fits_u32 || fits_i32) {
                    return Err(EncodeError::Unsupported(
                        "E0019: 64-bit immediate in 32-bit mode",
                    ));
                }
                mov_reg32_imm32(buf, reg64_from(*dst)?, imm_u as u32);
                return Ok(EncodeOutput::new());
            }
            [Operand::Reg(dst), Operand::Reg(src)] => {
                mov_reg32_reg32(buf, reg64_from(*dst)?, reg64_from(*src)?);
                return Ok(EncodeOutput::new());
            }
            // Phase 15 m3-002: mov r32, [abs32]
            [Operand::Reg(dst), Operand::SymbolRef { name, addend }] => {
                let byte_offset = mov_reg32_mem_abs32(buf, reg64_from(*dst)?);

                let mut output = EncodeOutput::new();
                output.add_reloc(RelocSite {
                    byte_offset,
                    symbol: name.clone(),
                    kind: RelocKind::Abs32,
                    addend: *addend,
                });
                return Ok(output);
            }
            // Phase 15 m3-002: mov [abs32], r32
            [Operand::SymbolRef { name, addend }, Operand::Reg(src)] => {
                let byte_offset = mov_mem_abs32_reg32(buf, reg64_from(*src)?);

                let mut output = EncodeOutput::new();
                output.add_reloc(RelocSite {
                    byte_offset,
                    symbol: name.clone(),
                    kind: RelocKind::Abs32,
                    addend: *addend,
                });
                return Ok(output);
            }
            // Phase 15 m3-003: mov [abs32], imm32
            [Operand::SymbolRef { name, addend }, Operand::Imm64(imm)] => {
                let imm_i = *imm as i64;
                let fits_u32 = (*imm as u64) <= u32::MAX as u64;
                let fits_i32 = (i32::MIN as i64..=i32::MAX as i64).contains(&imm_i);
                if !(fits_u32 || fits_i32) {
                    return Err(EncodeError::Unsupported(
                        "E0019: mov [abs32], imm: immediate exceeds 32 bits",
                    ));
                }
                let byte_offset = mov_mem_abs32_imm32(buf, *imm as u32);
                let mut output = EncodeOutput::new();
                output.add_reloc(RelocSite {
                    byte_offset,
                    symbol: name.clone(),
                    kind: RelocKind::Abs32,
                    addend: *addend,
                });
                return Ok(output);
            }
            _ => {} // fall through
        }
    }

    // Phase 15 m3-003: Mode64 diagnostic for mov [abs], imm
    if inst.mode == InstrMode::Mode64 {
        if let [Operand::SymbolRef { .. }, Operand::Imm64(_)] = inst.operands.as_slice() {
            return Err(EncodeError::Unsupported(
                "mov [abs], imm not encodable in 64-bit mode without register base; \
                 use mov rax, sym; mov [rax], imm",
            ));
        }
    }

    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // mov r64, r64 → 48 89 <ModR/M>
            mov_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            // Emit 7-byte C7 imm32 form when value fits sign-extended i32.
            // The C7 /0 instruction (REX.W C7 C0+rd imm32) sign-extends imm32 into r64,
            // which is architecturally identical to B8+imm64 for in-range values.
            // This saves 3 bytes per mov on common hot paths (e.g. per-arg call marshalling,
            // small syscall args, enum discriminants). Value -1_i64 correctly encodes
            // as 48 C7 C0 FF FF FF FF (sign-extends to 0xFFFFFFFFFFFFFFFF).
            if *imm >= i32::MIN as i64 && *imm <= i32::MAX as i64 {
                mov_reg64_imm32(buf, reg64_from(*dest)?, *imm as i32);
            } else {
                mov_reg64_imm64(buf, reg64_from(*dest)?, *imm as u64);
            }
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
            // mov r64, [base + index * scale] — delegate to general SIB handler
            // (emit_indexed_load was designed for mixed widths and confused scale with operand width)
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            mov_reg64_mem_sib_disp(
                buf,
                reg64_from(*dest)?,
                reg64_from(*base)?,
                reg64_from(*index)?,
                scale_bits,
                0,
            );
            Ok(EncodeOutput::new())
        }
        [
            Operand::Reg(dest),
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
        ] => {
            // mov r64, [base + disp] — Phase 8 m5-002: general memory operand form
            mov_reg64_mem_reg64_disp(buf, reg64_from(*dest)?, reg64_from(*base)?, *disp);
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
            // mov [base + disp], r64 — Phase 8 m5-002: general memory operand form
            mov_mem_reg64_disp_reg64(buf, reg64_from(*base)?, *disp, reg64_from(*src)?);
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
            // mov r64, [base + index*scale + disp] — Phase 9 m1-003: SIB with displacement
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            mov_reg64_mem_sib_disp(
                buf,
                reg64_from(*dest)?,
                reg64_from(*base)?,
                reg64_from(*index)?,
                scale_bits,
                *disp,
            );
            Ok(EncodeOutput::new())
        }
        [
            Operand::MemSib {
                base,
                index: Some(index),
                scale,
                disp,
            },
            Operand::Reg(src),
        ] => {
            // mov [base + index*scale + disp], r64 — Phase 9 m1-003: SIB with displacement
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            mov_mem_sib_disp_reg64(
                buf,
                reg64_from(*base)?,
                reg64_from(*index)?,
                scale_bits,
                *disp,
                reg64_from(*src)?,
            );
            Ok(EncodeOutput::new())
        }
        // Fix #1240: mov [base + disp], imm64 (no index, just base + displacement)
        [
            Operand::MemSib {
                base,
                index: None,
                scale: Scale::X1,
                disp,
            },
            Operand::Imm64(imm),
        ] => {
            // mov [base + disp], imm — W64 form with sign-extended i32 immediate
            let base_reg = reg64_from(*base)?;
            let disp32 = *disp;
            if *imm < i32::MIN as i64 || *imm > i32::MAX as i64 {
                return Err(EncodeError::Unsupported(
                    "mov_q [mem], imm64 requires imm ∈ i32 sign-ext range; use movabs r11, imm64 + mov [mem], r11",
                ));
            }
            mov_mem_base_disp_imm32_sxt(buf, base_reg, disp32, *imm as i32);
            Ok(EncodeOutput::new())
        }
        // Fix #1240: mov [base + index*scale + disp], imm64 (SIB with index + displacement)
        [
            Operand::MemSib {
                base,
                index: Some(idx),
                scale,
                disp,
            },
            Operand::Imm64(imm),
        ] => {
            // mov [base + index*scale + disp], imm — W64 form with sign-extended i32 immediate
            let base_reg = reg64_from(*base)?;
            let index_reg = reg64_from(*idx)?;
            let scale_bits = match scale {
                Scale::X1 => 0,
                Scale::X2 => 1,
                Scale::X4 => 2,
                Scale::X8 => 3,
            };
            let disp32 = *disp;
            if *imm < i32::MIN as i64 || *imm > i32::MAX as i64 {
                return Err(EncodeError::Unsupported(
                    "mov_q [mem], imm64 requires imm ∈ i32 sign-ext range; use movabs r11, imm64 + mov [mem], r11",
                ));
            }
            mov_mem_sib_disp_imm32_sxt(buf, base_reg, index_reg, scale_bits, disp32, *imm as i32);
            Ok(EncodeOutput::new())
        }
        // Fix #1240: mov [rip + sym], imm64 (RIP-relative memory with symbol)
        [Operand::MemRipRelSym { name, addend }, Operand::Imm64(imm)] => {
            // mov [rip + sym], imm — W64 form via RIP-relative addressing
            // Emits: 48 C7 05 <disp32_placeholder> <imm32>
            if *imm < i32::MIN as i64 || *imm > i32::MAX as i64 {
                return Err(EncodeError::Unsupported(
                    "mov_q [mem], imm64 requires imm ∈ i32 sign-ext range; use movabs r11, imm64 + mov [mem], r11",
                ));
            }
            // REX.W prefix (0x48), opcode (0xC7), ModR/M with rip-relative form (0x05)
            buf.bytes.push(0x48);
            buf.bytes.push(0xC7);
            buf.bytes.push(0x05);
            buf.bytes.extend(imm.to_le_bytes()[0..4].iter());
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3, // disp32 starts at byte +3 (instruction-local)
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R16-007: mov reg, [disp32] and related absolute-address forms
        [Operand::Reg(dest), Operand::MemDisp { disp }] => {
            // mov reg, [disp32] — delegate to absolute-form encoder
            // Width is determined by the register size (default W64 for r64)
            let dest_reg = reg64_from(*dest)?;
            let width = if let Mnemonic::MovSized { width } = inst.mnemonic {
                width
            } else {
                // Plain Mov defaults to W64
                IntWidth::W64
            };
            mov_reg_mem_abs_disp32(buf, width, dest_reg, *disp);
            Ok(EncodeOutput::new())
        }
        [Operand::MemDisp { disp }, Operand::Reg(src)] => {
            // mov [disp32], reg — delegate to absolute-form encoder
            // Width is determined by the register size (default W64 for r64)
            let src_reg = reg64_from(*src)?;
            let width = if let Mnemonic::MovSized { width } = inst.mnemonic {
                width
            } else {
                // Plain Mov defaults to W64
                IntWidth::W64
            };
            mov_mem_abs_disp32_reg(buf, width, *disp, src_reg);
            Ok(EncodeOutput::new())
        }
        [Operand::MemDisp { disp }, Operand::Imm64(imm)] => {
            // mov [disp32], imm — delegate to absolute-form encoder
            // Width must come from MovSized mnemonic; if plain Mov, default W64
            let width = if let Mnemonic::MovSized { width } = inst.mnemonic {
                width
            } else {
                // Plain Mov with immediate defaults to W64
                IntWidth::W64
            };
            mov_mem_abs_disp32_imm(buf, width, *disp, *imm);
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

            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3, // disp32 starts at byte +3 of the mov instruction (instruction-local); translator adds offset_before
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        [Operand::SymbolRef { name, addend }, Operand::Reg(src)] => {
            // PA10-006w: mov [symbol + addend], r64 → 48 89 /r [rip-relative ModR/M] [disp32_placeholder]
            // Symmetric to the load form above; opcode 0x8B → 0x89 for store; REX.R still applies
            // to the register operand (now the source). ModR/M mod=00, rm=5 (rip-relative),
            // reg field = src<2:0>. Emits R_X86_64_PC32 with addend biased by -4 per SysV AMD64 ABI.
            let src_id = reg64_from(*src)? as u8;
            let rex_byte = rex(true, (src_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x89); // mov r/m64, r64 opcode
            buf.bytes.push(0x05 | ((src_id & 7) << 3)); // ModR/M with rip-relative form
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: Parallel MemRipRelSym form for mov r64, [rip + sym + addend]
        [Operand::Reg(dest), Operand::MemRipRelSym { name, addend }] => {
            // Identical encoding to SymbolRef form: 48 8B /r [rip-relative ModR/M] [disp32_placeholder]
            let dest_id = reg64_from(*dest)? as u8;
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8B); // mov r64, r/m64 opcode
            buf.bytes.push(0x05 | ((dest_id & 7) << 3)); // ModR/M with rip-relative form
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        // PA-R13-003: Parallel MemRipRelSym form for mov [rip + sym + addend], r64
        [Operand::MemRipRelSym { name, addend }, Operand::Reg(src)] => {
            // Identical encoding to SymbolRef store form: 48 89 /r [rip-relative ModR/M] [disp32_placeholder]
            let src_id = reg64_from(*src)? as u8;
            let rex_byte = rex(true, (src_id >> 3) != 0, false, false);

            buf.bytes.push(rex_byte);
            buf.bytes.push(0x89); // mov r/m64, r64 opcode
            buf.bytes.push(0x05 | ((src_id & 7) << 3)); // ModR/M with rip-relative form
            buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

            let mut output = EncodeOutput::new();
            output.add_reloc(RelocSite {
                byte_offset: 3,
                symbol: name.clone(),
                kind: RelocKind::PcRel32,
                addend: addend.wrapping_add(PC32_FIELD_BIAS),
            });
            Ok(output)
        }
        operands if operands.iter().any(|op| matches!(op, Operand::Var { .. })) => {
            unreachable!("Operand::Var reached encoder — resolve_var_operands pass was skipped")
        }
        _ => Err(EncodeError::Unsupported(
            "unsupported mov operand shape; check operand types and addressing modes",
        )),
    }
}
