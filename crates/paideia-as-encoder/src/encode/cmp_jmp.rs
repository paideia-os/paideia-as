//! CMP/TEST (register and immediate), JMP rel8/rel32, and indirect JMP via SIB.

use super::types::*;
use crate::encode_instruction::EncodeError;

/// Encode `cmp [base + disp], src` (compare memory with register).
///
/// Instruction: REX.W 39 /r
/// Operand-size: 64-bit
/// ModR/M: depends on displacement encoding (disp8 or disp32)
pub fn cmp_mem_reg64_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64) {
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x39);
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `cmp reg64, imm8` (8-bit immediate, sign-extended to 64-bit).
///
/// Instruction: REX.W 83 /7 ib
/// ModR/M: 0xF8 | (reg & 7) (register 7 in the reg field means cmp)
/// Bytes: `48 83 (0xF8 | reg) imm8`
///
/// Example: `cmp rax, 0` → `48 83 F8 00`
pub fn cmp_reg64_imm8(buf: &mut CodeBuffer, dst: Reg64, imm: i8) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x83);
    buf.bytes.push(0xF8 | (reg_id & 7));
    buf.bytes.push(imm as u8);
}

/// Encode `cmp reg64, imm32` (32-bit immediate, sign-extended to 64-bit).
///
/// Instruction: REX.W 81 /7 id
/// ModR/M: 0xF8 | (reg & 7) (register 7 in the reg field means cmp)
/// Bytes: `48 81 (0xF8 | reg) imm32_le`
pub fn cmp_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x81);
    buf.bytes.push(0xF8 | (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `test reg64, reg64`.
///
/// Instruction: REX.W 85 /r
/// ModR/M: 0xC0 | (src<<3) | dst
pub fn test_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (dst_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x85);
    buf.bytes.push(0xC0 | ((src_id & 7) << 3) | (dst_id & 7));
}

/// Encode `test reg64, imm32` (32-bit immediate, sign-extended to 64-bit) — PA-R13-006 (issue #935).
///
/// RAX uses the short form `REX.W A9 id` (6 bytes total). All other GPRs use
/// the general `REX.W F7 /0 id` form (7 bytes total). The `/0` opcode extension
/// lives in the `reg` field of ModR/M, so ModR/M = `mod=11 | reg=000 | rm=dst`
/// = `0xC0 | (dst & 7)`.
///
/// REX.W=1 is always required for 64-bit operand size; REX.B=dst>>3 for
/// extended registers (R8..R15).
///
/// The immediate is sign-extended by the CPU from imm32 to the 64-bit operand,
/// so callers should keep values in `i32::MIN..=i32::MAX`. Ranged dispatch is
/// performed by `encode_test`; this primitive takes an `i32` directly.
///
/// Example: `test rax, 0x100`  → `48 A9 00 01 00 00 00`      (short form, 6 bytes)
/// Example: `test rbx, 0x100`  → `48 F7 C3 00 01 00 00 00`   (general form, 7 bytes)
/// Example: `test r8,  1`      → `49 F7 C0 01 00 00 00 00`
/// Example: `test rdi, -1`     → `48 F7 C7 FF FF FF FF`      (sign-extended)
pub fn test_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    if matches!(dst, Reg64::Rax) {
        // Short form: REX.W A9 id (48 A9 <imm32>)
        buf.bytes.push(rex(true, false, false, false));
        buf.bytes.push(0xA9);
        buf.bytes.extend(imm.to_le_bytes());
    } else {
        // General form: REX.W F7 /0 id
        let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
        buf.bytes.push(rex_byte);
        buf.bytes.push(0xF7);
        buf.bytes.push(0xC0 | (reg_id & 7));
        buf.bytes.extend(imm.to_le_bytes());
    }
}

/// Encode `jmp rel8` (short jump).
///
/// Instruction: EB cb (displacement is relative to end of instruction)
/// Total size: 2 bytes
pub fn jmp_rel8(buf: &mut CodeBuffer, rel: i8) {
    buf.bytes.push(0xEB);
    buf.bytes.push(rel as u8);
}

/// Encode `jmp rel32` (near jump).
///
/// Instruction: E9 cd (displacement is relative to end of instruction)
/// Total size: 5 bytes
pub fn jmp_rel32(buf: &mut CodeBuffer, rel: i32) {
    buf.bytes.push(0xE9);
    buf.bytes.extend(rel.to_le_bytes());
}

/// Encode `jmp [disp32 + index*scale]` with no base register and no RIP.
///
/// PA-R15-009a: Emits `FF 24 <SIB> <disp32>` with SIB base=0b101 (no base).
/// This encodes absolute addressing at a link-known address, suitable for kernel-mapped
/// jump tables. x86-64 has no true RIP+SIB mode; this uses the mod=00, base=101 form
/// which normally means RIP-relative in 64-bit mode, but with explicit disp32 it
/// becomes `[disp32 + index*scale]` absolute.
///
/// Arguments:
/// - buf: code buffer to write to
/// - index: index register (cannot be RSP; id 4 is reserved for "no index")
/// - scale_bits: 0=1x, 1=2x, 2=4x, 3=8x
/// - disp32: absolute 32-bit displacement (will be relocated by linker)
///
/// Returns: byte offset of disp32 within the instruction (3 for registers 0-7, 4 for REX.X).
///
/// Instruction encoding:
/// - [REX.X if index >= 8] 0xFF 0x24 SIB disp32
/// - REX = 0x42 (only if index >= R8, i.e., REX.X=1)
/// - SIB = (scale<<6) | ((index&7)<<3) | 0b101
///
/// Rejects: index=RSP (id 4) → EncodeError::InvalidOperand
pub fn jmp_mem_sib_no_base_indexed(
    buf: &mut CodeBuffer,
    index: Reg64,
    scale_bits: u8,
    disp32: i32,
) -> Result<usize, EncodeError> {
    let index_id = index as u8;

    // Reject RSP as index (index field 0b100 is reserved for "no index" in SIB)
    if index_id == 4 {
        return Err(EncodeError::InvalidOperand("RSP cannot be used as SIB index register"));
    }

    let disp_offset;

    // Emit REX.X if index >= R8 (index_id >= 8)
    if (index_id >> 3) != 0 {
        buf.bytes.push(0x42); // REX prefix with X=1
        disp_offset = 4; // disp32 starts at byte +4 after 0xFF, 0x24, SIB
    } else {
        disp_offset = 3; // disp32 starts at byte +3 after 0xFF, 0x24, SIB
    }

    // Emit: FF 24 SIB disp32
    buf.bytes.push(0xFF);
    buf.bytes.push(0x24); // ModRM: mod=00, reg=100, r/m=100 (SIB follows)

    // SIB byte: (scale_bits << 6) | ((index_id & 7) << 3) | 0b101 (base=101)
    let sib = ((scale_bits & 3) << 6) | ((index_id & 7) << 3) | 0b101;
    buf.bytes.push(sib);

    // Emit disp32
    buf.bytes.extend(disp32.to_le_bytes());

    Ok(disp_offset)
}

