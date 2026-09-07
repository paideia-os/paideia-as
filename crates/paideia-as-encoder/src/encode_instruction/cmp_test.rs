//! Compare and test encoders: `cmp` and the width-typed `cmp_sized`
//! family (per-shape reg/imm and reg/reg helpers), plus `test`.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400) without behavioural changes.

use super::*;


pub(super) fn encode_cmp(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
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
            "cmp form not supported: expected reg64,reg64, reg64,imm64, or [base+disp],reg64",
        )),
    }
}

pub(super) fn encode_cmp_sized(
    inst: &Instruction,
    width: &IntWidth,
    buf: &mut CodeBuffer,
) -> Result<EncodeOutput, EncodeError> {
    // W64 shape is byte-identical to the generic Cmp encoder; delegate.
    if matches!(width, IntWidth::W64) {
        return encode_cmp(inst, buf);
    }
    match (width, inst.operands.as_slice()) {
        // ── cmp reg, imm (all narrow widths) ─────────────────────────
        (IntWidth::W8, [Operand::Reg(dest), Operand::Imm64(imm)]) => {
            encode_cmp_reg8_imm(buf, dest.0, *imm)
        }
        (IntWidth::W16, [Operand::Reg(dest), Operand::Imm64(imm)]) => {
            encode_cmp_reg16_imm(buf, dest.0, *imm)
        }
        (IntWidth::W32, [Operand::Reg(dest), Operand::Imm64(imm)]) => {
            encode_cmp_reg32_imm(buf, dest.0, *imm)
        }
        // ── cmp reg, reg (all narrow widths) ─────────────────────────
        (IntWidth::W8, [Operand::Reg(dest), Operand::Reg(src)]) => {
            encode_cmp_reg8_reg8(buf, dest.0, src.0)
        }
        (IntWidth::W16, [Operand::Reg(dest), Operand::Reg(src)]) => {
            encode_cmp_reg16_reg16(buf, dest.0, src.0)
        }
        (IntWidth::W32, [Operand::Reg(dest), Operand::Reg(src)]) => {
            encode_cmp_reg32_reg32(buf, dest.0, src.0)
        }
        // ── cmp [base + disp], reg (store shape, all narrow widths) ──
        (
            IntWidth::W8,
            [
                Operand::MemSib {
                    base,
                    index: None,
                    scale: Scale::X1,
                    disp,
                },
                Operand::Reg(src),
            ],
        ) => encode_cmp_mem_reg8(buf, base.0, *disp, src.0),
        (
            IntWidth::W16,
            [
                Operand::MemSib {
                    base,
                    index: None,
                    scale: Scale::X1,
                    disp,
                },
                Operand::Reg(src),
            ],
        ) => encode_cmp_mem_reg16(buf, base.0, *disp, src.0),
        (
            IntWidth::W32,
            [
                Operand::MemSib {
                    base,
                    index: None,
                    scale: Scale::X1,
                    disp,
                },
                Operand::Reg(src),
            ],
        ) => encode_cmp_mem_reg32(buf, base.0, *disp, src.0),
        _ => Err(EncodeError::OperandShape {
            mnemonic: Mnemonic::CmpSized { width: *width },
        }),
    }
}

/// #1254: `cmp reg8, imm8` — [REX.B] 80 /7 ib, with AL short form 3C ib.
///
/// AL short form (2 bytes): 3C ib for `cmp al, imm8`.
/// General form: (REX for r8b–r15b) 80 F8+r ib.
/// Non-REX access to spl/bpl/sil/dil (RegId 4..=7 as W8) is not yet supported —
/// callers must emit REX prefix which flips the operand meaning; this is a
/// documented future extension (see comment in walker retarget).
pub(super) fn encode_cmp_reg8_imm(
    buf: &mut CodeBuffer,
    dest_reg_id: u8,
    imm_i64: i64,
) -> Result<EncodeOutput, EncodeError> {
    if !(-128..=127).contains(&imm_i64) {
        return Err(EncodeError::Unsupported(
            "cmp_b imm must fit in signed byte; use a smaller value or load into reg first",
        ));
    }
    let imm_i8 = imm_i64 as i8;
    if dest_reg_id == 0 {
        // cmp al, imm8 → 3C ib
        buf.bytes.push(0x3C);
        buf.bytes.push(imm_i8 as u8);
    } else {
        let reg_id = dest_reg_id & 7;
        if (dest_reg_id >> 3) != 0 {
            buf.bytes.push(0x41); // REX.B for r8b–r15b
        }
        buf.bytes.push(0x80);
        buf.bytes.push(0xF8 | reg_id);
        buf.bytes.push(imm_i8 as u8);
    }
    Ok(EncodeOutput::new())
}

/// #1254: `cmp reg16, imm{8sxt,16}` — 66 prefix + [REX] path.
///
/// AX short form: 66 3D iw for `cmp ax, imm16`.
/// imm8 sign-extension short form: 66 [REX] 83 /7 ib when imm fits i8.
/// General form: 66 [REX] 81 /7 iw.
pub(super) fn encode_cmp_reg16_imm(
    buf: &mut CodeBuffer,
    dest_reg_id: u8,
    imm_i64: i64,
) -> Result<EncodeOutput, EncodeError> {
    if imm_i64 < i16::MIN as i64 || imm_i64 > i16::MAX as i64 {
        return Err(EncodeError::Unsupported(
            "cmp_w imm must fit in signed 16-bit; use a smaller value or load into reg first",
        ));
    }
    buf.bytes.push(0x66); // operand-size override
    let reg_low = dest_reg_id & 7;
    let needs_rex_b = (dest_reg_id >> 3) != 0;
    // imm8 sign-extend short form via /7 subgroup.
    if (-128..=127).contains(&imm_i64) {
        if needs_rex_b {
            buf.bytes.push(0x41);
        }
        buf.bytes.push(0x83);
        buf.bytes.push(0xF8 | reg_low);
        buf.bytes.push(imm_i64 as i8 as u8);
    } else if dest_reg_id == 0 {
        // cmp ax, imm16 short form: 66 3D iw
        buf.bytes.push(0x3D);
        buf.bytes.extend((imm_i64 as i16).to_le_bytes());
    } else {
        if needs_rex_b {
            buf.bytes.push(0x41);
        }
        buf.bytes.push(0x81);
        buf.bytes.push(0xF8 | reg_low);
        buf.bytes.extend((imm_i64 as i16).to_le_bytes());
    }
    Ok(EncodeOutput::new())
}

/// #1254: `cmp reg32, imm{8sxt,32}` — [REX] path, no operand-size override.
///
/// EAX short form: 3D id for `cmp eax, imm32`.
/// imm8 sign-extension short form: [REX] 83 /7 ib when imm fits i8.
/// General form: [REX] 81 /7 id.
pub(super) fn encode_cmp_reg32_imm(
    buf: &mut CodeBuffer,
    dest_reg_id: u8,
    imm_i64: i64,
) -> Result<EncodeOutput, EncodeError> {
    if imm_i64 < i32::MIN as i64 || imm_i64 > i32::MAX as i64 {
        return Err(EncodeError::Unsupported(
            "cmp_d imm must fit in signed 32-bit; use a smaller value or load into reg first",
        ));
    }
    let reg_low = dest_reg_id & 7;
    let needs_rex_b = (dest_reg_id >> 3) != 0;
    if (-128..=127).contains(&imm_i64) {
        if needs_rex_b {
            buf.bytes.push(0x41);
        }
        buf.bytes.push(0x83);
        buf.bytes.push(0xF8 | reg_low);
        buf.bytes.push(imm_i64 as i8 as u8);
    } else if dest_reg_id == 0 {
        // cmp eax, imm32 short form: 3D id
        buf.bytes.push(0x3D);
        buf.bytes.extend((imm_i64 as i32).to_le_bytes());
    } else {
        if needs_rex_b {
            buf.bytes.push(0x41);
        }
        buf.bytes.push(0x81);
        buf.bytes.push(0xF8 | reg_low);
        buf.bytes.extend((imm_i64 as i32).to_le_bytes());
    }
    Ok(EncodeOutput::new())
}

/// #1254: `cmp reg8, reg8` — [REX] 38 /r.
///
/// ModR/M encoding: mod=11, reg=<src>, rm=<dest> (per Intel Vol 2A: 38 /r
/// means r/m8 is the first operand, r8 is the second; the reg field
/// carries the source register).
pub(super) fn encode_cmp_reg8_reg8(
    buf: &mut CodeBuffer,
    dest_id: u8,
    src_id: u8,
) -> Result<EncodeOutput, EncodeError> {
    let dest_low = dest_id & 7;
    let src_low = src_id & 7;
    let rex_b = (dest_id >> 3) != 0;
    let rex_r = (src_id >> 3) != 0;
    if rex_b || rex_r {
        buf.bytes.push(
            0x40 | if rex_r { 0x04 } else { 0 } | if rex_b { 0x01 } else { 0 },
        );
    }
    buf.bytes.push(0x38);
    buf.bytes.push(0xC0 | (src_low << 3) | dest_low);
    Ok(EncodeOutput::new())
}

/// #1254: `cmp reg16, reg16` — 66 [REX] 39 /r.
pub(super) fn encode_cmp_reg16_reg16(
    buf: &mut CodeBuffer,
    dest_id: u8,
    src_id: u8,
) -> Result<EncodeOutput, EncodeError> {
    buf.bytes.push(0x66); // operand-size override
    let dest_low = dest_id & 7;
    let src_low = src_id & 7;
    let rex_b = (dest_id >> 3) != 0;
    let rex_r = (src_id >> 3) != 0;
    if rex_b || rex_r {
        buf.bytes.push(
            0x40 | if rex_r { 0x04 } else { 0 } | if rex_b { 0x01 } else { 0 },
        );
    }
    buf.bytes.push(0x39);
    buf.bytes.push(0xC0 | (src_low << 3) | dest_low);
    Ok(EncodeOutput::new())
}

/// #1254: `cmp reg32, reg32` — [REX] 39 /r.
pub(super) fn encode_cmp_reg32_reg32(
    buf: &mut CodeBuffer,
    dest_id: u8,
    src_id: u8,
) -> Result<EncodeOutput, EncodeError> {
    let dest_low = dest_id & 7;
    let src_low = src_id & 7;
    let rex_b = (dest_id >> 3) != 0;
    let rex_r = (src_id >> 3) != 0;
    if rex_b || rex_r {
        buf.bytes.push(
            0x40 | if rex_r { 0x04 } else { 0 } | if rex_b { 0x01 } else { 0 },
        );
    }
    buf.bytes.push(0x39);
    buf.bytes.push(0xC0 | (src_low << 3) | dest_low);
    Ok(EncodeOutput::new())
}

/// #1254: `cmp [base + disp], reg8` — [REX] 38 /r.
pub(super) fn encode_cmp_mem_reg8(
    buf: &mut CodeBuffer,
    base_id: u8,
    disp: i32,
    src_id: u8,
) -> Result<EncodeOutput, EncodeError> {
    let rex_r = (src_id >> 3) != 0;
    let rex_b = (base_id >> 3) != 0;
    if rex_r || rex_b {
        buf.bytes.push(
            0x40 | if rex_r { 0x04 } else { 0 } | if rex_b { 0x01 } else { 0 },
        );
    }
    buf.bytes.push(0x38);
    crate::encode::emit_mem_base_disp(buf, src_id & 7, base_id, disp);
    Ok(EncodeOutput::new())
}

/// #1254: `cmp [base + disp], reg16` — 66 [REX] 39 /r.
pub(super) fn encode_cmp_mem_reg16(
    buf: &mut CodeBuffer,
    base_id: u8,
    disp: i32,
    src_id: u8,
) -> Result<EncodeOutput, EncodeError> {
    buf.bytes.push(0x66);
    let rex_r = (src_id >> 3) != 0;
    let rex_b = (base_id >> 3) != 0;
    if rex_r || rex_b {
        buf.bytes.push(
            0x40 | if rex_r { 0x04 } else { 0 } | if rex_b { 0x01 } else { 0 },
        );
    }
    buf.bytes.push(0x39);
    crate::encode::emit_mem_base_disp(buf, src_id & 7, base_id, disp);
    Ok(EncodeOutput::new())
}

/// #1254: `cmp [base + disp], reg32` — [REX] 39 /r.
pub(super) fn encode_cmp_mem_reg32(
    buf: &mut CodeBuffer,
    base_id: u8,
    disp: i32,
    src_id: u8,
) -> Result<EncodeOutput, EncodeError> {
    let rex_r = (src_id >> 3) != 0;
    let rex_b = (base_id >> 3) != 0;
    if rex_r || rex_b {
        buf.bytes.push(
            0x40 | if rex_r { 0x04 } else { 0 } | if rex_b { 0x01 } else { 0 },
        );
    }
    buf.bytes.push(0x39);
    crate::encode::emit_mem_base_disp(buf, src_id & 7, base_id, disp);
    Ok(EncodeOutput::new())
}

pub(super) fn encode_test(inst: &Instruction, buf: &mut CodeBuffer) -> Result<EncodeOutput, EncodeError> {
    // Phase 7 m1-001: test r64, r64 for condition testing.
    // Operands: [register, register] for "test rdi, rdi" shape.
    // PA-R13-006 (issue #935): [register, imm64-in-i32-range] for
    // "test r64, imm32" — REX.W F7 /0 id (with 48 A9 id short form for RAX).
    match inst.operands.as_slice() {
        [Operand::Reg(dest), Operand::Reg(src)] => {
            // test r64, r64 → 48 85 <ModR/M>
            test_reg64_reg64(buf, reg64_from(*dest)?, reg64_from(*src)?);
            Ok(EncodeOutput::new())
        }
        [Operand::Reg(dest), Operand::Imm64(imm)] => {
            let dest_reg = reg64_from(*dest)?;
            let imm_i64 = *imm;

            // TEST has no imm8 sign-extended form (unlike CMP/ADD/SUB — the
            // 83 /X ib subgroup doesn't include /0=TEST). All immediates go
            // through F7 /0 id (or the A9 id short form for RAX).
            if imm_i64 < i32::MIN as i64 || imm_i64 > i32::MAX as i64 {
                return Err(EncodeError::Unsupported(
                    "64-bit immediate test not yet supported; use and+cmp workaround",
                ));
            }
            test_reg64_imm32(buf, dest_reg, imm_i64 as i32);
            Ok(EncodeOutput::new())
        }
        _ => Err(EncodeError::Unsupported(
            "test form not supported: expected reg64,reg64 or reg64,imm32",
        )),
    }
}
