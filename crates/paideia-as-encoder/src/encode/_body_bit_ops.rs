/// Encode `popcnt reg64, reg64` (population count, 64-bit).
///
/// Instruction: F3 REX.W 0F B8 /r
/// ModR/M: 0xC0 | (dst<<3) | src
/// CPUID requirement: Nehalem+ (POPCNT bit).
pub fn popcnt_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    buf.bytes.push(0xF3);
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB8);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `popcnt reg64, [base + disp]` (population count from memory, 64-bit).
///
/// Instruction: F3 REX.W 0F B8 /r
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
pub fn popcnt_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    buf.bytes.push(0xF3);
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB8);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `popcnt reg32, reg32` (population count, 32-bit).
///
/// Instruction: F3 0F B8 /r (no REX.W)
/// ModR/M: 0xC0 | (dst<<3) | src
/// Suppress REX when both dst and src are < 8.
pub fn popcnt_reg32_reg32(buf: &mut CodeBuffer, dst_id: u8, src_id: u8) {
    buf.bytes.push(0xF3);
    if (dst_id >> 3) != 0 || (src_id >> 3) != 0 {
        let rex_byte = rex(false, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB8);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `popcnt reg32, [base + disp]` (population count from memory, 32-bit).
///
/// Instruction: F3 0F B8 /r (no REX.W)
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
pub fn popcnt_reg32_mem_base_disp(buf: &mut CodeBuffer, dst_id: u8, base_id: u8, disp: i32) {
    buf.bytes.push(0xF3);
    if (dst_id >> 3) != 0 || (base_id >> 3) != 0 {
        let rex_byte = rex(false, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB8);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `crc32 reg64, reg64` (CRC32 checksum from register, 64-bit).
///
/// Instruction: F2 REX.W 0F 38 F1 /r (PA-R15-006, #1005)
/// ModR/M: 0xC0 | (dst<<3) | src
/// CPUID requirement: SSE 4.2.
pub fn crc32_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    buf.bytes.push(0xF2);
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0x38);
    buf.bytes.push(0xF1);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `crc32 reg64, [base + disp]` (CRC32 checksum from memory, 64-bit).
///
/// Instruction: F2 REX.W 0F 38 F1 /r (PA-R15-006, #1005)
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
pub fn crc32_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    buf.bytes.push(0xF2);
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0x38);
    buf.bytes.push(0xF1);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `bsf reg64, reg64` (bit scan forward, 64-bit).
///
/// Instruction: REX.W 0F BC /r (PA-R16-008, #974)
/// ModR/M: 0xC0 | (dst<<3) | src
pub fn bsf_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBC);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `bsf reg64, [base + disp]` (bit scan forward from memory, 64-bit).
///
/// Instruction: REX.W 0F BC /r (PA-R16-008, #974)
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
pub fn bsf_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBC);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `bsr reg64, reg64` (bit scan reverse, 64-bit).
///
/// Instruction: REX.W 0F BD /r (PA-R16-008, #974)
/// ModR/M: 0xC0 | (dst<<3) | src
pub fn bsr_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBD);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `bsr reg64, [base + disp]` (bit scan reverse from memory, 64-bit).
///
/// Instruction: REX.W 0F BD /r (PA-R16-008, #974)
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
pub fn bsr_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBD);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `tzcnt reg64, reg64` (trailing-zero count, 64-bit).
///
/// Instruction: F3 REX.W 0F BC /r (PA-R16-008, #974)
/// ModR/M: 0xC0 | (dst<<3) | src
/// Requires CPUID.07H:EBX.BMI1[bit 3].
pub fn tzcnt_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    buf.bytes.push(0xF3);
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (src_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBC);
    buf.bytes.push(0xC0 | ((dst_id & 7) << 3) | (src_id & 7));
}

/// Encode `tzcnt reg64, [base + disp]` (trailing-zero count from memory, 64-bit).
///
/// Instruction: F3 REX.W 0F BC /r (PA-R16-008, #974)
/// ModR/M+SIB: emit_mem_base_disp with dst in reg field
/// Requires CPUID.07H:EBX.BMI1[bit 3].
pub fn tzcnt_reg64_mem_base_disp(buf: &mut CodeBuffer, dst: Reg64, base: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let base_id = base as u8;
    buf.bytes.push(0xF3);
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBC);
    emit_mem_base_disp(buf, dst_id & 7, base_id, disp);
}

/// Encode `bt reg64, reg64` (bit test).
///
/// Instruction: REX.W 0F A3 /r (MR form: index in reg, bitmap in rm)
/// ModR/M: 0xC0 | (index<<3) | bitmap
pub fn bt_reg64_reg64(buf: &mut CodeBuffer, bitmap: Reg64, index: Reg64) {
    let bitmap_id = bitmap as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (bitmap_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xA3);
    buf.bytes.push(0xC0 | ((index_id & 7) << 3) | (bitmap_id & 7));
}

/// Encode `bt reg64, [base + disp]` (bit test from memory).
///
/// Instruction: REX.W 0F A3 /r (MR form: index in reg, bitmap in rm/mem)
/// ModR/M+SIB: emit_mem_base_disp with index in reg field
pub fn bt_mem_base_disp_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, index: Reg64) {
    let base_id = base as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xA3);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

/// Encode `bt reg32, reg32` (bit test, 32-bit).
///
/// Instruction: 0F A3 /r (no REX.W, MR form)
/// ModR/M: 0xC0 | (index<<3) | bitmap
pub fn bt_reg32_reg32(buf: &mut CodeBuffer, bitmap_id: u8, index_id: u8) {
    if (bitmap_id >> 3) != 0 || (index_id >> 3) != 0 {
        let rex_byte = rex(false, (index_id >> 3) != 0, false, (bitmap_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xA3);
    buf.bytes.push(0xC0 | ((index_id & 7) << 3) | (bitmap_id & 7));
}

/// Encode `bt reg32, [base + disp]` (bit test from memory, 32-bit).
///
/// Instruction: 0F A3 /r (no REX.W, MR form)
pub fn bt_mem_base_disp_reg32(buf: &mut CodeBuffer, base_id: u8, disp: i32, index_id: u8) {
    if (base_id >> 3) != 0 || (index_id >> 3) != 0 {
        let rex_byte = rex(false, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xA3);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

/// Encode `bts reg64, reg64` (bit test and set).
///
/// Instruction: REX.W 0F AB /r (MR form: index in reg, bitmap in rm)
pub fn bts_reg64_reg64(buf: &mut CodeBuffer, bitmap: Reg64, index: Reg64) {
    let bitmap_id = bitmap as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (bitmap_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAB);
    buf.bytes.push(0xC0 | ((index_id & 7) << 3) | (bitmap_id & 7));
}

/// Encode `bts reg64, [base + disp]` (bit test and set from memory).
///
/// Instruction: REX.W 0F AB /r
pub fn bts_mem_base_disp_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, index: Reg64) {
    let base_id = base as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAB);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

/// Encode `bts reg32, reg32` (bit test and set, 32-bit).
///
/// Instruction: 0F AB /r (no REX.W)
pub fn bts_reg32_reg32(buf: &mut CodeBuffer, bitmap_id: u8, index_id: u8) {
    if (bitmap_id >> 3) != 0 || (index_id >> 3) != 0 {
        let rex_byte = rex(false, (index_id >> 3) != 0, false, (bitmap_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAB);
    buf.bytes.push(0xC0 | ((index_id & 7) << 3) | (bitmap_id & 7));
}

/// Encode `bts reg32, [base + disp]` (bit test and set from memory, 32-bit).
///
/// Instruction: 0F AB /r (no REX.W)
pub fn bts_mem_base_disp_reg32(buf: &mut CodeBuffer, base_id: u8, disp: i32, index_id: u8) {
    if (base_id >> 3) != 0 || (index_id >> 3) != 0 {
        let rex_byte = rex(false, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAB);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

/// Encode `btr reg64, reg64` (bit test and reset).
///
/// Instruction: REX.W 0F B3 /r (MR form: index in reg, bitmap in rm)
pub fn btr_reg64_reg64(buf: &mut CodeBuffer, bitmap: Reg64, index: Reg64) {
    let bitmap_id = bitmap as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (bitmap_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB3);
    buf.bytes.push(0xC0 | ((index_id & 7) << 3) | (bitmap_id & 7));
}

/// Encode `btr reg64, [base + disp]` (bit test and reset from memory).
///
/// Instruction: REX.W 0F B3 /r
pub fn btr_mem_base_disp_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, index: Reg64) {
    let base_id = base as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB3);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

/// Encode `btr reg32, reg32` (bit test and reset, 32-bit).
///
/// Instruction: 0F B3 /r (no REX.W)
pub fn btr_reg32_reg32(buf: &mut CodeBuffer, bitmap_id: u8, index_id: u8) {
    if (bitmap_id >> 3) != 0 || (index_id >> 3) != 0 {
        let rex_byte = rex(false, (index_id >> 3) != 0, false, (bitmap_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB3);
    buf.bytes.push(0xC0 | ((index_id & 7) << 3) | (bitmap_id & 7));
}

/// Encode `btr reg32, [base + disp]` (bit test and reset from memory, 32-bit).
///
/// Instruction: 0F B3 /r (no REX.W)
pub fn btr_mem_base_disp_reg32(buf: &mut CodeBuffer, base_id: u8, disp: i32, index_id: u8) {
    if (base_id >> 3) != 0 || (index_id >> 3) != 0 {
        let rex_byte = rex(false, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB3);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

/// Encode `btc reg64, reg64` (bit test and complement).
///
/// Instruction: REX.W 0F BB /r (MR form: index in reg, bitmap in rm)
pub fn btc_reg64_reg64(buf: &mut CodeBuffer, bitmap: Reg64, index: Reg64) {
    let bitmap_id = bitmap as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (bitmap_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBB);
    buf.bytes.push(0xC0 | ((index_id & 7) << 3) | (bitmap_id & 7));
}

/// Encode `btc reg64, [base + disp]` (bit test and complement from memory).
///
/// Instruction: REX.W 0F BB /r
pub fn btc_mem_base_disp_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, index: Reg64) {
    let base_id = base as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBB);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

/// Encode `btc reg32, reg32` (bit test and complement, 32-bit).
///
/// Instruction: 0F BB /r (no REX.W)
pub fn btc_reg32_reg32(buf: &mut CodeBuffer, bitmap_id: u8, index_id: u8) {
    if (bitmap_id >> 3) != 0 || (index_id >> 3) != 0 {
        let rex_byte = rex(false, (index_id >> 3) != 0, false, (bitmap_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBB);
    buf.bytes.push(0xC0 | ((index_id & 7) << 3) | (bitmap_id & 7));
}

/// Encode `btc reg32, [base + disp]` (bit test and complement from memory, 32-bit).
///
/// Instruction: 0F BB /r (no REX.W)
pub fn btc_mem_base_disp_reg32(buf: &mut CodeBuffer, base_id: u8, disp: i32, index_id: u8) {
    if (base_id >> 3) != 0 || (index_id >> 3) != 0 {
        let rex_byte = rex(false, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
        buf.bytes.push(rex_byte);
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBB);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

/// Encode `lock bts [base + disp], imm8` (bit test and set with immediate, locked).
///
/// Instruction: F0 REX.W 0F BA /5 ib
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
/// Phase R16 PA-R16-002 (issue #968): lock bts immediate form.
pub fn lock_bts_mem_base_disp_imm8(buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: u8) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let rex_byte = rex(true, false, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBA);
    emit_mem_base_disp(buf, 5, base_id, disp); // /5 for BTS
    buf.bytes.push(imm);
}

/// Encode `lock btr [base + disp], imm8` (bit test and reset with immediate, locked).
///
/// Instruction: F0 REX.W 0F BA /6 ib
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
/// Phase R16 PA-R16-002 (issue #968): lock btr immediate form.
pub fn lock_btr_mem_base_disp_imm8(buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: u8) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let rex_byte = rex(true, false, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBA);
    emit_mem_base_disp(buf, 6, base_id, disp); // /6 for BTR
    buf.bytes.push(imm);
}

/// Encode `lock btc [base + disp], imm8` (bit test and complement with immediate, locked).
///
/// Instruction: F0 REX.W 0F BA /7 ib
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
/// Phase R16 PA-R16-002 (issue #968): lock btc immediate form.
pub fn lock_btc_mem_base_disp_imm8(buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: u8) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let rex_byte = rex(true, false, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBA);
    emit_mem_base_disp(buf, 7, base_id, disp); // /7 for BTC
    buf.bytes.push(imm);
}

/// Encode `lock bts [base + disp], index` (bit test and set with register, locked).
///
/// Instruction: F0 REX.W 0F AB /r
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
/// Phase R16 PA-R16-002 (issue #968): lock bts register form.
pub fn lock_bts_mem_base_disp_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, index: Reg64) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xAB);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

/// Encode `lock btr [base + disp], index` (bit test and reset with register, locked).
///
/// Instruction: F0 REX.W 0F B3 /r
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
/// Phase R16 PA-R16-002 (issue #968): lock btr register form.
pub fn lock_btr_mem_base_disp_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, index: Reg64) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB3);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

/// Encode `lock btc [base + disp], index` (bit test and complement with register, locked).
///
/// Instruction: F0 REX.W 0F BB /r
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
/// Phase R16 PA-R16-002 (issue #968): lock btc register form.
pub fn lock_btc_mem_base_disp_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, index: Reg64) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let index_id = index as u8;
    let rex_byte = rex(true, (index_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xBB);
    emit_mem_base_disp(buf, index_id & 7, base_id, disp);
}

