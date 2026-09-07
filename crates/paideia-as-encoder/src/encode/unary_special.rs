//! Unary encoders (NOT, INC, DEC, BSWAP), LTR, MOV memory-immediate variants, XCHG, and CMPXCHG (LOCK forms).

use super::types::*;

/// Encode `not reg64` (bitwise NOT / one's complement).
///
/// Instruction: REX.W F7 /2
/// ModR/M: 0xC0 | (2 << 3) | (reg & 7) = 0xD0 | (reg & 7)
/// (the reg field of ModR/M is the /2 opcode extension for NOT)
/// REX.W=1 for 64-bit; REX.B=reg>>3 for extended registers (R8..R15).
/// Bytes: `48+REX.B F7 (0xD0 | (reg & 7))`
///
/// Example: `not rax` → `48 f7 d0`
/// Example: `not r8`  → `49 f7 d0`
pub fn not_reg64(buf: &mut CodeBuffer, dst: Reg64) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xF7);
    buf.bytes.push(0xD0 | (reg_id & 7));
}

/// Encode `inc reg64` (increment 64-bit register by 1) — PA-R13-005 (issue #934).
///
/// Instruction: REX.W FF /0
/// ModR/M: mod=11 (register direct), reg field = 0 (/0 opcode extension for INC),
/// r/m = dst → `0xC0 | (reg & 7)`.
/// REX.W=1 for 64-bit operand size; REX.B=reg>>3 for extended registers (R8..R15).
/// Bytes: `[REX.W | REX.B] FF (0xC0 | (reg & 7))`
///
/// Note: in x86_64 long mode the 1-byte legacy `40+rd` INC form was repurposed
/// as REX prefixes and is unavailable, so REX.W FF /0 is the canonical form.
///
/// Example: `inc rax` → `48 FF C0`
/// Example: `inc rdi` → `48 FF C7`
/// Example: `inc r8`  → `49 FF C0`
/// Example: `inc r15` → `49 FF C7`
pub fn inc_reg64(buf: &mut CodeBuffer, dst: Reg64) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xFF);
    buf.bytes.push(0xC0 | (reg_id & 7));
}

/// Encode `dec reg64` (decrement 64-bit register by 1) — PA-R13-005 (issue #934).
///
/// Instruction: REX.W FF /1
/// ModR/M: mod=11 (register direct), reg field = 1 (/1 opcode extension for DEC),
/// r/m = dst → `0xC8 | (reg & 7)`.
/// REX.W=1 for 64-bit operand size; REX.B=reg>>3 for extended registers (R8..R15).
/// Bytes: `[REX.W | REX.B] FF (0xC8 | (reg & 7))`
///
/// Note: in x86_64 long mode the 1-byte legacy `48+rd` DEC form was repurposed
/// as REX prefixes and is unavailable, so REX.W FF /1 is the canonical form.
///
/// Example: `dec rax` → `48 FF C8`
/// Example: `dec rdi` → `48 FF CF`
/// Example: `dec r8`  → `49 FF C8`
/// Example: `dec r15` → `49 FF CF`
pub fn dec_reg64(buf: &mut CodeBuffer, dst: Reg64) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xFF);
    buf.bytes.push(0xC8 | (reg_id & 7));
}

/// Encode `bswap r64` (byte-swap 64-bit register) — PA-R13-014 (issue #943).
///
/// Instruction: REX.W 0F C8+rd
/// This is an opcode+register form with no ModR/M byte. The register number
/// is encoded directly in the low 3 bits of the final byte (0xC8 | (reg & 7)).
/// REX.W=1 for 64-bit operand size; REX.B=reg>>3 for extended registers (R8..R15).
/// Bytes: `[REX.W | REX.B] 0F (0xC8 | (reg & 7))`
///
/// Example: `bswap rax` → `48 0F C8`
/// Example: `bswap rdi` → `48 0F CF`
/// Example: `bswap r8`  → `49 0F C8`
/// Example: `bswap r15` → `49 0F CF`
pub fn bswap_reg64(buf: &mut CodeBuffer, dst: Reg64) {
    let reg_id = dst as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xC8 | (reg_id & 7));
}

/// Encode `bswap r32` (byte-swap 32-bit register) — PA-R15-001 (issue #956).
///
/// Instruction: 0F C8+rd (no REX.W)
/// This is an opcode+register form with no ModR/M byte. The register number
/// is encoded directly in the low 3 bits of the final byte (0xC8 | (reg & 7)).
/// REX.B=reg>>3 for extended registers (R8D..R15D), but no REX.W.
/// Bytes: `[REX.B] 0F (0xC8 | (reg & 7))` where REX.B is optional (omitted for reg 0-7).
///
/// Example: `bswap eax` → `0F C8`
/// Example: `bswap edi` → `0F CF`
/// Example: `bswap r8d`  → `41 0F C8`
/// Example: `bswap r15d` → `41 0F CF`
pub fn bswap_reg32(buf: &mut CodeBuffer, dst: Reg32) {
    let reg_id = dst as u8;
    if (reg_id >> 3) != 0 {
        // REX.B only (extended register r8d–r15d), no REX.W
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xC8 | (reg_id & 7));
}

/// Encode `ltr r16` (load task register).
///
/// Instruction: 0F 00 /3 per Intel SDM Vol 2A LTR.
/// ModR/M: mod=11 (register direct), reg field = 3 (/3 opcode extension), r/m = dst.
/// Bytes: `[REX.B] 0F 00 (0xD8 | (reg & 7))`
/// REX.B is emitted for r8..r15 (extended registers); no REX.W (LTR is inherently 16-bit).
/// PA-R13-001 (issue #914).
///
/// Example: `ltr ax`   → `0F 00 D8`
/// Example: `ltr r10`  → `41 0F 00 DA`
pub fn ltr_reg16(buf: &mut CodeBuffer, dst: Reg64) {
    let reg_id = dst as u8;
    if (reg_id >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));  // REX.B
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0x00);
    buf.bytes.push(0xD8 | (reg_id & 7));
}

/// Encode `mov [base + disp], imm8` — PA-R14-001 (issue #944): 8-bit immediate to memory.
///
/// Instruction: [REX.B] C6 /0 <ModR/M+disp> <imm8>
/// ModR/M reg field = 0 (reg_field = 0 in emit_mem_base_disp).
/// REX.B only if base is r8–r15.
pub fn mov_mem_base_disp_imm8(buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: u8) {
    let bid = base as u8;
    if (bid >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xC6);
    emit_mem_base_disp(buf, 0, bid, disp);
    buf.bytes.push(imm);
}

/// Encode `mov [base + disp], imm16` — PA-R14-001 (issue #944): 16-bit immediate to memory.
///
/// Instruction: 66 [REX.B] C7 /0 <ModR/M+disp> <imm16_le>
/// Operand-size override prefix (66) precedes REX.
pub fn mov_mem_base_disp_imm16(buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: u16) {
    buf.bytes.push(0x66); // operand-size override
    let bid = base as u8;
    if (bid >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xC7);
    emit_mem_base_disp(buf, 0, bid, disp);
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `mov [base + disp], imm32` — PA-R14-001 (issue #944): 32-bit immediate to memory.
///
/// Instruction: [REX.B] C7 /0 <ModR/M+disp> <imm32_le>
/// REX.B only if base is r8–r15. No REX.W for 32-bit form.
pub fn mov_mem_base_disp_imm32(buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: u32) {
    let bid = base as u8;
    if (bid >> 3) != 0 {
        buf.bytes.push(rex(false, false, false, true));
    }
    buf.bytes.push(0xC7);
    emit_mem_base_disp(buf, 0, bid, disp);
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `mov [base + disp], imm32 (sign-extended to 64)` — PA-R14-001 (issue #944): 32-bit sign-extended to 64-bit.
///
/// Instruction: REX.W C7 /0 <ModR/M+disp> <imm32_le>
/// REX.W always set; REX.B if base is r8–r15.
pub fn mov_mem_base_disp_imm32_sxt(buf: &mut CodeBuffer, base: Reg64, disp: i32, imm: i32) {
    let bid = base as u8;
    buf.bytes.push(rex(true, false, false, (bid >> 3) != 0));
    buf.bytes.push(0xC7);
    emit_mem_base_disp(buf, 0, bid, disp);
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `mov [base + index*scale + disp], imm8` — PA-R14-001 (issue #944): 8-bit immediate via SIB.
///
/// Instruction: [REX.X/B] C6 /0 SIB <ModR/M+disp> <imm8>
/// ModR/M reg field = 0.
pub fn mov_mem_sib_disp_imm8(buf: &mut CodeBuffer, base: Reg64, index: Reg64, scale_bits: u8, disp: i32, imm: u8) {
    let bid = base as u8;
    let iid = index as u8;
    if (bid | iid) >> 3 != 0 {
        buf.bytes.push(rex(false, false, (iid >> 3) != 0, (bid >> 3) != 0));
    }
    buf.bytes.push(0xC6);
    emit_mem_sib_disp(buf, 0, bid, iid, scale_bits, disp);
    buf.bytes.push(imm);
}

/// Encode `mov [base + index*scale + disp], imm16` — PA-R14-001 (issue #944): 16-bit immediate via SIB.
///
/// Instruction: 66 [REX.X/B] C7 /0 SIB <ModR/M+disp> <imm16_le>
pub fn mov_mem_sib_disp_imm16(buf: &mut CodeBuffer, base: Reg64, index: Reg64, scale_bits: u8, disp: i32, imm: u16) {
    buf.bytes.push(0x66);
    let bid = base as u8;
    let iid = index as u8;
    if (bid | iid) >> 3 != 0 {
        buf.bytes.push(rex(false, false, (iid >> 3) != 0, (bid >> 3) != 0));
    }
    buf.bytes.push(0xC7);
    emit_mem_sib_disp(buf, 0, bid, iid, scale_bits, disp);
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `mov [base + index*scale + disp], imm32` — PA-R14-001 (issue #944): 32-bit immediate via SIB.
///
/// Instruction: [REX.X/B] C7 /0 SIB <ModR/M+disp> <imm32_le>
pub fn mov_mem_sib_disp_imm32(buf: &mut CodeBuffer, base: Reg64, index: Reg64, scale_bits: u8, disp: i32, imm: u32) {
    let bid = base as u8;
    let iid = index as u8;
    if (bid | iid) >> 3 != 0 {
        buf.bytes.push(rex(false, false, (iid >> 3) != 0, (bid >> 3) != 0));
    }
    buf.bytes.push(0xC7);
    emit_mem_sib_disp(buf, 0, bid, iid, scale_bits, disp);
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `mov [base + index*scale + disp], imm32 (sign-extended to 64)` — PA-R14-001 (issue #944): 32-bit sign-extended via SIB.
///
/// Instruction: REX.W [REX.X/B] C7 /0 SIB <ModR/M+disp> <imm32_le>
pub fn mov_mem_sib_disp_imm32_sxt(buf: &mut CodeBuffer, base: Reg64, index: Reg64, scale_bits: u8, disp: i32, imm: i32) {
    let bid = base as u8;
    let iid = index as u8;
    buf.bytes.push(rex(true, false, (iid >> 3) != 0, (bid >> 3) != 0));
    buf.bytes.push(0xC7);
    emit_mem_sib_disp(buf, 0, bid, iid, scale_bits, disp);
    buf.bytes.extend(imm.to_le_bytes());
}

/// Encode `xchg [base + disp], src` — PA-R13-003 (issue #916).
///
/// Instruction: REX.W 87 /r
/// The memory form of XCHG is implicitly locked (Intel SDM Vol 2A) so no
/// LOCK prefix is required. Uses the shared emit_mem_base_disp helper for
/// SIB/BP escape handling.
pub fn xchg_mem_base_disp_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64) {
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x87);
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock cmpxchg [base + disp], src` — PA-R13-004 (issue #917).
///
/// Instruction: F0 REX.W 0F B1 /r
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
pub fn lock_cmpxchg_mem_base_disp_reg64(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB1);
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock cmpxchg [base + disp], src` (32-bit) — PA-R16-003 (issue #969).
///
/// Instruction: F0 [REX] 0F B1 /r (no REX.W)
/// Compares implicit EAX with r/m32; if equal writes reg32, else loads r/m32 into EAX.
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
/// Note: REX is omitted when neither R nor B bits are needed (both src and base are r0–r7).
pub fn lock_cmpxchg_mem_base_disp_reg32(
    buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64,
) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    let src_id = src as u8;
    if (src_id >> 3) != 0 || (base_id >> 3) != 0 {
        buf.bytes.push(rex(false, (src_id >> 3) != 0, false, (base_id >> 3) != 0));
    }
    buf.bytes.push(0x0F);
    buf.bytes.push(0xB1);
    emit_mem_base_disp(buf, src_id & 7, base_id, disp);
}

/// Encode `lock cmpxchg16b [base + disp]` — PA-R16-004 (issue #970).
///
/// Instruction: F0 REX.W 0F C7 /1
/// Group opcode /1 (ModR/M.reg = 001). Implicit register operands: RDX:RAX
/// (expected, high:low), RCX:RBX (new, high:low). ZF=1 on success; ZF=0 loads
/// RDX:RAX ← memory.
///
/// REX is always emitted (REX.W=1 is required). REX.B is set when base∈{r8..r15}.
///
/// Prefix order: LOCK (Group 1) precedes REX (Intel SDM Vol 2A §2.1.1).
/// Requires 16-byte aligned memory operand at runtime (unaligned raises #GP).
/// Requires CPUID.01H:ECX.CMPXCHG16B[bit 13] at runtime.
pub fn lock_cmpxchg16b_mem_base_disp(buf: &mut CodeBuffer, base: Reg64, disp: i32) {
    buf.bytes.push(0xF0); // LOCK prefix
    let base_id = base as u8;
    buf.bytes.push(rex(true, false, false, (base_id >> 3) != 0));
    buf.bytes.push(0x0F);
    buf.bytes.push(0xC7);
    emit_mem_base_disp(buf, /* reg_field = */ 0b001, base_id, disp);
}

