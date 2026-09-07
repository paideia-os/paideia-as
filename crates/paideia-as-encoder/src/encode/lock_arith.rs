//! LOCK-prefixed atomic arithmetic and logical ops: XADD, ADD, SUB, AND, OR, XOR (mem+reg / mem+imm).

use super::types::*;

/// Encode `lock xadd [base + disp], src` — PA-R15-002 (issue #957).
///
/// Instruction: F0 REX.W 0F C1 /r
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
/// Atomically adds src to [base+disp] and stores old value in src. 64-bit form.
pub fn lock_xadd_mem_base_disp_reg64(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xC1);
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock xadd [base + disp], src` — PA-R15-002 (issue #957).
///
/// Instruction: F0 0F C1 /r (no REX.W for 32-bit)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
/// Atomically adds src to [base+disp] and stores old value in src. 32-bit form.
pub fn lock_xadd_mem_base_disp_reg32(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;

    // Only emit REX.B or REX.R if extended registers are used
    if (src_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, (src_id >> 3) != 0, false, (base_id >> 3) != 0));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xC1);

    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock add [base + disp], imm8` — PA-R15-003 (issue #958).
///
/// Instruction: F0 REX.W 83 /0 ib (8-bit sign-extended immediate)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_add_mem_base_disp_imm8(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: i8,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let rex_byte = rex(true, false, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x83); // opcode for imm8
    emit_mem_base_disp(buf, 0, base_id, disp); // /0 for ADD
    buf.bytes.push(imm as u8);
}

/// Encode `lock add [base + disp], imm32` — PA-R15-003 (issue #958).
///
/// Instruction: F0 REX.W 81 /0 id (32-bit sign-extended immediate)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_add_mem_base_disp_imm32(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: i32,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let rex_byte = rex(true, false, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x81); // opcode for imm32
    emit_mem_base_disp(buf, 0, base_id, disp); // /0 for ADD
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `lock add [base + disp], src` — PA-R15-003 (issue #958).
///
/// Instruction: F0 REX.W 01 /r (register form, 64-bit)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_add_mem_base_disp_reg64(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x01); // opcode for ADD /r
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock add [base + disp], imm8` — PA-R15-003 (issue #958).
///
/// Instruction: F0 83 /0 ib (8-bit sign-extended immediate, 32-bit)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_add_mem_base_disp_imm8_w32(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: i8,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;

    // Only emit REX.B if extended register is used
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x83); // opcode for imm8
    emit_mem_base_disp(buf, 0, base_id, disp); // /0 for ADD
    buf.bytes.push(imm as u8);
}

/// Encode `lock add [base + disp], imm32` — PA-R15-003 (issue #958).
///
/// Instruction: F0 81 /0 id (32-bit immediate)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_add_mem_base_disp_imm32_w32(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: i32,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;

    // Only emit REX.B if extended register is used
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x81); // opcode for imm32
    emit_mem_base_disp(buf, 0, base_id, disp); // /0 for ADD
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `lock add [base + disp], src` — PA-R15-003 (issue #958).
///
/// Instruction: F0 01 /r (register form, 32-bit, no REX.W)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_add_mem_base_disp_reg32(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;

    // Only emit REX.B or REX.R if extended registers are used
    if (src_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, (src_id >> 3) != 0, false, (base_id >> 3) != 0));
    }
    buf.bytes.push(0x01); // opcode for ADD /r
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock sub [base + disp], imm8` — PA-R15-003 (issue #958).
///
/// Instruction: F0 REX.W 83 /5 ib (8-bit sign-extended immediate)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_sub_mem_base_disp_imm8(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: i8,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let rex_byte = rex(true, false, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x83); // opcode for imm8
    emit_mem_base_disp(buf, 5, base_id, disp); // /5 for SUB
    buf.bytes.push(imm as u8);
}

/// Encode `lock sub [base + disp], imm32` — PA-R15-003 (issue #958).
///
/// Instruction: F0 REX.W 81 /5 id (32-bit sign-extended immediate)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_sub_mem_base_disp_imm32(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: i32,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let rex_byte = rex(true, false, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x81); // opcode for imm32
    emit_mem_base_disp(buf, 5, base_id, disp); // /5 for SUB
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `lock sub [base + disp], src` — PA-R15-003 (issue #958).
///
/// Instruction: F0 REX.W 29 /r (register form, 64-bit)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_sub_mem_base_disp_reg64(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x29); // opcode for SUB /r
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock sub [base + disp], imm8` — PA-R15-003 (issue #958).
///
/// Instruction: F0 83 /5 ib (8-bit sign-extended immediate, 32-bit)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_sub_mem_base_disp_imm8_w32(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: i8,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;

    // Only emit REX.B if extended register is used
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x83); // opcode for imm8
    emit_mem_base_disp(buf, 5, base_id, disp); // /5 for SUB
    buf.bytes.push(imm as u8);
}

/// Encode `lock sub [base + disp], imm32` — PA-R15-003 (issue #958).
///
/// Instruction: F0 81 /5 id (32-bit immediate)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_sub_mem_base_disp_imm32_w32(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: i32,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;

    // Only emit REX.B if extended register is used
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x81); // opcode for imm32
    emit_mem_base_disp(buf, 5, base_id, disp); // /5 for SUB
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `lock sub [base + disp], src` — PA-R15-003 (issue #958).
///
/// Instruction: F0 29 /r (register form, 32-bit, no REX.W)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_sub_mem_base_disp_reg32(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;

    // Only emit REX.B or REX.R if extended registers are used
    if (src_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, (src_id >> 3) != 0, false, (base_id >> 3) != 0));
    }
    buf.bytes.push(0x29); // opcode for SUB /r
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock and [base + disp], src` — PA-R16-006 (issue #972).
///
/// Instruction: F0 REX.W 21 /r (register form, 64-bit)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_and_mem_base_disp_reg64(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x21); // opcode for AND /r
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock or [base + disp], src` — PA-R16-006 (issue #972).
///
/// Instruction: F0 REX.W 09 /r (register form, 64-bit)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_or_mem_base_disp_reg64(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x09); // opcode for OR /r
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock xor [base + disp], src` — PA-R16-006 (issue #972).
///
/// Instruction: F0 REX.W 31 /r (register form, 64-bit)
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_xor_mem_base_disp_reg64(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x31); // opcode for XOR /r
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

