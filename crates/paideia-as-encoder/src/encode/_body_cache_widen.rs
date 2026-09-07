/// Encode `mfence` — PA-R13-005 (issue #918).
///
/// Instruction: 0F AE F0. Zero operands. Serializing memory barrier.
pub fn mfence(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAE);
    buf.bytes.push(0xF0);
}

/// Encode `sfence` — PA-R14-004 (issue #947).
///
/// Instruction: 0F AE F8. Zero operands. Store fence barrier.
pub fn sfence(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAE);
    buf.bytes.push(0xF8);
}

/// Encode `lfence` — PA-R14-004 (issue #947).
///
/// Instruction: 0F AE E8. Zero operands. Load fence barrier.
pub fn lfence(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAE);
    buf.bytes.push(0xE8);
}

/// Encode `pause` — PA-R16-007 (issue #973).
///
/// Instruction: F3 90 (2 bytes). Zero operands. Spinloop hint.
/// Architecturally equivalent to NOP; on modern µarch reduces power
/// and prevents memory-ordering violations in spin-wait loops.
pub fn pause(buf: &mut CodeBuffer) {
    buf.bytes.push(0xF3);
    buf.bytes.push(0x90);
}

/// Encode `wbinvd` — PA-R14-005 (issue #948).
///
/// Instruction: 0F 09. Zero operands. Write-back and invalidate cache. Privileged.
pub fn wbinvd(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0x09);
}

/// Encode `invd` — PA-R14-005 (issue #948).
///
/// Instruction: 0F 08. Zero operands. Invalidate cache without write-back. Privileged.
pub fn invd(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0x08);
}

/// Encode `fxsave [base + disp]` — PA-R13-007 (issue #920).
/// Instruction: 0F AE /0 (reg field = 000). Saves x87/MMX/SSE state.
/// REX.B for r8-r15 base; no REX.W.
pub fn fxsave_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    let base_id = base as u8;
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAE);
    emit_mem_base_disp(buf, 0, base_id, disp);
}

/// Encode `fxrstor [base + disp]` — PA-R13-007 (issue #920).
/// Instruction: 0F AE /1 (reg field = 001). Restores state from a matching fxsave frame.
pub fn fxrstor_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    let base_id = base as u8;
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAE);
    emit_mem_base_disp(buf, 1, base_id, disp);
}

/// Encode `xsaveopt [base + disp]` — PA-R15-m4-005 (issue #1022).
/// Instruction: 0F AE /6 (reg field = 110). Optimized save of processor extended state.
pub fn xsaveopt_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    let base_id = base as u8;
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAE);
    emit_mem_base_disp(buf, 6, base_id, disp);
}

/// Encode `xrstor [base + disp]` — PA-R15-m4-005 (issue #1022).
/// Instruction: 0F AE /5 (reg field = 101). Restore processor extended state.
pub fn xrstor_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    let base_id = base as u8;
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAE);
    emit_mem_base_disp(buf, 5, base_id, disp);
}

/// Encode `clflush [base + disp]` — PA-R14-005 (issue #948).
/// Instruction: 0F AE /7 (reg field = 111). Flushes cache line to main memory.
/// REX.B for r8-r15 base; no REX.W.
pub fn clflush_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    let base_id = base as u8;
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAE);
    emit_mem_base_disp(buf, 7, base_id, disp);
}

/// Encode `clflushopt [base + disp]` — PA-R14-005 (issue #948).
/// Instruction: 66 0F AE /7 (reg field = 111). Optimized cache line flush with 0x66 prefix.
/// REX.B for r8-r15 base; no REX.W.
pub fn clflushopt_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    buf.bytes.push(0x66);
    let base_id = base as u8;
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAE);
    emit_mem_base_disp(buf, 7, base_id, disp);
}

/// Encode `prefetchnta [base + disp]` — PA-R14-006 (issue #949).
/// Instruction: 0F 18 /0 (reg field = 000). Non-temporal prefetch.
/// REX.B for r8-r15 base; no REX.W.
pub fn prefetchnta_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    let base_id = base as u8;
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0x18);
    emit_mem_base_disp(buf, 0, base_id, disp);
}

/// Encode `prefetcht0 [base + disp]` — PA-R14-006 (issue #949).
/// Instruction: 0F 18 /1 (reg field = 001). Temporal prefetch (all cache levels).
/// REX.B for r8-r15 base; no REX.W.
pub fn prefetcht0_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    let base_id = base as u8;
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0x18);
    emit_mem_base_disp(buf, 1, base_id, disp);
}

/// Encode `prefetcht1 [base + disp]` — PA-R14-006 (issue #949).
/// Instruction: 0F 18 /2 (reg field = 010). Temporal prefetch (L2 down).
/// REX.B for r8-r15 base; no REX.W.
pub fn prefetcht1_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    let base_id = base as u8;
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0x18);
    emit_mem_base_disp(buf, 2, base_id, disp);
}

/// Encode `prefetcht2 [base + disp]` — PA-R14-006 (issue #949).
/// Instruction: 0F 18 /3 (reg field = 011). Temporal prefetch (L3 down).
/// REX.B for r8-r15 base; no REX.W.
pub fn prefetcht2_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    let base_id = base as u8;
    if (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0x18);
    emit_mem_base_disp(buf, 3, base_id, disp);
}

/// Encode `div src64` (unsigned 64-bit divide, register operand).
///
/// Phase R11 PA-R11-006: the divisor is read from src, quotient written to rax, remainder to rdx.
///
/// Opcode: REX.W F7 /6
/// ModR/M: mod=11 (register direct), r/m = src.
/// Bytes: `48+REX.B F7 (0xF0 | (reg & 7))`
///
/// Example: `div rax` → `48 f7 f0`
/// Example: `div r8`  → `49 f7 f0`
pub fn div_reg64(buf: &mut CodeBuffer, src: Reg64) {
    let reg_id = src as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xF7);
    buf.bytes.push(0xF0 | (reg_id & 7));
}

/// Encode `mul src64` (unsigned 64-bit multiply, register operand).
///
/// paideia-as#1398: the multiplier is read from src, multiplicand implicit in rax;
/// the 128-bit unsigned product lands in rdx:rax (low 64 in rax, high 64 in rdx).
/// Complements `imul` (signed low-64) and `div` (128÷64) for wide-integer emulation.
///
/// Opcode: REX.W F7 /4
/// ModR/M: mod=11 (register direct), r/m = src, reg-field = /4 (opcode extension).
/// Bytes: `48+REX.B F7 (0xE0 | (reg & 7))`
///
/// Example: `mul rax` → `48 f7 e0`
/// Example: `mul rcx` → `48 f7 e1`
/// Example: `mul r8`  → `49 f7 e0`
/// Example: `mul r15` → `49 f7 e7`
pub fn mul_reg64(buf: &mut CodeBuffer, src: Reg64) {
    let reg_id = src as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xF7);
    buf.bytes.push(0xE0 | (reg_id & 7));
}

/// Encode `idiv src64` (signed 64-bit divide, register operand).
///
/// Phase R11 PA-R11-006: the divisor is read from src, quotient written to rax, remainder to rdx.
///
/// Opcode: REX.W F7 /7
/// ModR/M: mod=11 (register direct), r/m = src.
/// Bytes: `48+REX.B F7 (0xF8 | (reg & 7))`
///
/// Example: `idiv rax` → `48 f7 f8`
/// Example: `idiv r8`  → `49 f7 f8`
pub fn idiv_reg64(buf: &mut CodeBuffer, src: Reg64) {
    let reg_id = src as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xF7);
    buf.bytes.push(0xF8 | (reg_id & 7));
}

/// Encode `movsx dst64, src` (move with sign-extend, register-to-register).
///
/// Phase 7 m4-002: used for widening *signed* casts. The source register is
/// read at `src_width` bytes (1, 2, or 4) and sign-extended into the 64-bit
/// destination.
///
/// Opcode by source width:
/// - 1 byte (r/m8 → r64):  `REX.W 0F BE /r`
/// - 2 bytes (r/m16 → r64): `REX.W 0F BF /r`
/// - 4 bytes (r/m32 → r64): `REX.W 63 /r` (MOVSXD; single-byte opcode)
///
/// ModR/M: mod=11 (register direct), reg = dst, r/m = src.
///
/// Example: `movsx rax, ecx` (4-byte src) → `48 63 c1`
/// Example: `movsx rax, cl`  (1-byte src) → `48 0f be c1`
///
/// Returns `false` (emitting nothing) if `src_width` is not 1, 2, or 4.
pub fn movsx_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64, src_width: u8) -> bool {
    let dst_id = dst as u8;
    let src_id = src as u8;
    // dst is the ModR/M.reg field (R extension); src is r/m (B extension).
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    let modrm = 0xC0 | ((dst_id & 7) << 3) | (src_id & 7);
    match src_width {
        1 => {
            buf.bytes.push(rex_byte);
            buf.bytes.push(0x0F);
            buf.bytes.push(0xBE);
            buf.bytes.push(modrm);
            true
        }
        2 => {
            buf.bytes.push(rex_byte);
            buf.bytes.push(0x0F);
            buf.bytes.push(0xBF);
            buf.bytes.push(modrm);
            true
        }
        4 => {
            buf.bytes.push(rex_byte);
            buf.bytes.push(0x63);
            buf.bytes.push(modrm);
            true
        }
        _ => false,
    }
}

/// Encode `movzx dst64, src` (move with zero-extend, register-to-register).
///
/// Phase 7 m4-002: used for widening *unsigned* casts of 1- or 2-byte
/// sources. For 4-byte sources, a plain `mov r32, r32` already zero-extends,
/// so callers should use that instead (this returns `false` for width 4).
///
/// Opcode by source width:
/// - 1 byte (r/m8 → r64):  `REX.W 0F B6 /r`
/// - 2 bytes (r/m16 → r64): `REX.W 0F B7 /r`
///
/// ModR/M: mod=11 (register direct), reg = dst, r/m = src.
///
/// Example: `movzx rax, cl` (1-byte src) → `48 0f b6 c1`
///
/// Returns `false` (emitting nothing) if `src_width` is not 1 or 2.
pub fn movzx_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64, src_width: u8) -> bool {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    let modrm = 0xC0 | ((dst_id & 7) << 3) | (src_id & 7);
    match src_width {
        1 => {
            buf.bytes.push(rex_byte);
            buf.bytes.push(0x0F);
            buf.bytes.push(0xB6);
            buf.bytes.push(modrm);
            true
        }
        2 => {
            buf.bytes.push(rex_byte);
            buf.bytes.push(0x0F);
            buf.bytes.push(0xB7);
            buf.bytes.push(modrm);
            true
        }
        _ => false,
    }
}

/// Encode `mov dst32, src32` (32-bit register move).
///
/// Phase 7 m4-002: used for narrowing casts and for unsigned widening of a
/// 4-byte source — writing a 32-bit register implicitly zero-extends the high
/// 32 bits of the destination per the x86_64 architecture.
///
/// Instruction: `8B /r` (no REX.W). REX is emitted only when an extended
/// register (R8..R15) participates.
///
/// ModR/M: mod=11, reg = dst, r/m = src.
///
/// Example: `mov eax, ecx` → `89 c8` (here encoded via 8B form: `8b c1`)
pub fn mov_reg32_reg32(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    // No REX.W; only emit REX if an extended register is used.
    if (dst_id >> 3) != 0 || (src_id >> 3) != 0 {
        buf.bytes
            .push(rex(false, (src_id >> 3) != 0, false, (dst_id >> 3) != 0));
    }
    // Use store form (89 C8) per AC specifications, not load form (8B C1).
    buf.bytes.push(0x89);
    buf.bytes.push(0xC0 | ((src_id & 7) << 3) | (dst_id & 7));
}

/// Encode `mov r32, [abs32]` — Phase 15 m3-002: 32-bit mov from absolute address.
///
/// Instruction: 8B /r, ModR/M with mod=00, r/m=5 (absolute addressing), disp32
/// Bytes: `[41] 8B 05 disp32_le` (REX.B if reg ∈ r8..r15)
///
/// Returns the instruction-local byte offset of the disp32 placeholder.
/// For registers eax..edi, disp32 starts at byte 2 of the instruction.
/// For r8d..r15d, disp32 starts at byte 3 (after REX.B).
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `dst`: destination register (32-bit operand size)
///
/// # Returns
/// Instruction-local byte offset of the disp32 field (2 or 3).
pub fn mov_reg32_mem_abs32(buf: &mut CodeBuffer, dst: Reg64) -> u32 {
    let reg_id = dst as u8;
    let byte_offset_before = buf.len() as u32;

    // REX.B only needed for r8d..r15d; no REX.W for 32-bit operand size.
    if (reg_id >> 3) != 0 {
        buf.bytes.push(0x41); // REX.B
    }

    buf.bytes.push(0x8B); // mov r32, r/m32 opcode
    buf.bytes.push(0x05 | ((reg_id & 7) << 3)); // ModR/M: mod=00, r/m=5 (absolute), reg=reg_id

    let byte_offset_disp32 = (buf.len() as u32) - byte_offset_before;
    buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

    byte_offset_disp32
}

/// Encode `mov [abs32], r32` — Phase 15 m3-002: 32-bit mov to absolute address.
///
/// Instruction: 89 /r, ModR/M with mod=00, r/m=5 (absolute addressing), disp32
/// Bytes: `[41] 89 05 disp32_le` (REX.B if reg ∈ r8..r15)
///
/// Returns the instruction-local byte offset of the disp32 placeholder.
/// For registers eax..edi, disp32 starts at byte 2 of the instruction.
/// For r8d..r15d, disp32 starts at byte 3 (after REX.B).
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `src`: source register (32-bit operand size)
///
/// # Returns
/// Instruction-local byte offset of the disp32 field (2 or 3).
pub fn mov_mem_abs32_reg32(buf: &mut CodeBuffer, src: Reg64) -> u32 {
    let reg_id = src as u8;
    let byte_offset_before = buf.len() as u32;

    // REX.B only needed for r8d..r15d; no REX.W for 32-bit operand size.
    if (reg_id >> 3) != 0 {
        buf.bytes.push(0x41); // REX.B
    }

    buf.bytes.push(0x89); // mov r/m32, r32 opcode
    buf.bytes.push(0x05 | ((reg_id & 7) << 3)); // ModR/M: mod=00, r/m=5 (absolute), reg=reg_id

    let byte_offset_disp32 = (buf.len() as u32) - byte_offset_before;
    buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

    byte_offset_disp32
}

/// Encode `mov [abs32], imm32`.
///
/// Instruction: C7 05 disp32 imm32
/// Helper for Mode32 dispatcher; RelocSite added at dispatcher level.
pub fn mov_mem_abs32_imm32(buf: &mut CodeBuffer, imm: u32) -> u32 {
    let byte_offset_before = buf.len() as u32;
    buf.bytes.push(0xC7);
    buf.bytes.push(0x05);
    let byte_offset_disp32 = (buf.len() as u32) - byte_offset_before;
    buf.bytes.extend([0, 0, 0, 0]);
    buf.bytes.extend(imm.to_le_bytes());
    byte_offset_disp32 // == 2
}

/// Encode `lea r32, [abs32]` — Phase 15 m6-001c: 32-bit LEA to absolute address.
///
/// Instruction: 8D /r, ModR/M with mod=00, r/m=5 (absolute addressing), disp32
/// Bytes: `[41] 8D 05 disp32_le` (REX.B if reg ∈ r8d..r15d)
///
/// Returns the instruction-local byte offset of the disp32 placeholder.
/// For registers eax..edi, disp32 starts at byte 2 of the instruction.
/// For r8d..r15d, disp32 starts at byte 3 (after REX.B).
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `dst`: destination register (32-bit operand size)
///
/// # Returns
/// Instruction-local byte offset of the disp32 field (2 or 3).
pub fn lea_reg32_mem_abs32(buf: &mut CodeBuffer, dst: Reg64) -> u32 {
    let reg_id = dst as u8;
    let byte_offset_before = buf.len() as u32;

    // REX.B only needed for r8d..r15d; no REX.W for 32-bit operand size.
    if (reg_id >> 3) != 0 {
        buf.bytes.push(0x41); // REX.B
    }

    buf.bytes.push(0x8D); // lea r32, r/m32 opcode
    buf.bytes.push(0x05 | ((reg_id & 7) << 3)); // ModR/M: mod=00, r/m=5 (absolute), reg=reg_id

    let byte_offset_disp32 = (buf.len() as u32) - byte_offset_before;
    buf.bytes.extend([0, 0, 0, 0]); // placeholder disp32

    byte_offset_disp32
}

/// Encode `cmp reg64, reg64`.
///
/// Instruction: REX.W 39 /r
/// ModR/M: 0xC0 | (src<<3) | dst
pub fn cmp_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (dst_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x39);
    buf.bytes.push(0xC0 | ((src_id & 7) << 3) | (dst_id & 7));
}

