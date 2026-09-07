//! Absolute-displacement (disp32) encoders: LOCK INC, LOCK ADD imm8/imm32, and MOV reg/mem/imm variants using [disp32] addressing.

use super::types::*;
use paideia_as_ir::instruction::IntWidth;

/// Encode `lock inc qword [disp32]` via SIB no-base absolute form.
///
/// Instruction: F0 REX.W FF /0 modrm=0x04 sib=0x25 disp32
/// Total size: 9 bytes (for W64)
/// Returns the byte offset where disp32 starts within the instruction.
///
/// PA-R16-007 (issue #1060): Used for atomic increment at absolute addresses,
/// typically with GS-prefix for per-cpu access (MemSeg pre-pass applies prefix).
pub fn lock_inc_mem_abs_disp32(
    buf: &mut CodeBuffer,
    width: IntWidth,
    disp32: i32,
) -> usize {
    buf.bytes.push(0xF0); // LOCK prefix
    let disp_offset;
    match width {
        IntWidth::W64 => {
            buf.bytes.push(0x48); // REX.W
            disp_offset = 4; // disp32 starts at byte offset 4: F0 48 FF 04 25 [disp32...]
        }
        IntWidth::W32 => {
            // W32 form has no REX.W
            disp_offset = 3; // disp32 starts at byte offset 3: F0 FF 04 25 [disp32...]
        }
        _ => unreachable!(),
    }

    buf.bytes.push(0xFF); // opcode for inc
    buf.bytes.push(0x04); // ModRM: mod=00, reg=0 (/0 for inc), r/m=100 (SIB follows)
    buf.bytes.push(0x25); // SIB: scale=00, index=100 (no index), base=101 (no base/absolute)
    buf.bytes.extend(disp32.to_le_bytes());

    disp_offset
}

/// Encode `lock add qword [disp32], imm8` via SIB no-base absolute form.
///
/// Instruction: F0 REX.W 83 /0 modrm=0x04 sib=0x25 disp32 imm8
/// Total size: 10 bytes (for W64)
/// Returns the byte offset where disp32 starts within the instruction.
///
/// PA-R16-007 (issue #1060): Used for atomic add at absolute addresses,
/// typically with GS-prefix for per-cpu access (MemSeg pre-pass applies prefix).
pub fn lock_add_mem_abs_disp32_imm8(
    buf: &mut CodeBuffer,
    width: IntWidth,
    disp32: i32,
    imm8: i8,
) -> usize {
    buf.bytes.push(0xF0); // LOCK prefix
    let disp_offset;
    match width {
        IntWidth::W64 => {
            buf.bytes.push(0x48); // REX.W
            disp_offset = 4; // disp32 starts at byte offset 4: F0 48 83 04 25 [disp32...]
        }
        IntWidth::W32 => {
            // W32 form has no REX.W
            disp_offset = 3; // disp32 starts at byte offset 3: F0 83 04 25 [disp32...]
        }
        _ => unreachable!(),
    }

    buf.bytes.push(0x83); // opcode for add imm8
    buf.bytes.push(0x04); // ModRM: mod=00, reg=0 (/0 for add), r/m=100 (SIB follows)
    buf.bytes.push(0x25); // SIB: scale=00, index=100 (no index), base=101 (no base/absolute)
    buf.bytes.extend(disp32.to_le_bytes());
    buf.bytes.push(imm8 as u8);

    disp_offset
}

/// Encode `lock add qword [disp32], imm32` via SIB no-base absolute form.
///
/// Instruction: F0 REX.W 81 /0 modrm=0x04 sib=0x25 disp32 imm32
/// Total size: 13 bytes (for W64)
/// Returns the byte offset where disp32 starts within the instruction.
///
/// PA-R16-007 (issue #1060): Used for atomic add at absolute addresses,
/// typically with GS-prefix for per-cpu access (MemSeg pre-pass applies prefix).
pub fn lock_add_mem_abs_disp32_imm32(
    buf: &mut CodeBuffer,
    width: IntWidth,
    disp32: i32,
    imm32: i32,
) -> usize {
    buf.bytes.push(0xF0); // LOCK prefix
    let disp_offset;
    match width {
        IntWidth::W64 => {
            buf.bytes.push(0x48); // REX.W
            disp_offset = 4; // disp32 starts at byte offset 4: F0 48 81 04 25 [disp32...]
        }
        IntWidth::W32 => {
            // W32 form has no REX.W
            disp_offset = 3; // disp32 starts at byte offset 3: F0 81 04 25 [disp32...]
        }
        _ => unreachable!(),
    }

    buf.bytes.push(0x81); // opcode for add imm32
    buf.bytes.push(0x04); // ModRM: mod=00, reg=0 (/0 for add), r/m=100 (SIB follows)
    buf.bytes.push(0x25); // SIB: scale=00, index=100 (no index), base=101 (no base/absolute)
    buf.bytes.extend(disp32.to_le_bytes());
    buf.bytes.extend(imm32.to_le_bytes());

    disp_offset
}

/// Emit `mov reg, [disp32]` via SIB no-base absolute form.
///
/// Widths: W8 → 8A + no REX.W; W16 → 66 + 8B + no REX.W;
/// W32 → 8B + no REX.W; W64 → REX.W + 8B.
/// ModRM: mod=00, reg=dst_code, r/m=100 (SIB follows).
/// SIB: scale=00, index=100 (no index), base=101 (no base/absolute).
/// Returns disp32 byte offset within the instruction for reloc symmetry
/// with #1060 (no relocs today; kept for API consistency).
pub fn mov_reg_mem_abs_disp32(
    buf: &mut CodeBuffer,
    width: IntWidth,
    dst: Reg64,
    disp32: i32,
) -> usize {
    let dst_id = dst as u8;
    let disp_offset;

    // Handle width-specific prefixes and REX
    match width {
        IntWidth::W8 => {
            // 8A (mov r8, r/m8)
            // No REX.W, but emit REX for high registers (r8-r15)
            if (dst_id >> 3) != 0 {
                buf.bytes.push(rex(false, (dst_id >> 3) != 0, false, false));
            }
            disp_offset = buf.bytes.len();
            buf.bytes.push(0x8A);
        }
        IntWidth::W16 => {
            // 66 8B (mov r16, r/m16)
            // Per Intel SDM Vol 2A §2.1.1: legacy prefixes (0x66) MUST precede REX
            buf.bytes.push(0x66); // operand-size override
            if (dst_id >> 3) != 0 {
                buf.bytes.push(rex(false, (dst_id >> 3) != 0, false, false));
            }
            disp_offset = buf.bytes.len();
            buf.bytes.push(0x8B);
        }
        IntWidth::W32 => {
            // 8B (mov r32, r/m32)
            // Emit REX if high register (r8-r15)
            if (dst_id >> 3) != 0 {
                buf.bytes.push(rex(false, (dst_id >> 3) != 0, false, false));
            }
            disp_offset = buf.bytes.len();
            buf.bytes.push(0x8B);
        }
        IntWidth::W64 => {
            // REX.W 8B (mov r64, r/m64)
            buf.bytes.push(rex(true, (dst_id >> 3) != 0, false, false));
            disp_offset = buf.bytes.len();
            buf.bytes.push(0x8B);
        }
    }

    buf.bytes.push(0x04 | ((dst_id & 7) << 3)); // ModRM: mod=00, reg=dst_code, r/m=100 (SIB)
    buf.bytes.push(0x25); // SIB: scale=00, index=100, base=101 (absolute)
    buf.bytes.extend(disp32.to_le_bytes());

    disp_offset
}

/// Emit `mov [disp32], reg` via SIB no-base absolute form.
///
/// Widths: W8 → 88; W16 → 66 + 89; W32 → 89; W64 → REX.W + 89.
pub fn mov_mem_abs_disp32_reg(
    buf: &mut CodeBuffer,
    width: IntWidth,
    disp32: i32,
    src: Reg64,
) -> usize {
    let src_id = src as u8;
    let disp_offset;

    // Handle width-specific prefixes and REX
    match width {
        IntWidth::W8 => {
            // 88 (mov r/m8, r8)
            // No REX.W, but emit REX for high registers (r8-r15)
            if (src_id >> 3) != 0 {
                buf.bytes.push(rex(false, (src_id >> 3) != 0, false, false));
            }
            disp_offset = buf.bytes.len();
            buf.bytes.push(0x88);
        }
        IntWidth::W16 => {
            // 66 89 (mov r/m16, r16)
            // Per Intel SDM Vol 2A §2.1.1: legacy prefixes (0x66) MUST precede REX
            buf.bytes.push(0x66); // operand-size override
            if (src_id >> 3) != 0 {
                buf.bytes.push(rex(false, (src_id >> 3) != 0, false, false));
            }
            disp_offset = buf.bytes.len();
            buf.bytes.push(0x89);
        }
        IntWidth::W32 => {
            // 89 (mov r/m32, r32)
            // Emit REX if high register (r8-r15)
            if (src_id >> 3) != 0 {
                buf.bytes.push(rex(false, (src_id >> 3) != 0, false, false));
            }
            disp_offset = buf.bytes.len();
            buf.bytes.push(0x89);
        }
        IntWidth::W64 => {
            // REX.W 89 (mov r/m64, r64)
            buf.bytes.push(rex(true, (src_id >> 3) != 0, false, false));
            disp_offset = buf.bytes.len();
            buf.bytes.push(0x89);
        }
    }

    buf.bytes.push(0x04 | ((src_id & 7) << 3)); // ModRM: mod=00, reg=src_code, r/m=100 (SIB)
    buf.bytes.push(0x25); // SIB: scale=00, index=100, base=101 (absolute)
    buf.bytes.extend(disp32.to_le_bytes());

    disp_offset
}

/// Emit `mov [disp32], imm` via SIB no-base absolute form.
///
/// Widths: W8 → C6 + imm8; W16 → 66 + C7 + imm16; W32 → C7 + imm32;
/// W64 → REX.W + C7 + imm32 (sign-extended to 64 bits, so the imm must
/// fit in i32 range). Callers should validate range and emit a
/// diagnostic (e.g. T0553) if the u64 value overflows.
/// ModRM.reg = 0 (opcode extension /0), r/m = 100, SIB = 0x25.
pub fn mov_mem_abs_disp32_imm(
    buf: &mut CodeBuffer,
    width: IntWidth,
    disp32: i32,
    imm: i64,
) -> usize {
    let disp_offset;

    // Handle width-specific prefixes, opcodes, and immediates
    match width {
        IntWidth::W8 => {
            // C6 /0 ib (mov r/m8, imm8)
            disp_offset = buf.bytes.len();
            buf.bytes.push(0xC6);
            buf.bytes.push(0x04); // ModRM: mod=00, reg=0 (/0), r/m=100 (SIB)
            buf.bytes.push(0x25); // SIB: scale=00, index=100, base=101 (absolute)
            buf.bytes.extend(disp32.to_le_bytes());
            buf.bytes.push((imm & 0xFF) as u8);
        }
        IntWidth::W16 => {
            // 66 C7 /0 iw (mov r/m16, imm16)
            buf.bytes.push(0x66); // operand-size override
            disp_offset = buf.bytes.len();
            buf.bytes.push(0xC7);
            buf.bytes.push(0x04); // ModRM: mod=00, reg=0 (/0), r/m=100 (SIB)
            buf.bytes.push(0x25); // SIB: scale=00, index=100, base=101 (absolute)
            buf.bytes.extend(disp32.to_le_bytes());
            buf.bytes.extend(((imm & 0xFFFF) as i16).to_le_bytes());
        }
        IntWidth::W32 => {
            // C7 /0 id (mov r/m32, imm32)
            disp_offset = buf.bytes.len();
            buf.bytes.push(0xC7);
            buf.bytes.push(0x04); // ModRM: mod=00, reg=0 (/0), r/m=100 (SIB)
            buf.bytes.push(0x25); // SIB: scale=00, index=100, base=101 (absolute)
            buf.bytes.extend(disp32.to_le_bytes());
            buf.bytes.extend((imm as i32).to_le_bytes());
        }
        IntWidth::W64 => {
            // REX.W C7 /0 id (mov r/m64, imm32 sign-extended)
            // The immediate must fit in i32 range; if not, error should be caught by elaborator
            debug_assert!(
                (imm as i64 >= i32::MIN as i64) && (imm as i64 <= i32::MAX as i64),
                "mov [disp32], imm: immediate {} does not fit in i32 range",
                imm
            );
            buf.bytes.push(0x48); // REX.W
            disp_offset = buf.bytes.len();
            buf.bytes.push(0xC7);
            buf.bytes.push(0x04); // ModRM: mod=00, reg=0 (/0), r/m=100 (SIB)
            buf.bytes.push(0x25); // SIB: scale=00, index=100, base=101 (absolute)
            buf.bytes.extend(disp32.to_le_bytes());
            buf.bytes.extend((imm as i32).to_le_bytes());
        }
    }

    disp_offset
}

