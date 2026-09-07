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

