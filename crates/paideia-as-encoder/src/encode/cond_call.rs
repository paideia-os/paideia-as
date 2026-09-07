//! Conditional jump/set (JCC/SETCC), ADD/SUB/ADC-immediate variants, CALL/RET, PUSH/POP, indexed load/store helpers, and higher-level record/enum/match emission helpers.

use super::mov_arith::mov_reg64_imm64;
use super::types::*;

/// Encode conditional jump `jcc rel32`.
///
/// Instruction: 0F 8X cd (where X is the condition code)
/// Total size: 6 bytes
pub fn jcc_rel32(buf: &mut CodeBuffer, cond: Cond, rel: i32) {
    buf.bytes.push(0x0F);
    buf.bytes.push(cond as u8);
    buf.bytes.extend(rel.to_le_bytes());
}

/// Encode conditional jump `jcc rel8` (short form).
///
/// Instruction: 7X cb (where X is the condition code, displacement is relative to end of instruction)
/// Total size: 2 bytes
///
/// Only valid when displacement fits in i8 (-128..=127).
pub fn jcc_rel8(buf: &mut CodeBuffer, cond: Cond, rel: i8) {
    // Convert Cond to rel8 opcode: 0x70 + (cond_byte - 0x80)
    // For example: Eq (0x84) → 0x74; Below (0x82) → 0x72; Overflow (0x80) → 0x70
    let rel8_opcode = 0x70 + (cond as u8 - 0x80);
    buf.bytes.push(rel8_opcode);
    buf.bytes.push(rel as u8);
}

/// Encode conditional set byte `setcc r8` (set on condition).
///
/// Instruction: [REX] 0F 9X /0 (where X is the condition code)
/// Total size: 3–4 bytes (1–2 bytes REX + 2 bytes opcode + ModR/M)
///
/// For r8-r15b (upper registers), emits REX.B; for spl/bpl/sil/dil (low regs needing REX),
/// emits bare 0x40 REX prefix.
pub fn setcc_reg8(buf: &mut CodeBuffer, cc: Cond, reg_id: u8, needs_rex: bool) {
    // REX prefix: needed if register id > 7 (for r8-r15) or if high-byte reg needs REX (spl/bpl/sil/dil)
    if (reg_id >> 3) != 0 {
        // r8b-r15b: REX.B (0x41)
        buf.bytes.push(rex(false, false, false, true));
    } else if needs_rex {
        // spl/bpl/sil/dil: bare REX (0x40)
        buf.bytes.push(0x40);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0x90 | ((cc as u8) & 0x0F));
    buf.bytes.push(0xC0 | (reg_id & 7));
}

/// Encode `add reg64, imm8` (8-bit immediate, sign-extended to 64-bit).
///
/// Instruction: REX.W 83 /0 ib
/// ModR/M: 0xC0 | reg
/// Bytes: `48 83 (0xC0 | reg) imm8`
pub fn add_reg64_imm8(buf: &mut CodeBuffer, dst: Reg64, imm: i8) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x83);
    buf.bytes.push(0xC0 | (reg_id & 7));
    buf.bytes.push(imm as u8);
}

/// Encode `add reg64, imm32` (32-bit immediate, sign-extended to 64-bit).
///
/// Instruction: REX.W 81 /0 id
/// ModR/M: 0xC0 | reg
/// Bytes: `48 81 (0xC0 | reg) imm32_le`
pub fn add_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x81);
    buf.bytes.push(0xC0 | (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `sub reg64, imm8` (8-bit immediate, sign-extended to 64-bit).
/// PA-R13-010 (issue #923).
///
/// Instruction: REX.W 83 /5 ib
/// ModR/M: 0xE8 | reg  (mod=11 reg=101 rm=<reg&7>)
pub fn sub_reg64_imm8(buf: &mut CodeBuffer, dst: Reg64, imm: i8) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x83);
    buf.bytes.push(0xE8 | (reg_id & 7));
    buf.bytes.push(imm as u8);
}

/// Encode `sub reg64, imm32` (32-bit immediate, sign-extended to 64-bit).
/// PA-R13-010 (issue #923).
///
/// Instruction: REX.W 81 /5 id
pub fn sub_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x81);
    buf.bytes.push(0xE8 | (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `adc reg64, imm8` (8-bit immediate, sign-extended to 64-bit).
/// PA-R16-007 (issue #1069).
///
/// Instruction: REX.W 83 /2 ib
/// ModR/M: 0xD0 | reg  (mod=11 reg=010 rm=<reg&7>)
pub fn adc_reg64_imm8(buf: &mut CodeBuffer, dst: Reg64, imm: i8) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x83);
    buf.bytes.push(0xD0 | (reg_id & 7));
    buf.bytes.push(imm as u8);
}

/// Encode `adc reg64, imm32` (32-bit immediate, sign-extended to 64-bit).
/// PA-R16-007 (issue #1069).
///
/// Instruction: REX.W 81 /2 id
/// ModR/M: 0xD0 | reg  (mod=11 reg=010 rm=<reg&7>)
pub fn adc_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x81);
    buf.bytes.push(0xD0 | (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `adc reg32, imm8` (8-bit immediate).
/// PA-R16-007 (issue #1069).
///
/// Instruction: [REX.B] 83 /2 ib
/// ModR/M: 0xD0 | (reg_id & 7)
/// When the high bit is set in the register ID (r8–r15), we emit REX.B.
///
/// Example: `adc eax, 5` → `83 d0 05`
/// Example: `adc r8d, 5` → `41 83 d0 05`
pub fn adc_reg32_imm8(buf: &mut CodeBuffer, reg_id: u8, imm: i8) {
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x83);
    buf.bytes.push(0xD0 | (reg_id & 7));
    buf.bytes.push(imm as u8);
}

/// Encode `adc reg32, imm32` (32-bit immediate).
/// PA-R16-007 (issue #1069).
///
/// Instruction: [REX.B] 81 /2 id
/// ModR/M: 0xD0 | (reg_id & 7)
/// When the high bit is set in the register ID (r8–r15), we emit REX.B.
///
/// Example: `adc eax, 0x1000` → `81 d0 00 10 00 00`
/// Example: `adc r15d, 0x80000001` → `41 81 d7 01 00 00 80`
pub fn adc_reg32_imm32(buf: &mut CodeBuffer, reg_id: u8, imm: i32) {
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x81);
    buf.bytes.push(0xD0 | (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `call rel32` (near call).
///
/// Instruction: E8 cd (displacement is relative to end of instruction)
/// Total size: 5 bytes
pub fn call_rel32(buf: &mut CodeBuffer, rel: i32) {
    buf.bytes.push(0xE8);
    buf.bytes.extend(rel.to_le_bytes());
}

/// Encode `ret` (return from procedure).
///
/// Instruction: C3 (single byte)
pub fn ret(buf: &mut CodeBuffer) {
    buf.bytes.push(0xC3);
}

/// Encode `push reg64`.
///
/// Instruction: 50+rd for registers 0-7; REX.B 41 50+rd for registers 8-15
/// Total size: 1 or 2 bytes
pub fn push_reg64(buf: &mut CodeBuffer, reg: Reg64) {
    let reg_id = reg as u8;
    if reg_id > 7 {
        buf.bytes.push(0x41); // REX.B
        buf.bytes.push(0x50 + (reg_id & 7));
    } else {
        buf.bytes.push(0x50 + reg_id);
    }
}

/// Encode `push imm8`.
///
/// Instruction: 6A ib (2 bytes). The immediate is sign-extended by the CPU.
pub fn push_imm8(buf: &mut CodeBuffer, imm: i8) {
    buf.bytes.push(0x6A);
    buf.bytes.push(imm as u8);
}

/// Encode `push imm32`.
///
/// Instruction: 68 id (5 bytes). The immediate is sign-extended by the CPU.
pub fn push_imm32(buf: &mut CodeBuffer, imm: i32) {
    buf.bytes.push(0x68);
    buf.bytes.extend_from_slice(&imm.to_le_bytes());
}

/// Encode `pop reg64`.
///
/// Instruction: 58+rd for registers 0-7; REX.B 41 58+rd for registers 8-15
/// Total size: 1 or 2 bytes
pub fn pop_reg64(buf: &mut CodeBuffer, reg: Reg64) {
    let reg_id = reg as u8;
    if reg_id > 7 {
        buf.bytes.push(0x41); // REX.B
        buf.bytes.push(0x58 + (reg_id & 7));
    } else {
        buf.bytes.push(0x58 + reg_id);
    }
}

/// Encode `mov <dest>, [<base> + <index> * <scale>]` for width = 1, 2, 4, 8.
///
/// The width parameter specifies the AMD64 effective operand size:
/// - 1 byte: mov al, byte ptr [base + index]
/// - 2 bytes: mov ax, word ptr [base + index * 2]
/// - 4 bytes: mov eax, dword ptr [base + index * 4]
/// - 8 bytes: mov rax, qword ptr [base + index * 8]
///
/// Phase-3-m1-007: emits to 64-bit-dest form for all widths using the canonically-sized
/// destination register (RAX for 8, EAX for 4, AX for 2, AL for 1). Narrower loads
/// are zero-extended (implicit for 32-bit dest in x86-64). The signedness parameter
/// is accepted for API compatibility but phase-1 uses zero-extension only.
///
/// **Borrowed references at codegen:** At the x86_64 byte level, `&T`, `&mut T`, and `*T`
/// are indistinguishable: all encode as pointers (8-byte machine addresses). Type-level
/// borrow safety is enforced by the m6 borrow checker. This encoder treats all three forms
/// identically per m4-006.
///
/// Instruction pattern:
/// - [PREFIX if needed] OPCODE [REX] [SIB]
/// - Opcode: 0x8A (MOV r8, r/m8), 0x8B (MOV r16/32/64, r/m16/32/64)
/// - ModR/M: 0x04 (mod=00, reg=dest_id, rm=100 which triggers SIB)
/// - SIB: (scale<<6) | (index_id<<3) | base_id
///
/// Scale encoding:
/// - width 1: scale=00 (×1)
/// - width 2: scale=01 (×2)
/// - width 4: scale=10 (×4)
/// - width 8: scale=11 (×8)
pub fn emit_indexed_load(
    buf: &mut CodeBuffer,
    dest: Reg64,
    base: Reg64,
    index: Reg64,
    width: u32,
    _signed: bool,
) {
    let dest_id = dest as u8;
    let base_id = base as u8;
    let index_id = index as u8;

    // PA-R13-002: Compute REX byte from all three register positions:
    // REX.R = (dest_id >> 3) for r8-r15 destination
    // REX.X = (index_id >> 3) for r8-r15 index
    // REX.B = (base_id >> 3) for r8-r15 base
    let rex_r_bit: u8 = if (dest_id >> 3) != 0 { 0x04 } else { 0 };
    let rex_x_bit: u8 = if (index_id >> 3) != 0 { 0x02 } else { 0 };
    let rex_b_bit: u8 = if (base_id >> 3) != 0 { 0x01 } else { 0 };

    match width {
        1 => {
            // mov r8, [base + index]
            // Opcode 8A
            // W8 with high registers must always have REX prefix to select SPL/BPL/SIL/DIL over AH/CH/DH/BH
            let rex_byte = 0x40 | rex_r_bit | rex_x_bit | rex_b_bit;
            if rex_byte != 0x40 {
                buf.bytes.push(rex_byte);
            }
            buf.bytes.push(0x8A);
            emit_mem_sib_disp(buf, dest_id & 7, base_id, index_id, 0, 0);
        }
        2 => {
            // mov r16, [base + index * 2]
            // Operand-size prefix 0x66
            buf.bytes.push(0x66);
            let rex_byte = 0x40 | rex_r_bit | rex_x_bit | rex_b_bit;
            if rex_byte != 0x40 {
                buf.bytes.push(rex_byte);
            }
            buf.bytes.push(0x8B);
            emit_mem_sib_disp(buf, dest_id & 7, base_id, index_id, 1, 0);
        }
        4 => {
            // mov r32, [base + index * 4]
            let rex_byte = 0x40 | rex_r_bit | rex_x_bit | rex_b_bit;
            if rex_byte != 0x40 {
                buf.bytes.push(rex_byte);
            }
            buf.bytes.push(0x8B);
            emit_mem_sib_disp(buf, dest_id & 7, base_id, index_id, 2, 0);
        }
        8 => {
            // mov r64, [base + index * 8]
            // REX.W=1, add REX.R/X/B as needed
            buf.bytes.push(0x48 | rex_r_bit | rex_x_bit | rex_b_bit);
            buf.bytes.push(0x8B);
            emit_mem_sib_disp(buf, dest_id & 7, base_id, index_id, 3, 0);
        }
        _ => panic!(
            "invalid width {} for emit_indexed_load; must be 1, 2, 4, or 8",
            width
        ),
    }
}

/// Encode `mov [<base> + <index> * <scale>], <src>` for width = 1, 2, 4, 8.
///
/// The width parameter specifies the AMD64 effective operand size:
/// - 1 byte: mov byte ptr [base + index], src_byte
/// - 2 bytes: mov word ptr [base + index * 2], src_word
/// - 4 bytes: mov dword ptr [base + index * 4], src_dword
/// - 8 bytes: mov qword ptr [base + index * 8], src
///
/// Instruction pattern similar to emit_indexed_load but with opcode 0x88/0x89
/// (store, not load) and using the canonically-sized source register.
///
/// Scale encoding (same as emit_indexed_load):
/// - width 1: scale=00 (×1)
/// - width 2: scale=01 (×2)
/// - width 4: scale=10 (×4)
/// - width 8: scale=11 (×8)
pub fn emit_indexed_store(
    buf: &mut CodeBuffer,
    base: Reg64,
    index: Reg64,
    src: Reg64,
    width: u32,
) {
    let src_id = src as u8;
    let base_id = base as u8;
    let index_id = index as u8;

    // PA-R13-002: Compute REX byte from all three register positions:
    // REX.R = (src_id >> 3) for r8-r15 source
    // REX.X = (index_id >> 3) for r8-r15 index
    // REX.B = (base_id >> 3) for r8-r15 base
    let rex_r_bit: u8 = if (src_id >> 3) != 0 { 0x04 } else { 0 };
    let rex_x_bit: u8 = if (index_id >> 3) != 0 { 0x02 } else { 0 };
    let rex_b_bit: u8 = if (base_id >> 3) != 0 { 0x01 } else { 0 };

    match width {
        1 => {
            // mov [base + index], r8
            // Opcode 88
            // W8 with high registers must always have REX prefix to select SPL/BPL/SIL/DIL over AH/CH/DH/BH
            let rex_byte = 0x40 | rex_r_bit | rex_x_bit | rex_b_bit;
            if rex_byte != 0x40 {
                buf.bytes.push(rex_byte);
            }
            buf.bytes.push(0x88);
            emit_mem_sib_disp(buf, src_id & 7, base_id, index_id, 0, 0);
        }
        2 => {
            // mov [base + index * 2], r16
            // Operand-size prefix 0x66
            buf.bytes.push(0x66);
            let rex_byte = 0x40 | rex_r_bit | rex_x_bit | rex_b_bit;
            if rex_byte != 0x40 {
                buf.bytes.push(rex_byte);
            }
            buf.bytes.push(0x89);
            emit_mem_sib_disp(buf, src_id & 7, base_id, index_id, 1, 0);
        }
        4 => {
            // mov [base + index * 4], r32
            let rex_byte = 0x40 | rex_r_bit | rex_x_bit | rex_b_bit;
            if rex_byte != 0x40 {
                buf.bytes.push(rex_byte);
            }
            buf.bytes.push(0x89);
            emit_mem_sib_disp(buf, src_id & 7, base_id, index_id, 2, 0);
        }
        8 => {
            // mov [base + index * 8], r64
            // REX.W=1, add REX.R/X/B as needed
            buf.bytes.push(0x48 | rex_r_bit | rex_x_bit | rex_b_bit);
            buf.bytes.push(0x89);
            emit_mem_sib_disp(buf, src_id & 7, base_id, index_id, 3, 0);
        }
        _ => panic!(
            "invalid width {} for emit_indexed_store; must be 1, 2, 4, or 8",
            width
        ),
    }
}

/// Record construction: emit a sequence of `mov [base + offset], src` stores.
///
/// Stores each field value from the provided register to the record memory
/// at the specified offset. This is the core operation for struct initialization.
///
/// # Arguments
/// - `buf`: code buffer to append instructions to
/// - `base`: destination pointer (typically a struct address in a register)
/// - `field_stores`: slice of (offset, src_register, width) tuples for each field
///
/// Each store uses the smallest form possible:
/// - If offset fits in i8 and is not 0: mod=01, disp8 (2 bytes for disp)
/// - If offset is 0: use disp8=0 (mod=01 with disp8=0)
/// - Otherwise: mod=10, disp32 (4 bytes for disp)
///
/// Example: `mov [rdi + 8], rsi` (store qword):
/// - REX.W = 0x48, opcode = 0x89
/// - ModR/M = 0x77 (mod=01 [disp8], reg=110 [RSI], rm=111 [RDI])
/// - disp8 = 0x08
/// - Total: `48 89 77 08`
pub fn emit_record_cons(buf: &mut CodeBuffer, base: Reg64, field_stores: &[(i32, Reg64, u32)]) {
    let base_id = base as u8;

    for (offset, src, width) in field_stores {
        let src_id = *src as u8;

        match width {
            8 => {
                // mov [base + offset], src (qword)
                // REX.W=1
                let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);
                buf.bytes.push(rex_byte);
                // Opcode: 0x89 (MOV r/m64, r64)
                buf.bytes.push(0x89);

                if (-128..=127).contains(offset) {
                    // Use mod=01, disp8
                    buf.bytes.push(0x40 | ((src_id & 7) << 3) | (base_id & 7));
                    buf.bytes.push(*offset as u8);
                } else {
                    // Use mod=10, disp32
                    buf.bytes.push(0x80 | ((src_id & 7) << 3) | (base_id & 7));
                    buf.bytes.extend(offset.to_le_bytes());
                }
            }
            _ => panic!("emit_record_cons: unsupported width {}", width),
        }
    }
}

/// Field access: emit `mov dest, [base + offset]` for a struct field.
///
/// Loads a field value from memory at base + offset into the destination register.
/// This is the core operation for field extraction from a struct.
///
/// # Arguments
/// - `buf`: code buffer to append instructions to
/// - `dest`: destination register
/// - `base`: source pointer (struct address)
/// - `offset`: field offset from base
/// - `width`: field width in bytes (8 for qword in phase-4 minimum)
///
/// Uses the smallest addressing form possible (same logic as `emit_record_cons`).
///
/// Example: `mov rax, [rdi + 8]` (load qword from offset 8):
/// - REX.W = 0x48, opcode = 0x8B
/// - ModR/M = 0x47 (mod=01 [disp8], reg=000 [RAX], rm=111 [RDI])
/// - disp8 = 0x08
/// - Total: `48 8b 47 08`
pub fn emit_field_access(buf: &mut CodeBuffer, dest: Reg64, base: Reg64, offset: i32, width: u32) {
    let dest_id = dest as u8;
    let base_id = base as u8;

    match width {
        8 => {
            // mov dest, [base + offset] (qword)
            // REX.W=1
            let rex_byte = rex(true, (dest_id >> 3) != 0, false, (base_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            // Opcode: 0x8B (MOV r64, r/m64)
            buf.bytes.push(0x8B);

            if (-128..=127).contains(&offset) {
                // Use mod=01, disp8
                buf.bytes.push(0x40 | ((dest_id & 7) << 3) | (base_id & 7));
                buf.bytes.push(offset as u8);
            } else {
                // Use mod=10, disp32
                buf.bytes.push(0x80 | ((dest_id & 7) << 3) | (base_id & 7));
                buf.bytes.extend(offset.to_le_bytes());
            }
        }
        _ => panic!("emit_field_access: unsupported width {}", width),
    }
}

/// Enum construction: emit discriminant store + payload stores.
///
/// Phase-4 minimum: 8-byte discriminant at offset 0; payload at offset 8.
/// First stores the discriminant (variant index), then each payload field.
///
/// # Arguments
/// - `buf`: code buffer to append instructions to
/// - `base`: destination pointer (enum address)
/// - `discriminant`: variant index (u64)
/// - `payload_stores`: slice of (offset, src_register, width) tuples for payload fields
///
/// First emits: `mov [base + 0], rax` with discriminant value loaded into rax.
/// Then emits each payload store using emit_record_cons logic.
pub fn emit_enum_cons(
    buf: &mut CodeBuffer,
    base: Reg64,
    discriminant: u64,
    payload_stores: &[(i32, Reg64, u32)],
) {
    // Store discriminant at offset 0 using RAX as temp
    let base_id = base as u8;
    mov_reg64_imm64(buf, Reg64::Rax, discriminant);

    // mov [base + 0], rax
    let rex_byte = rex(true, false, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x89);
    // ModR/M: mod=00, reg=0 (RAX), rm=7 (base register)
    buf.bytes.push(base_id & 7);

    // Store payload fields
    emit_record_cons(buf, base, payload_stores);
}

/// Enum discriminant extraction: `mov dest, [base + 0]` (8-byte load).
///
/// Loads the discriminant (variant index) from an enum value.
/// Phase-4: always loads 8 bytes from offset 0.
///
/// # Arguments
/// - `buf`: code buffer to append instructions to
/// - `dest`: destination register for the discriminant
/// - `base`: source enum pointer
///
/// Emits: `mov dest, [base + 0]`
pub fn emit_enum_discriminant(buf: &mut CodeBuffer, dest: Reg64, base: Reg64) {
    emit_field_access(buf, dest, base, 0, 8);
}

/// Match-on-enum: emit `cmp dest, imm; jcc target`. Returns the rel32-patch
/// offset that the linker will fix up.
///
/// # Arguments
/// - `buf`: code buffer to append instructions to
/// - `dest`: register holding discriminant
/// - `expected_variant`: discriminant value to compare against
/// - `cond`: condition code for the branch (typically Neq for "skip this arm")
///
/// Returns: the buffer offset of the rel32 displacement bytes (for linker patching).
///
/// Emits:
/// 1. `cmp dest, expected_variant` (8-byte comparison with sign-extended imm32)
/// 2. `jcc rel32` (conditional near jump)
///
/// Phase-4 minimum: linear cmp+jcc chain (no jump table optimization).
pub fn emit_match_arm_branch(
    buf: &mut CodeBuffer,
    dest: Reg64,
    expected_variant: u64,
    cond: Cond,
) -> usize {
    let dest_id = dest as u8;

    // Emit cmp dest, expected_variant
    // Use cmp with imm32 (0x81) for phase-4 simplicity
    let rex_byte = rex(true, false, false, (dest_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x81);
    // ModR/M: mod=11, reg=7 (cmp opcode /7), rm=dest
    buf.bytes.push(0xF8 | (dest_id & 7));
    // Immediate: sign-extended from 32-bit
    buf.bytes.extend((expected_variant as i32).to_le_bytes());

    // Emit jcc rel32; return offset for linker patching
    // After pushing 0x0F and cond, rel32 will start at buf.len() + 2
    let patch_offset = buf.bytes.len() + 2; // +2 for two-byte opcode
    buf.bytes.push(0x0F);
    buf.bytes.push(cond as u8);
    buf.bytes.extend([0, 0, 0, 0]); // Placeholder for rel32

    patch_offset
}

