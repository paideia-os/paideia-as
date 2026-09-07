//! MOV memory-load and memory-store variants across all widths, plus MOVNTI (temporal-hint stores).

use super::types::*;
use paideia_as_ir::instruction::IntWidth;

/// Encode `cmp [mem], reg64` (memory form with base + disp).
///
/// Instruction: REX.W 39 /r
/// ModR/M encodes the base register as r/m, and the src register in the reg field.
///
/// Uses the smallest form possible:
/// - If disp fits in i8 and is not 0: mod=01, disp8 (2 bytes for disp)
/// - If disp is 0: use mod=01 with disp8=0 (RBP with mod=00 is special)
/// - Otherwise: mod=10, disp32 (4 bytes for disp)
///
/// Example: `cmp [rdi + 24], rcx` → `48 39 4F 18`
/// Encode `mov dst, [base + disp]` — Phase 8 m5-002: general memory operand.
///
/// Instruction: REX.W 8B /r
/// Operand-size: 64-bit (register is r64, memory is r/m64)
/// ModR/M: depends on displacement encoding (no disp, disp8, or disp32)
///
/// Examples:
/// - `mov rax, [rdi]`: `48 8B 07`
/// - `mov rax, [rdi + 8]`: `48 8B 47 08`
/// - `mov rax, [rdi + 256]`: `48 8B 87 00 01 00 00`
pub fn mov_reg64_mem_reg64_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x8B); // mov r64, r/m64
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `mov r8, [base + disp]` — Phase 13 m6-001: 8-bit load.
///
/// Instruction: [REX?] 8A /r
/// Operand-size: 8-bit (register is r8, memory is r/m8)
/// REX.B: required only for r8–r15 destination
/// No REX.W (8-bit operand size)
///
/// Examples:
/// - `mov al, [rdi]`: `8A 07`
/// - `mov r8b, [rdi]`: `41 8A 07`
/// - `mov bl, [rdi + 8]`: `8A 47 08`
pub fn mov_reg8_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(false, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    if (dst_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x8A); // mov r8, r/m8
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `mov r16, [base + disp]` — Phase 13 m6-001: 16-bit load.
///
/// Instruction: 66 [REX?] 8B /r
/// Operand-size: 16-bit (register is r16, memory is r/m16)
/// 66: operand-size override prefix
/// REX.B: required only for r8w–r15w destination
/// No REX.W (16-bit operand size)
///
/// Examples:
/// - `mov ax, [rdi]`: `66 8B 07`
/// - `mov r8w, [rdi]`: `66 41 8B 07`
/// - `mov bx, [rdi + 8]`: `66 8B 47 08`
pub fn mov_reg16_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    buf.bytes.push(0x66); // operand-size override
    let rex_byte = rex(false, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    if (dst_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x8B); // mov r16, r/m16
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `mov r32, [base + disp]` — Phase 13 m6-001: 32-bit load.
///
/// Instruction: [REX?] 8B /r
/// Operand-size: 32-bit (register is r32, memory is r/m32)
/// REX.B: required only for r8d–r15d destination
/// No REX.W (32-bit operand size, implicit zero-extend to r64)
///
/// Examples:
/// - `mov eax, [rdi]`: `8B 07`
/// - `mov r8d, [rdi]`: `41 8B 07`
/// - `mov ebx, [rdi + 8]`: `8B 47 08`
pub fn mov_reg32_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(false, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    if (dst_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x8B); // mov r32, r/m32
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `movsx r64, [base + disp]` — Phase 13 m6-001: sign-extending load from memory.
///
/// Instruction: REX.W opcode /r [disp]
/// Operand-size: determined by src_width parameter
/// - 1 byte (r/m8 → r64):  `REX.W 0F BE /r` (movsx r64, byte [mem])
/// - 2 bytes (r/m16 → r64): `REX.W 0F BF /r` (movsx r64, word [mem])
/// - 4 bytes (r/m32 → r64): `REX.W 63 /r` (movsxd r64, dword [mem])
///
/// REX.W: always set (64-bit destination)
/// REX.R: set if dst in r8–r15
/// REX.B: set if base in r8–r15
/// ModR/M: depends on displacement encoding (no disp, disp8, or disp32)
///
/// Examples:
/// - `movsx rax, byte [rdi]`: `48 0F BE 07`
/// - `movsx rax, word [rdi + 8]`: `48 0F BF 47 08`
/// - `movsx rax, dword [rdi + 8]`: `48 63 47 08`
pub fn movsx_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32, src_width: u8) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    match src_width {
        1 => {
            buf.bytes.push(0x0F);
            buf.bytes.push(0xBE); // movsx r64, r/m8
        }
        2 => {
            buf.bytes.push(0x0F);
            buf.bytes.push(0xBF); // movsx r64, r/m16
        }
        4 => {
            buf.bytes.push(0x63); // movsxd r64, r/m32
        }
        _ => {
            // Invalid source width; caller should have validated
            return;
        }
    }
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}


/// Encode `mov r, [base + index*scale + disp]` with width — Phase 13 m6-001: SIB load.
///
/// Instruction: [0x66] [REX?] opcode /r SIB [disp]
/// Operand-size: determined by width parameter
/// - W8:  opcode = 0x8A (mov r8, r/m8)
/// - W16: 0x66 prefix + opcode = 0x8B (mov r16, r/m16)
/// - W32: opcode = 0x8B (mov r32, r/m32)
/// - W64: REX.W + opcode = 0x8B (mov r64, r/m64)
/// REX: W for W64, R for dst in r8–r15, X for index in r8–r15, B for base in r8–r15
/// SIB: scale (2 bits) | index (3 bits) | base (3 bits)
/// ModR/M: depends on displacement encoding (no disp, disp8, or disp32)
pub fn mov_reg_mem_sib_disp_sized(
    buf: &mut CodeBuffer,
    width: IntWidth,
    dst: Reg64,
    base: Reg64,
    index: Reg64,
    scale_bits: u8,
    disp: i32,
) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let index_id = index as u8;

    match width {
        IntWidth::W8 => {
            let rex_byte = rex(false, (dst_id >> 3) != 0, (index_id >> 3) != 0, (base_id >> 3) != 0);
            if (dst_id >> 3) != 0 || (base_id >> 3) != 0 || (index_id >> 3) != 0 {
                buf.bytes.push(rex_byte);
            }
            buf.bytes.push(0x8A); // mov r8, r/m8
        }
        IntWidth::W16 => {
            buf.bytes.push(0x66); // operand-size override
            let rex_byte = rex(false, (dst_id >> 3) != 0, (index_id >> 3) != 0, (base_id >> 3) != 0);
            if (dst_id >> 3) != 0 || (base_id >> 3) != 0 || (index_id >> 3) != 0 {
                buf.bytes.push(rex_byte);
            }
            buf.bytes.push(0x8B); // mov r16, r/m16
        }
        IntWidth::W32 => {
            let rex_byte = rex(false, (dst_id >> 3) != 0, (index_id >> 3) != 0, (base_id >> 3) != 0);
            if (dst_id >> 3) != 0 || (base_id >> 3) != 0 || (index_id >> 3) != 0 {
                buf.bytes.push(rex_byte);
            }
            buf.bytes.push(0x8B); // mov r32, r/m32
        }
        IntWidth::W64 => {
            let rex_byte = rex(true, (dst_id >> 3) != 0, (index_id >> 3) != 0, (base_id >> 3) != 0);
            buf.bytes.push(rex_byte);
            buf.bytes.push(0x8B); // mov r64, r/m64
        }
    }

    emit_mem_sib_disp(buf, dst_id & 7, base_id, index_id, scale_bits, disp);
}

/// Encode `mov [base + disp], src` — Phase 8 m5-002: general memory operand.
///
/// Instruction: REX.W 89 /r
/// Operand-size: 64-bit (register is r64, memory is r/m64)
/// ModR/M: depends on displacement encoding (no disp, disp8, or disp32)
///
/// Examples:
/// - `mov [rdi], rax`: `48 89 07`
/// - `mov [rdi + 8], rax`: `48 89 47 08`
/// - `mov [rdi + 256], rax`: `48 89 87 00 01 00 00`
pub fn mov_mem_reg64_disp_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64) {
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x89); // mov r/m64, r64
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `mov [base + disp], r8` — pa-r17-006 (#984): 8-bit register-source store.
///
/// Instruction: [REX?] 88 /r
/// Operand-size: 8-bit (register is r8, memory is r/m8)
/// REX: MANDATORY when src ∈ {rsp, rbp, rsi, rdi} (ids 4-7) to select SPL/BPL/SIL/DIL
///      Otherwise decoded as ah/bh/ch/dh — WRONG instruction
/// REX.R: set if src in r8–r15 (high bit of src register)
/// REX.B: set if base in r8–r15 (high bit of base register)
///
/// Examples:
/// - `mov [rdi], al`: `88 07`
/// - `mov [rdi + 8], r8b`: `44 88 47 08`
/// - `mov [rdi], sil`: `40 88 37` (REX.0 required for SIL)
pub fn mov_mem_base_disp_reg8(buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64) {
    let base_id = base as u8;
    let src_id = src as u8;
    let src_low = src_id & 7;

    // Check if src is one of {rsp, rbp, rsi, rdi} (low 3 bits: 4-7)
    // These require REX to select byte registers SPL/BPL/SIL/DIL
    let requires_rex_for_byte_reg = (4..=7).contains(&src_low);

    let rex_byte = rex(false, (src_id >> 3) != 0, false, (base_id >> 3) != 0);

    // Emit REX if needed (high bits set OR low bits in 4-7 range)
    if (src_id >> 3) != 0 || (base_id >> 3) != 0 || requires_rex_for_byte_reg {
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x88); // mov r/m8, r8
    emit_mem_base_disp(buf, src_low, base_id, disp);
}

/// Encode `mov [base + disp], r16` — pa-r17-006 (#984): 16-bit register-source store.
///
/// Instruction: 66 [REX?] 89 /r
/// Operand-size: 16-bit (register is r16, memory is r/m16)
/// 66: operand-size override prefix
/// REX.R: set if src in r8–r15 (high bit of src register)
/// REX.B: set if base in r8–r15 (high bit of base register)
///
/// Examples:
/// - `mov [rdi], ax`: `66 89 07`
/// - `mov [rdi + 8], r8w`: `66 44 89 47 08`
/// - `mov [r13], ax`: `66 49 89 45 00` (R13 requires disp8=0 escape)
pub fn mov_mem_base_disp_reg16(buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64) {
    let base_id = base as u8;
    let src_id = src as u8;
    buf.bytes.push(0x66); // operand-size override
    let rex_byte = rex(false, (src_id >> 3) != 0, false, (base_id >> 3) != 0);

    if (src_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x89); // mov r/m16, r16
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `mov [base + disp], r32` — pa-r17-006 (#984): 32-bit register-source store.
///
/// Instruction: [REX?] 89 /r
/// Operand-size: 32-bit (register is r32, memory is r/m32)
/// REX: Only for high bits (no REX.W for 32-bit operand size)
/// REX.R: set if src in r8–r15 (high bit of src register)
/// REX.B: set if base in r8–r15 (high bit of base register)
///
/// Examples:
/// - `mov [rdi], eax`: `89 07`
/// - `mov [rdi + 8], r8d`: `44 89 47 08`
/// - `mov [r13], eax`: `49 89 45 00` (R13 requires disp8=0 escape)
pub fn mov_mem_base_disp_reg32(buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64) {
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(false, (src_id >> 3) != 0, false, (base_id >> 3) != 0);

    if (src_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x89); // mov r/m32, r32
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `mov [base + index*scale + disp], r8` — issue #1269: SIB-indexed narrow (8-bit) store.
///
/// Instruction: `[REX?] 88 /r` with SIB byte
/// Operand-size: 8-bit (register is r8, memory is r/m8)
/// REX: MANDATORY when src ∈ {rsp, rbp, rsi, rdi} (ids 4-7) to select SPL/BPL/SIL/DIL,
///      otherwise decoded as ah/bh/ch/dh.
/// REX.R: set if src ∈ r8-r15; REX.X: set if index ∈ r8-r15; REX.B: set if base ∈ r8-r15.
///
/// Example: `mov [rax + rcx*4], dil` → `40 88 3C 88`
pub fn mov_mem_sib_disp_reg8(
    buf: &mut CodeBuffer,
    base: Reg64,
    index: Reg64,
    scale_bits: u8,
    disp: i32,
    src: Reg64,
) {
    let base_id = base as u8;
    let index_id = index as u8;
    let src_id = src as u8;
    let src_low = src_id & 7;
    let requires_rex_for_byte_reg = (4..=7).contains(&src_low);
    let rex_byte = rex(
        false,
        (src_id >> 3) != 0,
        (index_id >> 3) != 0,
        (base_id >> 3) != 0,
    );
    if (src_id >> 3) != 0
        || (index_id >> 3) != 0
        || (base_id >> 3) != 0
        || requires_rex_for_byte_reg
    {
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x88); // mov r/m8, r8
    emit_mem_sib_disp(buf, src_low, base_id, index_id, scale_bits, disp);
}

/// Encode `mov [base + index*scale + disp], r16` — issue #1269: SIB-indexed narrow (16-bit) store.
///
/// Instruction: `66 [REX?] 89 /r` with SIB byte
/// Operand-size: 16-bit via 0x66 override.
/// REX.R: set if src ∈ r8-r15; REX.X: set if index ∈ r8-r15; REX.B: set if base ∈ r8-r15.
///
/// Example: `mov [rax + rcx*4], di` → `66 89 3C 88`
pub fn mov_mem_sib_disp_reg16(
    buf: &mut CodeBuffer,
    base: Reg64,
    index: Reg64,
    scale_bits: u8,
    disp: i32,
    src: Reg64,
) {
    let base_id = base as u8;
    let index_id = index as u8;
    let src_id = src as u8;
    buf.bytes.push(0x66); // operand-size override
    let rex_byte = rex(
        false,
        (src_id >> 3) != 0,
        (index_id >> 3) != 0,
        (base_id >> 3) != 0,
    );
    if (src_id >> 3) != 0 || (index_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x89); // mov r/m16, r16
    emit_mem_sib_disp(buf, src_id & 7, base_id, index_id, scale_bits, disp);
}

/// Encode `mov [base + index*scale + disp], r32` — issue #1269: SIB-indexed narrow (32-bit) store.
///
/// Instruction: `[REX?] 89 /r` with SIB byte
/// Operand-size: 32-bit (no REX.W).
/// REX.R: set if src ∈ r8-r15; REX.X: set if index ∈ r8-r15; REX.B: set if base ∈ r8-r15.
///
/// Examples:
/// - `mov [rax + rcx*4], edi` → `89 3C 88`
/// - `mov [rax + rcx*4], r8d`  → `44 89 04 88`
pub fn mov_mem_sib_disp_reg32(
    buf: &mut CodeBuffer,
    base: Reg64,
    index: Reg64,
    scale_bits: u8,
    disp: i32,
    src: Reg64,
) {
    let base_id = base as u8;
    let index_id = index as u8;
    let src_id = src as u8;
    let rex_byte = rex(
        false,
        (src_id >> 3) != 0,
        (index_id >> 3) != 0,
        (base_id >> 3) != 0,
    );
    if (src_id >> 3) != 0 || (index_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x89); // mov r/m32, r32
    emit_mem_sib_disp(buf, src_id & 7, base_id, index_id, scale_bits, disp);
}

/// Encode `mov r64, [base + index*scale + disp]` — Phase 9 m1-003: SIB addressing with displacement.
///
/// Instruction: REX.W 8B /r
/// Operand-size: 64-bit (register is r64, memory is r/m64)
/// SIB: scale (2 bits) | index (3 bits) | base (3 bits)
/// ModR/M: depends on displacement encoding (no disp, disp8, or disp32)
///
/// Examples:
/// - `mov r10, [r8 + r9*2]`: `4A 8B 04 C8`
/// - `mov rax, [rbx + rcx*4 + 16]`: `48 8B 44 8B 10`
/// - `mov r12, [rsi + rdx*8 + 256]`: `4C 8B 84 D6 00 01 00 00`
pub fn mov_reg64_mem_sib_disp(
    buf: &mut CodeBuffer,
    dst: Reg64,
    base: Reg64,
    index: Reg64,
    scale_bits: u8,
    disp: i32,
) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let index_id = index as u8;
    let rex_byte = rex(
        true,
        (dst_id >> 3) != 0,
        (index_id >> 3) != 0,
        (base_id >> 3) != 0,
    );

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x8B); // mov r64, r/m64

    emit_mem_sib_disp(buf, dst_id & 7, base_id, index_id, scale_bits, disp);
}

/// Encode `mov [base + index*scale + disp], r64` — Phase 9 m1-003: SIB addressing with displacement.
///
/// Instruction: REX.W 89 /r
/// Operand-size: 64-bit (register is r64, memory is r/m64)
/// SIB: scale (2 bits) | index (3 bits) | base (3 bits)
/// ModR/M: depends on displacement encoding (no disp, disp8, or disp32)
///
/// Examples:
/// - `mov [r8 + r9*2], r10`: `4A 89 04 C8`
/// - `mov [rbx + rcx*4 + 16], rax`: `48 89 44 8B 10`
/// - `mov [rsi + rdx*8 + 256], r12`: `4C 89 84 D6 00 01 00 00`
pub fn mov_mem_sib_disp_reg64(
    buf: &mut CodeBuffer,
    base: Reg64,
    index: Reg64,
    scale_bits: u8,
    disp: i32,
    src: Reg64,
) {
    let base_id = base as u8;
    let index_id = index as u8;
    let src_id = src as u8;
    let rex_byte = rex(
        true,
        (src_id >> 3) != 0,
        (index_id >> 3) != 0,
        (base_id >> 3) != 0,
    );

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x89); // mov r/m64, r64

    emit_mem_sib_disp(buf, src_id & 7, base_id, index_id, scale_bits, disp);
}

/// Encode `movnti [base + disp], r32` — PA-R14-003 (issue #946): non-temporal store.
///
/// Instruction: `0F C3 /r` (no REX.W for 32-bit)
/// Bypasses cache hierarchy. REX.R for src ∈ r8-r15, REX.B for base ∈ r8-r15.
///
/// Example: `movnti [rdi], eax` → `0F C3 07`
/// Example: `movnti [rdi + 8], r10d` → `44 0F C3 57 08`
pub fn movnti_mem_base_disp_reg32(buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64) {
    let base_id = base as u8;
    let src_id = src as u8;

    // Only emit REX.B or REX.R if extended registers are used
    if (src_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, (src_id >> 3) != 0, false, (base_id >> 3) != 0));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xC3); // movnti /r

    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `movnti [base + disp], r64` — PA-R14-003 (issue #946): non-temporal store.
///
/// Instruction: `REX.W 0F C3 /r`
/// Bypasses cache hierarchy. REX.R for src ∈ r8-r15, REX.B for base ∈ r8-r15.
///
/// Example: `movnti [rdi], rax` → `48 0F C3 07`
/// Example: `movnti [r12 + rsi*4], rax` → `49 0F C3 04 B4`
pub fn movnti_mem_base_disp_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64) {
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xC3); // movnti /r

    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `movnti [base + index*scale + disp], r32` — PA-R14-003 (issue #946): SIB form, 32-bit.
///
/// Instruction: `0F C3 /r` (no REX.W for 32-bit)
/// SIB: scale (2 bits) | index (3 bits) | base (3 bits)
///
/// Example: `movnti [rbx + rcx*4], eax` → `0F C3 04 8B`
pub fn movnti_mem_sib_disp_reg32(
    buf: &mut CodeBuffer,
    base: Reg64,
    index: Reg64,
    scale_bits: u8,
    disp: i32,
    src: Reg64,
) {
    let base_id = base as u8;
    let index_id = index as u8;
    let src_id = src as u8;

    // Only emit REX if extended registers are used (REX.R, REX.X, or REX.B)
    if (src_id >> 3) != 0 || (index_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex(
            false,
            (src_id >> 3) != 0,
            (index_id >> 3) != 0,
            (base_id >> 3) != 0,
        ));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xC3); // movnti /r

    emit_mem_sib_disp(buf, src_id & 7, base_id, index_id, scale_bits, disp);
}

/// Encode `movnti [base + index*scale + disp], r64` — PA-R14-003 (issue #946): SIB form, 64-bit.
///
/// Instruction: `REX.W 0F C3 /r`
/// SIB: scale (2 bits) | index (3 bits) | base (3 bits)
///
/// Example: `movnti [rsi + rdx*2], r10` → `4A 0F C3 14 D6`
pub fn movnti_mem_sib_disp_reg64(
    buf: &mut CodeBuffer,
    base: Reg64,
    index: Reg64,
    scale_bits: u8,
    disp: i32,
    src: Reg64,
) {
    let base_id = base as u8;
    let index_id = index as u8;
    let src_id = src as u8;
    let rex_byte = rex(
        true,
        (src_id >> 3) != 0,
        (index_id >> 3) != 0,
        (base_id >> 3) != 0,
    );

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xC3); // movnti /r

    emit_mem_sib_disp(buf, src_id & 7, base_id, index_id, scale_bits, disp);
}

