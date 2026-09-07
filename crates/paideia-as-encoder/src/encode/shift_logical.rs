//! Shift / rotate (SAR, ROL, ROR) and logical (AND, OR, XOR, IMUL) encoders — register, immediate, and memory forms.

use super::types::*;

/// Encode `sar reg64, imm8` (arithmetic right shift by immediate).
///
/// Instruction: REX.W C1 /7 ib
/// ModR/M: 0xF8 | (reg & 7) (register 7 in the reg field means SAR)
/// Bytes: `48+REX C1 (0xF8 | (reg&7)) imm8`
///
/// Example: `sar rax, 3` → `48 c1 f8 03`
pub fn sar_reg64_imm8(buf: &mut CodeBuffer, reg: Reg64, imm: u8) {
    let reg_id = reg as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xC1);
    buf.bytes.push(0xF8 | (reg_id & 7));
    buf.bytes.push(imm);
}

/// Encode `rol reg64, imm8` or `rol reg64, 1` (rotate left).
///
/// Instruction: REX.W C1 /0 ib (general form) or REX.W D1 /0 (short form for imm=1)
/// ModR/M: 0xC0 | (reg & 7)
/// Bytes for imm != 1: `48+REX.B C1 (0xC0 | (reg&7)) imm8`
/// Bytes for imm == 1: `48+REX.B D1 (0xC0 | (reg&7))`
///
/// Example: `rol rax, 1` → `48 d1 c0`
/// Example: `rol rax, 4` → `48 c1 c0 04`
/// Example: `rol r15, 3` → `49 c1 c7 03`
pub fn rol_reg64_imm8(buf: &mut CodeBuffer, reg: Reg64, imm: u8) {
    let reg_id = reg as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);

    if imm == 1 {
        // Short form: REX.W D1 (0xC0 | reg)
        buf.bytes.push(rex_byte);
        buf.bytes.push(0xD1);
        buf.bytes.push(0xC0 | (reg_id & 7));
    } else {
        // General form: REX.W C1 (0xC0 | reg) imm8
        buf.bytes.push(rex_byte);
        buf.bytes.push(0xC1);
        buf.bytes.push(0xC0 | (reg_id & 7));
        buf.bytes.push(imm);
    }
}

/// Encode `rol reg64, cl` (rotate left by CL).
///
/// Instruction: REX.W D3 /0
/// ModR/M: 0xC0 | (reg & 7)
/// Bytes: `48+REX.B D3 (0xC0 | (reg&7))`
///
/// Example: `rol rax, cl` → `48 d3 c0`
pub fn rol_reg64_cl(buf: &mut CodeBuffer, reg: Reg64) {
    let reg_id = reg as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xD3);
    buf.bytes.push(0xC0 | (reg_id & 7));
}

/// Encode `rol reg32, imm8` or `rol reg32, 1` (rotate left, 32-bit).
///
/// Instruction: C1 /0 ib (general form) or D1 /0 (short form for imm=1)
/// ModR/M: 0xC0 | (reg & 7)
/// Bytes for imm != 1: `REX.B(if needed) C1 (0xC0 | (reg&7)) imm8`
/// Bytes for imm == 1: `REX.B(if needed) D1 (0xC0 | (reg&7))`
///
/// Example: `rol eax, 1` → `d1 c0`
/// Example: `rol r8d, 2` → `41 c1 c0 02`
pub fn rol_reg32_imm8(buf: &mut CodeBuffer, reg_id: u8, imm: u8) {
    if imm == 1 {
        // Short form
        if (reg_id >> 3) != 0 {
            buf.bytes.push(rex(false, false, false, true));
        }
        buf.bytes.push(0xD1);
        buf.bytes.push(0xC0 | (reg_id & 7));
    } else {
        // General form
        if (reg_id >> 3) != 0 {
            buf.bytes.push(rex(false, false, false, true));
        }
        buf.bytes.push(0xC1);
        buf.bytes.push(0xC0 | (reg_id & 7));
        buf.bytes.push(imm);
    }
}

/// Encode `rol reg32, cl` (rotate left by CL, 32-bit).
///
/// Instruction: D3 /0
/// ModR/M: 0xC0 | (reg & 7)
/// Bytes: `REX.B(if needed) D3 (0xC0 | (reg&7))`
///
/// Example: `rol eax, cl` → `d3 c0`
/// Example: `rol r8d, cl` → `41 d3 c0`
pub fn rol_reg32_cl(buf: &mut CodeBuffer, reg_id: u8) {
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xD3);
    buf.bytes.push(0xC0 | (reg_id & 7));
}

/// Encode `rol reg16, imm8` or `rol reg16, 1` (rotate left, 16-bit).
///
/// Instruction: 0x66 C1 /0 ib (general form) or 0x66 D1 /0 (short form for imm=1)
/// ModR/M: 0xC0 | (reg & 7)
/// Bytes for imm != 1: `0x66 [REX.B(if needed)] C1 (0xC0 | (reg&7)) imm8`
/// Bytes for imm == 1: `0x66 [REX.B(if needed)] D1 (0xC0 | (reg&7))`
///
/// Example: `rol ax, 1` → `66 d1 c0`
/// Example: `rol ax, 8` → `66 c1 c0 08`
/// Example: `rol r10w, 8` → `66 41 c1 c2 08`
pub fn rol_reg16_imm8(buf: &mut CodeBuffer, reg_id: u8, imm: u8) {
    if imm == 1 {
        // Short form
        buf.bytes.push(0x66);
        if (reg_id >> 3) != 0 {
            buf.bytes.push(rex(false, false, false, true));
        }
        buf.bytes.push(0xD1);
        buf.bytes.push(0xC0 | (reg_id & 7));
    } else {
        // General form
        buf.bytes.push(0x66);
        if (reg_id >> 3) != 0 {
            buf.bytes.push(rex(false, false, false, true));
        }
        buf.bytes.push(0xC1);
        buf.bytes.push(0xC0 | (reg_id & 7));
        buf.bytes.push(imm);
    }
}

/// Encode `rol reg16, cl` (rotate left by CL, 16-bit).
///
/// Instruction: 0x66 D3 /0
/// ModR/M: 0xC0 | (reg & 7)
/// Bytes: `0x66 [REX.B(if needed)] D3 (0xC0 | (reg&7))`
///
/// Example: `rol ax, cl` → `66 d3 c0`
/// Example: `rol r15w, cl` → `66 41 d3 c7`
pub fn rol_reg16_cl(buf: &mut CodeBuffer, reg_id: u8) {
    buf.bytes.push(0x66);
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xD3);
    buf.bytes.push(0xC0 | (reg_id & 7));
}

/// Encode `ror reg64, imm8` or `ror reg64, 1` (rotate right).
///
/// Instruction: REX.W C1 /1 ib (general form) or REX.W D1 /1 (short form for imm=1)
/// ModR/M: 0xC8 | (reg & 7)
/// Bytes for imm != 1: `48+REX.B C1 (0xC8 | (reg&7)) imm8`
/// Bytes for imm == 1: `48+REX.B D1 (0xC8 | (reg&7))`
///
/// Example: `ror rax, 1` → `48 d1 c8`
/// Example: `ror rax, 4` → `48 c1 c8 04`
pub fn ror_reg64_imm8(buf: &mut CodeBuffer, reg: Reg64, imm: u8) {
    let reg_id = reg as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);

    if imm == 1 {
        // Short form: REX.W D1 (0xC8 | reg)
        buf.bytes.push(rex_byte);
        buf.bytes.push(0xD1);
        buf.bytes.push(0xC8 | (reg_id & 7));
    } else {
        // General form: REX.W C1 (0xC8 | reg) imm8
        buf.bytes.push(rex_byte);
        buf.bytes.push(0xC1);
        buf.bytes.push(0xC8 | (reg_id & 7));
        buf.bytes.push(imm);
    }
}

/// Encode `ror reg64, cl` (rotate right by CL).
///
/// Instruction: REX.W D3 /1
/// ModR/M: 0xC8 | (reg & 7)
/// Bytes: `48+REX.B D3 (0xC8 | (reg&7))`
///
/// Example: `ror rax, cl` → `48 d3 c8`
pub fn ror_reg64_cl(buf: &mut CodeBuffer, reg: Reg64) {
    let reg_id = reg as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xD3);
    buf.bytes.push(0xC8 | (reg_id & 7));
}

/// Encode `ror reg32, imm8` or `ror reg32, 1` (rotate right, 32-bit).
///
/// Instruction: C1 /1 ib (general form) or D1 /1 (short form for imm=1)
/// ModR/M: 0xC8 | (reg & 7)
/// Bytes for imm != 1: `REX.B(if needed) C1 (0xC8 | (reg&7)) imm8`
/// Bytes for imm == 1: `REX.B(if needed) D1 (0xC8 | (reg&7))`
///
/// Example: `ror eax, 1` → `d1 c8`
/// Example: `ror r8d, 2` → `41 c1 c8 02`
pub fn ror_reg32_imm8(buf: &mut CodeBuffer, reg_id: u8, imm: u8) {
    if imm == 1 {
        // Short form
        if (reg_id >> 3) != 0 {
            buf.bytes.push(rex(false, false, false, true));
        }
        buf.bytes.push(0xD1);
        buf.bytes.push(0xC8 | (reg_id & 7));
    } else {
        // General form
        if (reg_id >> 3) != 0 {
            buf.bytes.push(rex(false, false, false, true));
        }
        buf.bytes.push(0xC1);
        buf.bytes.push(0xC8 | (reg_id & 7));
        buf.bytes.push(imm);
    }
}

/// Encode `ror reg32, cl` (rotate right by CL, 32-bit).
///
/// Instruction: D3 /1
/// ModR/M: 0xC8 | (reg & 7)
/// Bytes: `REX.B(if needed) D3 (0xC8 | (reg&7))`
///
/// Example: `ror eax, cl` → `d3 c8`
/// Example: `ror r8d, cl` → `41 d3 c8`
pub fn ror_reg32_cl(buf: &mut CodeBuffer, reg_id: u8) {
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xD3);
    buf.bytes.push(0xC8 | (reg_id & 7));
}

/// Encode `xor reg64, reg64`.
///
/// Instruction: REX.W 31 /r
/// ModR/M: 0xC0 | (src<<3) | dst
pub fn xor_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (dst_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x31);
    buf.bytes.push(0xC0 | ((src_id & 7) << 3) | (dst_id & 7));
}

/// Encode `or reg64, imm8` (8-bit immediate, sign-extended to 64-bit).
///
/// Instruction: REX.W 83 /1 ib
/// ModR/M: 0xC8 | (reg & 7) (register 1 in the reg field means OR)
/// Bytes: `48+REX.B 83 (0xC8 | (reg&7)) imm8`
///
/// WARNING: Sign-extension trap. `83 /1 ib` sign-extends the immediate to 64 bits.
/// Only use this form if `imm == (imm as i8 as i64)` (round-trip through i8).
///
/// Example: `or rax, 0x20` → `48 83 c8 20`
/// Example: `or r15, 0x7f` → `49 83 cf 7f` (imm8 boundary)
pub fn or_reg64_imm8(buf: &mut CodeBuffer, dst: Reg64, imm: i8) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x83);
    buf.bytes.push(0xC8 | (reg_id & 7));
    buf.bytes.push(imm as u8);
}

/// Encode `or reg64, imm32` (32-bit immediate, sign-extended to 64-bit).
///
/// Instruction: REX.W 81 /1 id
/// ModR/M: 0xC8 | (reg & 7) (register 1 in the reg field means OR)
/// Bytes: `48+REX.B 81 (0xC8 | (reg&7)) imm32_le`
///
/// WARNING: Sign-extension trap. `81 /1 id` sign-extends the immediate to 64 bits.
/// Only use this form if `imm == (imm as i32 as i64)` (round-trip through i32).
///
/// Example: `or rax, 0x100` → `48 81 c8 00 01 00 00`
/// Example: `or r8, 0x80000001` → `49 81 c8 01 00 00 80` (high bit set)
pub fn or_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x81);
    buf.bytes.push(0xC8 | (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `or reg32, imm8` (8-bit immediate, zero-extended to 32-bit).
///
/// Instruction: 83 /1 ib
/// ModR/M: 0xC8 | (reg & 7) (register 1 in the reg field means OR)
/// Bytes: `rex_if_needed 83 (0xC8 | (reg&7)) imm8`
///
/// WARNING: When the high bit is set in the register ID (r8–r15), we emit REX.B.
/// Only use this form if `imm == (imm as i8 as i32)` (round-trip through i8).
///
/// Example: `or eax, 0x03` → `83 c8 03`
/// Example: `or r8d, 0x20` → `41 83 c8 20` (imm8 boundary with REX.B)
pub fn or_reg32_imm8(buf: &mut CodeBuffer, dst: Reg64, imm: i8) {
    let reg_id = dst as u8;
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x83);
    buf.bytes.push(0xC8 | (reg_id & 7));
    buf.bytes.push(imm as u8);
}

/// Encode `or reg32, imm32` (32-bit immediate).
///
/// Instruction: 81 /1 id
/// ModR/M: 0xC8 | (reg & 7)
/// Bytes: `rex_if_needed 81 (0xC8 | (reg&7)) imm32_le`
///
/// When the high bit is set in the register ID (r8–r15), we emit REX.B.
///
/// Example: `or eax, 0x100` → `81 c8 00 01 00 00`
/// Example: `or r8d, 0x80000001` → `41 81 c8 01 00 00 80`
pub fn or_reg32_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x81);
    buf.bytes.push(0xC8 | (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `and reg64, reg64`.
///
/// Instruction: REX.W 21 /r
/// ModR/M: 0xC0 | (src<<3) | dst
pub fn and_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (dst_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x21);
    buf.bytes.push(0xC0 | ((src_id & 7) << 3) | (dst_id & 7));
}

/// Encode `and reg64, imm8` (8-bit immediate, sign-extended to 64-bit).
///
/// Instruction: REX.W 83 /4 ib
/// ModR/M: 0xE0 | (reg & 7) (register 4 in the reg field means AND)
///
/// WARNING: Sign-extension trap. Only use this form if `imm == (imm as i8 as i64)`.
pub fn and_reg64_imm8(buf: &mut CodeBuffer, dst: Reg64, imm: i8) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x83);
    buf.bytes.push(0xE0 | (reg_id & 7));
    buf.bytes.push(imm as u8);
}

/// Encode `and reg64, imm32` (32-bit immediate, sign-extended to 64-bit).
///
/// Instruction: REX.W 81 /4 id
/// ModR/M: 0xE0 | (reg & 7)
///
/// WARNING: Sign-extension trap. Only use this form if `imm == (imm as i32 as i64)`.
pub fn and_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x81);
    buf.bytes.push(0xE0 | (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `or reg64, reg64`.
///
/// Instruction: REX.W 09 /r
/// ModR/M: 0xC0 | (src<<3) | dst
pub fn or_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (dst_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x09);
    buf.bytes.push(0xC0 | ((src_id & 7) << 3) | (dst_id & 7));
}

/// Encode `xor reg64, imm8` (8-bit immediate, sign-extended to 64-bit).
///
/// Instruction: REX.W 83 /6 ib
/// ModR/M: 0xF0 | (reg & 7) (register 6 in the reg field means XOR)
///
/// WARNING: Sign-extension trap. Only use this form if `imm == (imm as i8 as i64)`.
pub fn xor_reg64_imm8(buf: &mut CodeBuffer, dst: Reg64, imm: i8) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x83);
    buf.bytes.push(0xF0 | (reg_id & 7));
    buf.bytes.push(imm as u8);
}

/// Encode `xor reg64, imm32` (32-bit immediate, sign-extended to 64-bit).
///
/// Instruction: REX.W 81 /6 id
/// ModR/M: 0xF0 | (reg & 7)
///
/// WARNING: Sign-extension trap. Only use this form if `imm == (imm as i32 as i64)`.
pub fn xor_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x81);
    buf.bytes.push(0xF0 | (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `imul reg64, reg64` (2-operand form).
///
/// Instruction: REX.W 0F AF /r
/// ModR/M: 0xC0 | (dst<<3) | src (IMUL is inverted: dst in reg, src in r/m)
///
/// CAVEAT: IMUL ModR/M layout is INVERTED from standard r/m←r.
/// REX.R toggles on dst, REX.B on src.
pub fn imul_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    // IMUL: dst is in reg field (R extension), src is in r/m field (B extension)
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAF);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `imul reg64, reg64, imm8` (3-operand form with 8-bit immediate).
///
/// Instruction: REX.W 6B /r ib
/// ModR/M: 0xC0 | (dst<<3) | src (IMUL is inverted: dst in reg, src in r/m)
///
/// WARNING: Sign-extension trap. Only use this form if `imm == (imm as i8 as i64)`.
pub fn imul_reg64_reg64_imm8(buf: &mut CodeBuffer, dst: Reg64, src: Reg64, imm: i8) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x6B);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
    buf.bytes.push(imm as u8);
}

/// Encode `imul reg64, reg64, imm32` (3-operand form with 32-bit immediate).
///
/// Instruction: REX.W 69 /r id
/// ModR/M: 0xC0 | (dst<<3) | src
///
/// WARNING: Sign-extension trap. Only use this form if `imm == (imm as i32 as i64)`.
pub fn imul_reg64_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, src: Reg64, imm: i32) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x69);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `and reg64, [base + disp]` (register ← memory AND).
///
/// Instruction: REX.W 23 /r
/// Uses the smallest form possible (disp8 or disp32).
pub fn and_reg64_mem_reg64_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x23);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `or reg64, [base + disp]` (register ← memory OR).
///
/// Instruction: REX.W 0B /r
/// Uses the smallest form possible (disp8 or disp32).
pub fn or_reg64_mem_reg64_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0B);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `xor reg64, [base + disp]` (register ← memory XOR).
///
/// Instruction: REX.W 33 /r
/// Uses the smallest form possible (disp8 or disp32).
pub fn xor_reg64_mem_reg64_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x33);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `imul reg64, [base + disp]` (2-operand form with memory).
///
/// Instruction: REX.W 0F AF /r
/// IMUL inverted: dst in reg, src in r/m.
pub fn imul_reg64_mem_reg64_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAF);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

