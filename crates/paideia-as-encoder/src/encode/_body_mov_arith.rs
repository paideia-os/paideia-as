/// Encode `mov reg64, imm32` (sign-extended to 64-bit).
///
/// Instruction: REX.W C7 /0 id
/// Bytes: `48+REX.B C7 (0xC0 | (reg & 7)) imm32_le`
///
/// Example: `mov rax, 1` → `48 c7 c0 01 00 00 00`
/// Example: `mov r8, 1` → `49 c7 c0 01 00 00 00` (REX.B set)
pub fn mov_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xC7);
    buf.bytes.push(0xC0 | (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `mov reg64, imm64` (full 64-bit immediate).
///
/// Instruction: REX.W B8+rd io (where io is 8-byte imm64)
/// Bytes: `48+REX.B B8+(reg&7) imm64_le`
pub fn mov_reg64_imm64(buf: &mut CodeBuffer, dst: Reg64, imm: u64) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xB8 + (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `mov reg32, imm32` (32-bit move, implicit zero-extend to 64-bit).
///
/// Phase 7 m4-003 (PA7C-m4-003): width-threaded form for typed integer-literal
/// `let` bindings. Writing a 32-bit register zero-extends into the full 64-bit
/// register, so no REX.W is required.
///
/// Instruction: B8+rd id (no REX.W; REX.B only for r8d–r15d)
/// Bytes: `[41] B8+(reg&7) imm32_le`
///
/// Example: `mov eax, 42` → `b8 2a 00 00 00` (5 bytes)
pub fn mov_reg32_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: u32) {
    let reg_id = dst as u8;
    // REX.B only needed to reach r8d–r15d; no REX.W for 32-bit operand size.
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xB8 + (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `mov reg16, imm16` (16-bit move via operand-size override).
///
/// Phase 7 m4-003 (PA7C-m4-003): width-threaded form for typed `u16`/`i16`
/// integer-literal `let` bindings. The `66` prefix selects 16-bit operand size.
///
/// Instruction: 66 B8+rd iw (REX.B only for r8w–r15w)
/// Bytes: `66 [41] B8+(reg&7) imm16_le`
///
/// Example: `mov ax, 42` → `66 b8 2a 00` (4 bytes)
pub fn mov_reg16_imm16(buf: &mut CodeBuffer, dst: Reg64, imm: u16) {
    let reg_id = dst as u8;
    buf.bytes.push(0x66);
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xB8 + (reg_id & 7));
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `mov reg8, imm8` (8-bit move).
///
/// Phase 7 m4-003 (PA7C-m4-003): width-threaded form for typed `u8`/`i8`
/// integer-literal `let` bindings.
///
/// Instruction: B0+rb ib (REX.B for r8b–r15b; a bare REX would also be needed
/// to address spl/bpl/sil/dil, but those are not produced here)
/// Bytes: `[41] B0+(reg&7) imm8`
///
/// Example: `mov al, 42` → `b0 2a` (2 bytes)
///
/// PA10-004 CONTRACT: RegIds 0–3 emit without REX, producing the classic 8-bit forms:
/// - RegId(0–3) with NO REX → B0–B3 + imm8 → al, cl, dl, bl (low-byte low-reg)
/// - HIGH-BYTE REGS ah–bh also map to RegId(4–7), and they ALSO emit without REX,
///   producing B4–B7 + imm8 (distinguished by the AST context only, not the bytes).
/// - RegId(8–15) with REX.B (0x41) → 41 B0–B7 + imm8 → r8b–r15b (extended low-byte)
///
/// If spl/bpl/sil/dil are added in a later phase, they require a different entry
/// point or special handling to preserve the high-byte aliasing trap: they also
/// need ids 4–7 but WITH bare REX (not REX.B).
pub fn mov_reg8_imm8(buf: &mut CodeBuffer, dst: Reg64, imm: u8) {
    let reg_id = dst as u8;
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xB0 + (reg_id & 7));
    buf.bytes.push(imm);
}

/// Encode `mov reg64, reg64`.
///
/// Instruction: REX.W 89 /r
/// ModR/M: 0xC0 | (src<<3) | dst
/// Bytes: `48+REX 89 (0xC0 | (src<<3) | dst)`
/// where REX.W=1, REX.R=src>>3, REX.B=dst>>3
pub fn mov_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (dst_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x89);
    buf.bytes.push(0xC0 | ((src_id & 7) << 3) | (dst_id & 7));
}

/// Encode `mov sreg, r16` (move from register to segment register).
///
/// Phase 15 m5-002: Encodes segment register MOV instructions.
/// Uses opcode 8E /r with no REX prefix (segment MOV does not use REX.W).
///
/// Instruction: `8E /r` (2 bytes)
/// ModR/M: `0xC0 | (sreg_id << 3) | (src_reg_id & 7)`
///
/// Example: `mov ds, ax` → `8E D8` (sreg_id=3 for DS, src_reg_id=0 for AX)
pub fn mov_sreg_reg16(buf: &mut CodeBuffer, sreg_id: u8, src: Reg64) {
    let src_id = src as u8;
    buf.bytes.push(0x8E); // MOV sreg, r16 opcode
    buf.bytes.push(0xC0 | ((sreg_id & 7) << 3) | (src_id & 7));
}

/// Encode `mov [rbp+disp], reg64` (store register to memory).
///
/// Uses the smallest form possible:
/// - If disp fits in i8 and is not 0: mod=01, disp8 (2 bytes for disp)
/// - If disp is 0: still use disp8=0 (RBP with mod=00 is special; we use mod=01 with disp8=0)
/// - Otherwise: mod=10, disp32 (4 bytes for disp)
///
/// Instruction: `REX.W 89 /r [ModR/M] [disp]`
pub fn mov_mem_rbp_disp_reg64(buf: &mut CodeBuffer, disp: i32, src: Reg64) {
    let src_id = src as u8;
    let rbp_id = 5u8; // RBP is register id 5
    let rex_byte = rex(true, (src_id >> 3) != 0, false, false);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x89);
    emit_mem_base_disp(buf, src_id & 7, rbp_id, disp);
}

/// Encode `mov reg64, [rbp+disp]` (load register from memory).
///
/// Uses the smallest form possible (same logic as `mov_mem_rbp_disp_reg64`).
///
/// Instruction: `REX.W 8B /r [ModR/M] [disp]`
pub fn mov_reg64_mem_rbp_disp(buf: &mut CodeBuffer, dst: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let rbp_id = 5u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, false);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x8B);
    emit_mem_base_disp(buf, dst_id & 7, rbp_id, disp);
}

/// Encode `add reg64, reg64`.
///
/// Instruction: REX.W 01 /r
/// ModR/M: 0xC0 | (src<<3) | dst
pub fn add_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (dst_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x01);
    buf.bytes.push(0xC0 | ((src_id & 7) << 3) | (dst_id & 7));
}

/// Encode `sub reg64, reg64`.
///
/// Instruction: REX.W 29 /r
/// ModR/M: 0xC0 | (src<<3) | dst
pub fn sub_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (dst_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x29);
    buf.bytes.push(0xC0 | ((src_id & 7) << 3) | (dst_id & 7));
}

/// Encode `add reg64, [base + disp]` (add from memory) — issue #1328.
///
/// Instruction: REX.W 03 /r
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
/// The `03` opcode is the load direction (r64, r/m64); `01` is the store form
/// used by add_reg64_reg64 for (r/m64, r64).
pub fn add_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x03);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `add reg64, [base + index*scale + disp]` (SIB load) — issue #1328.
///
/// Instruction: REX.W 03 /r SIB [disp]
/// REX: W set for 64-bit, R for dst in r8..r15, X for index in r8..r15,
/// B for base in r8..r15.
pub fn add_reg64_mem_sib_disp(
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
    buf.bytes.push(0x03);
    emit_mem_sib_disp(buf, dst_id & 7, base_id, index_id, scale_bits, disp);
}

/// Encode `sub reg64, [base + disp]` (subtract from memory) — issue #1328.
///
/// Instruction: REX.W 2B /r
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
/// The `2B` opcode is the load direction (r64, r/m64); `29` is the store form
/// used by sub_reg64_reg64 for (r/m64, r64).
pub fn sub_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x2B);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `sub reg64, [base + index*scale + disp]` (SIB load) — issue #1328.
///
/// Instruction: REX.W 2B /r SIB [disp]
/// REX: W set for 64-bit, R for dst in r8..r15, X for index in r8..r15,
/// B for base in r8..r15.
pub fn sub_reg64_mem_sib_disp(
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
    buf.bytes.push(0x2B);
    emit_mem_sib_disp(buf, dst_id & 7, base_id, index_id, scale_bits, disp);
}

/// Encode `adc reg64, reg64` (add with carry).
///
/// Instruction: REX.W 13 /r
/// ModR/M: 0xC0 | (dst<<3) | src
/// Reads and writes CF.
pub fn adc_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x13);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `adc reg64, [base + disp]` (add with carry from memory).
///
/// Instruction: REX.W 13 /r
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
/// Reads and writes CF.
pub fn adc_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x13);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `sbb reg64, reg64` (subtract with borrow).
///
/// Instruction: REX.W 1B /r
/// ModR/M: 0xC0 | (dst<<3) | src
/// Reads and writes CF.
pub fn sbb_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x1B);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `sbb reg64, [base + disp]` (subtract with borrow from memory).
///
/// Instruction: REX.W 1B /r
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
/// Reads and writes CF.
pub fn sbb_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x1B);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `adc reg32, reg32` (add with carry, 32-bit).
///
/// Instruction: 13 /r (no REX.W)
/// ModR/M: 0xC0 | (dst<<3) | src
/// Reads and writes CF.
pub fn adc_reg32_reg32(buf: &mut CodeBuffer, dst_id: u8, src_id: u8) {
    if (dst_id >> 3) != 0 || (src_id >> 3) != 0 {
        let rex_byte = rex(false, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x13);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `adc reg32, [base + disp]` (add with carry from memory, 32-bit).
///
/// Instruction: 13 /r (no REX.W)
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
/// Reads and writes CF.
pub fn adc_reg32_mem_base_disp(buf: &mut CodeBuffer, dst_id: u8, base_id: u8, disp: i32) {
    if (dst_id >> 3) != 0 || (base_id >> 3) != 0 {
        let rex_byte = rex(false, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x13);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `sbb reg32, reg32` (subtract with borrow, 32-bit).
///
/// Instruction: 1B /r (no REX.W)
/// ModR/M: 0xC0 | (dst<<3) | src
/// Reads and writes CF.
pub fn sbb_reg32_reg32(buf: &mut CodeBuffer, dst_id: u8, src_id: u8) {
    if (dst_id >> 3) != 0 || (src_id >> 3) != 0 {
        let rex_byte = rex(false, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x1B);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `sbb reg32, [base + disp]` (subtract with borrow from memory, 32-bit).
///
/// Instruction: 1B /r (no REX.W)
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
/// Reads and writes CF.
pub fn sbb_reg32_mem_base_disp(buf: &mut CodeBuffer, dst_id: u8, base_id: u8, disp: i32) {
    if (dst_id >> 3) != 0 || (base_id >> 3) != 0 {
        let rex_byte = rex(false, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x1B);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

