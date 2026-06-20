//! x86_64 instruction encoding for phase-1 smoke testing.
//!
//! This module provides a typed API for encoding a minimal x86_64 instruction set
//! to raw bytes. All encodings follow Intel SDM Vol 2A exactly.
//!
//! The encoder is stateless; callers maintain a `CodeBuffer` and pass it to
//! individual instruction functions.

/// x86_64 general-purpose 64-bit register identifier.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum Reg64 {
    /// RAX (register id 0)
    Rax = 0,
    /// RCX (register id 1)
    Rcx = 1,
    /// RDX (register id 2)
    Rdx = 2,
    /// RBX (register id 3)
    Rbx = 3,
    /// RSP (register id 4)
    Rsp = 4,
    /// RBP (register id 5)
    Rbp = 5,
    /// RSI (register id 6)
    Rsi = 6,
    /// RDI (register id 7)
    Rdi = 7,
    /// R8 (register id 8)
    R8 = 8,
    /// R9 (register id 9)
    R9 = 9,
    /// R10 (register id 10)
    R10 = 10,
    /// R11 (register id 11)
    R11 = 11,
    /// R12 (register id 12)
    R12 = 12,
    /// R13 (register id 13)
    R13 = 13,
    /// R14 (register id 14)
    R14 = 14,
    /// R15 (register id 15)
    R15 = 15,
}

/// x86_64 general-purpose 32-bit register identifier (lower half of 64-bit registers).
///
/// These are the 32-bit names for registers 0-7. Phase-1 uses 64-bit instructions
/// as the primary case; 32-bit forms are a follow-up.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum Reg32 {
    /// EAX (register id 0)
    Eax = 0,
    /// ECX (register id 1)
    Ecx = 1,
    /// EDX (register id 2)
    Edx = 2,
    /// EBX (register id 3)
    Ebx = 3,
    /// ESP (register id 4)
    Esp = 4,
    /// EBP (register id 5)
    Ebp = 5,
    /// ESI (register id 6)
    Esi = 6,
    /// EDI (register id 7)
    Edi = 7,
}

/// Conditional jump condition codes (used in `0F 8X` two-byte opcodes).
///
/// The second byte of a two-byte jump is `0x80 + (cond as u8)`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum Cond {
    /// JE / JZ (equal / zero): `0F 84`
    Eq = 0x84,
    /// JNE / JNZ (not equal / not zero): `0F 85`
    Neq = 0x85,
    /// JL (less than, signed): `0F 8C`
    Lt = 0x8C,
    /// JGE (greater than or equal, signed): `0F 8D`
    Ge = 0x8D,
    /// JLE (less than or equal, signed): `0F 8E`
    Le = 0x8E,
    /// JG (greater than, signed): `0F 8F`
    Gt = 0x8F,
}

/// A buffer that encodes instructions append bytes to.
///
/// `CodeBuffer` is the output target for all encoding functions.
#[derive(Default, Debug)]
pub struct CodeBuffer {
    /// The encoded instruction bytes.
    pub bytes: Vec<u8>,
}

impl CodeBuffer {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the number of bytes in the buffer.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Return true if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Return a slice of the encoded bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}

// Helper to emit a REX prefix byte.
// REX format: 0x40 | (W<<3) | (R<<2) | (X<<1) | B
// W: 64-bit operand size (1 for 64-bit)
// R: REX extension for ModR/M.reg
// X: REX extension for SIB.index (not used in phase-1)
// B: REX extension for ModR/M.rm or SIB.base
fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | (u8::from(w) << 3) | (u8::from(r) << 2) | (u8::from(x) << 1) | u8::from(b)
}

// REX prefix for 64-bit operand (W=1, R=0, X=0, B=0).
fn rex_w() -> u8 {
    rex(true, false, false, false)
}

/// Encode `mov reg64, imm32` (sign-extended to 64-bit).
///
/// Instruction: REX.W C7 /0 id
/// Bytes: `48 C7 (0xC0 | (reg & 7)) imm32_le`
///
/// Example: `mov rax, 1` → `48 c7 c0 01 00 00 00`
pub fn mov_reg64_imm32(buf: &mut CodeBuffer, dst: Reg64, imm: i32) {
    let reg_id = dst as u8;
    buf.bytes.push(rex_w());
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
    let rbp_rm = 5u8; // RBP is register id 5
    let rex_byte = rex(true, (src_id >> 3) != 0, false, false);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x89);

    if (-128..=127).contains(&disp) {
        // Use mod=01, disp8
        buf.bytes.push(0x40 | ((src_id & 7) << 3) | rbp_rm);
        buf.bytes.push(disp as u8);
    } else {
        // Use mod=10, disp32
        buf.bytes.push(0x80 | ((src_id & 7) << 3) | rbp_rm);
        buf.bytes.extend(disp.to_le_bytes());
    }
}

/// Encode `mov reg64, [rbp+disp]` (load register from memory).
///
/// Uses the smallest form possible (same logic as `mov_mem_rbp_disp_reg64`).
///
/// Instruction: `REX.W 8B /r [ModR/M] [disp]`
pub fn mov_reg64_mem_rbp_disp(buf: &mut CodeBuffer, dst: Reg64, disp: i32) {
    let dst_id = dst as u8;
    let rbp_rm = 5u8;
    let rex_byte = rex(true, (dst_id >> 3) != 0, false, false);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x8B);

    if (-128..=127).contains(&disp) {
        // Use mod=01, disp8
        buf.bytes.push(0x40 | ((dst_id & 7) << 3) | rbp_rm);
        buf.bytes.push(disp as u8);
    } else {
        // Use mod=10, disp32
        buf.bytes.push(0x80 | ((dst_id & 7) << 3) | rbp_rm);
        buf.bytes.extend(disp.to_le_bytes());
    }
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

/// Encode `xor reg64, reg64`.
///
/// Instruction: REX.W 31 /r
/// ModR/M: 0xC0 | (src<<3) | dst
pub fn xor_reg64_reg64(buf: &mut CodeBuffer, dst: Reg64, src: Reg64) {
    let dst_id = dst as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (dst_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0x31);
    buf.bytes.push(0xC0 | ((src_id & 7) << 3) | (dst_id & 7));
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

/// Encode conditional jump `jcc rel32`.
///
/// Instruction: 0F 8X cd (where X is the condition code)
/// Total size: 6 bytes
pub fn jcc_rel32(buf: &mut CodeBuffer, cond: Cond, rel: i32) {
    buf.bytes.push(0x0F);
    buf.bytes.push(cond as u8);
    buf.bytes.extend(rel.to_le_bytes());
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
    _dest: Reg64,
    base: Reg64,
    index: Reg64,
    width: u32,
    _signed: bool,
) {
    let base_id = base as u8;
    let index_id = index as u8;

    match width {
        1 => {
            // mov al, [base + index]
            // Opcode 8A (no prefix/rex needed for AL)
            buf.bytes.push(0x8A);
            // ModR/M: mod=00, reg=0 (AL), rm=100 (SIB follows)
            buf.bytes.push(0x04);
            // SIB: scale=00 (×1), index, base
            let sib = ((index_id & 7) << 3) | (base_id & 7);
            buf.bytes.push(sib);
        }
        2 => {
            // mov ax, [base + index * 2]
            // Operand-size prefix 0x66
            buf.bytes.push(0x66);
            // Opcode 8B
            buf.bytes.push(0x8B);
            // ModR/M: mod=00, reg=0 (AX), rm=100 (SIB follows)
            buf.bytes.push(0x04);
            // SIB: scale=01 (×2), index, base
            let sib = (1 << 6) | ((index_id & 7) << 3) | (base_id & 7);
            buf.bytes.push(sib);
        }
        4 => {
            // mov eax, [base + index * 4]
            // No prefix, opcode 8B
            buf.bytes.push(0x8B);
            // ModR/M: mod=00, reg=0 (EAX), rm=100 (SIB follows)
            buf.bytes.push(0x04);
            // SIB: scale=10 (×4), index, base
            let sib = (2 << 6) | ((index_id & 7) << 3) | (base_id & 7);
            buf.bytes.push(sib);
        }
        8 => {
            // mov rax, [base + index * 8]
            // REX.W=1
            buf.bytes.push(0x48);
            // Opcode 8B
            buf.bytes.push(0x8B);
            // ModR/M: mod=00, reg=0 (RAX), rm=100 (SIB follows)
            buf.bytes.push(0x04);
            // SIB: scale=11 (×8), index, base
            let sib = (3 << 6) | ((index_id & 7) << 3) | (base_id & 7);
            buf.bytes.push(sib);
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
    _src: Reg64,
    width: u32,
) {
    let base_id = base as u8;
    let index_id = index as u8;

    match width {
        1 => {
            // mov [base + index], al
            // Opcode 88 (no prefix/rex needed for AL)
            buf.bytes.push(0x88);
            // ModR/M: mod=00, reg=0 (AL), rm=100 (SIB follows)
            buf.bytes.push(0x04);
            // SIB: scale=00 (×1), index, base
            let sib = ((index_id & 7) << 3) | (base_id & 7);
            buf.bytes.push(sib);
        }
        2 => {
            // mov [base + index * 2], ax
            // Operand-size prefix 0x66
            buf.bytes.push(0x66);
            // Opcode 89
            buf.bytes.push(0x89);
            // ModR/M: mod=00, reg=0 (AX), rm=100 (SIB follows)
            buf.bytes.push(0x04);
            // SIB: scale=01 (×2), index, base
            let sib = (1 << 6) | ((index_id & 7) << 3) | (base_id & 7);
            buf.bytes.push(sib);
        }
        4 => {
            // mov [base + index * 4], eax
            // No prefix, opcode 89
            buf.bytes.push(0x89);
            // ModR/M: mod=00, reg=0 (EAX), rm=100 (SIB follows)
            buf.bytes.push(0x04);
            // SIB: scale=10 (×4), index, base
            let sib = (2 << 6) | ((index_id & 7) << 3) | (base_id & 7);
            buf.bytes.push(sib);
        }
        8 => {
            // mov [base + index * 8], rax
            // REX.W=1
            buf.bytes.push(0x48);
            // Opcode 89
            buf.bytes.push(0x89);
            // ModR/M: mod=00, reg=0 (RAX), rm=100 (SIB follows)
            buf.bytes.push(0x04);
            // SIB: scale=11 (×8), index, base
            let sib = (3 << 6) | ((index_id & 7) << 3) | (base_id & 7);
            buf.bytes.push(sib);
        }
        _ => panic!(
            "invalid width {} for emit_indexed_store; must be 1, 2, 4, or 8",
            width
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mov_rax_1() {
        let mut buf = CodeBuffer::new();
        mov_reg64_imm32(&mut buf, Reg64::Rax, 1);
        assert_eq!(buf.as_slice(), &[0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn ret_byte() {
        let mut buf = CodeBuffer::new();
        ret(&mut buf);
        assert_eq!(buf.as_slice(), &[0xc3]);
    }

    #[test]
    fn mov_mem_rbp_minus_8_rbx() {
        let mut buf = CodeBuffer::new();
        mov_mem_rbp_disp_reg64(&mut buf, -8, Reg64::Rbx);
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x5d, 0xf8]);
    }

    #[test]
    fn mov_rcx_imm32_42() {
        let mut buf = CodeBuffer::new();
        mov_reg64_imm32(&mut buf, Reg64::Rcx, 42);
        assert_eq!(buf.as_slice(), &[0x48, 0xc7, 0xc1, 0x2a, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn xor_rax_rax() {
        let mut buf = CodeBuffer::new();
        xor_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rax);
        assert_eq!(buf.as_slice(), &[0x48, 0x31, 0xc0]);
    }

    #[test]
    fn add_rcx_rdx() {
        let mut buf = CodeBuffer::new();
        add_reg64_reg64(&mut buf, Reg64::Rcx, Reg64::Rdx);
        assert_eq!(buf.as_slice(), &[0x48, 0x01, 0xd1]);
    }

    #[test]
    fn ret_then_jmp() {
        let mut buf = CodeBuffer::new();
        ret(&mut buf);
        jmp_rel8(&mut buf, 5);
        assert_eq!(buf.as_slice(), &[0xc3, 0xeb, 0x05]);
    }

    #[test]
    fn push_rbp_pop_rbp() {
        let mut buf = CodeBuffer::new();
        push_reg64(&mut buf, Reg64::Rbp);
        pop_reg64(&mut buf, Reg64::Rbp);
        assert_eq!(buf.as_slice(), &[0x55, 0x5d]);
    }

    #[test]
    fn push_r12_pop_r12() {
        let mut buf = CodeBuffer::new();
        push_reg64(&mut buf, Reg64::R12);
        pop_reg64(&mut buf, Reg64::R12);
        assert_eq!(buf.as_slice(), &[0x41, 0x54, 0x41, 0x5c]);
    }

    #[test]
    fn jmp_rel32_neg5() {
        let mut buf = CodeBuffer::new();
        jmp_rel32(&mut buf, -5);
        assert_eq!(buf.as_slice(), &[0xe9, 0xfb, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn je_rel32_neg10() {
        let mut buf = CodeBuffer::new();
        jcc_rel32(&mut buf, Cond::Eq, -10);
        assert_eq!(buf.as_slice(), &[0x0f, 0x84, 0xf6, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn mov_reg64_reg64_r8_r15() {
        let mut buf = CodeBuffer::new();
        mov_reg64_reg64(&mut buf, Reg64::R8, Reg64::R15);
        // REX.W=1, R=1 (for R15), B=1 (for R8): 0x4d
        // 89 (opcode)
        // 0xC0 | (7<<3) | 0 = 0xf8 (R15 is id 15, id&7=7; R8 is id 8, id&7=0)
        assert_eq!(buf.as_slice(), &[0x4d, 0x89, 0xf8]);
    }

    #[test]
    fn sub_rdx_rax() {
        let mut buf = CodeBuffer::new();
        sub_reg64_reg64(&mut buf, Reg64::Rdx, Reg64::Rax);
        assert_eq!(buf.as_slice(), &[0x48, 0x29, 0xc2]);
    }

    #[test]
    fn cmp_rsi_rdi() {
        let mut buf = CodeBuffer::new();
        cmp_reg64_reg64(&mut buf, Reg64::Rsi, Reg64::Rdi);
        assert_eq!(buf.as_slice(), &[0x48, 0x39, 0xfe]);
    }

    #[test]
    fn test_rbx_rbx() {
        let mut buf = CodeBuffer::new();
        test_reg64_reg64(&mut buf, Reg64::Rbx, Reg64::Rbx);
        assert_eq!(buf.as_slice(), &[0x48, 0x85, 0xdb]);
    }

    #[test]
    fn mov_reg64_imm64_large() {
        let mut buf = CodeBuffer::new();
        mov_reg64_imm64(&mut buf, Reg64::Rax, 0x0123456789abcdef);
        // REX.W=1, B=0 (Rax): 0x48
        // B8 (opcode for Rax)
        // 0x0123456789abcdef in little-endian
        assert_eq!(
            buf.as_slice(),
            &[0x48, 0xb8, 0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01]
        );
    }

    #[test]
    fn mov_reg64_imm64_r9() {
        let mut buf = CodeBuffer::new();
        mov_reg64_imm64(&mut buf, Reg64::R9, 0x1000);
        // REX.W=1, B=1 (R9 is id 9 > 7): 0x49
        // B8 + (9&7) = B8 + 1 = B9
        // 0x1000 in little-endian
        assert_eq!(
            buf.as_slice(),
            &[0x49, 0xb9, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn call_rel32_encode() {
        let mut buf = CodeBuffer::new();
        call_rel32(&mut buf, 0x1000);
        assert_eq!(buf.as_slice(), &[0xe8, 0x00, 0x10, 0x00, 0x00]);
    }

    #[test]
    fn jne_rel32() {
        let mut buf = CodeBuffer::new();
        jcc_rel32(&mut buf, Cond::Neq, 100);
        assert_eq!(buf.as_slice(), &[0x0f, 0x85, 0x64, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn jlt_rel32() {
        let mut buf = CodeBuffer::new();
        jcc_rel32(&mut buf, Cond::Lt, -20);
        assert_eq!(buf.as_slice(), &[0x0f, 0x8c, 0xec, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn mov_mem_rbp_disp32_reg64() {
        let mut buf = CodeBuffer::new();
        mov_mem_rbp_disp_reg64(&mut buf, 1000, Reg64::Rax);
        // mod=10 (0x80), disp32
        // 0x80 | (0<<3) | 5 = 0x85
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x85, 0xe8, 0x03, 0x00, 0x00]);
    }

    #[test]
    fn mov_mem_rbp_disp0_reg64() {
        let mut buf = CodeBuffer::new();
        mov_mem_rbp_disp_reg64(&mut buf, 0, Reg64::Rcx);
        // disp=0 fits in i8, so use mod=01 disp8=0
        // 0x40 | (1<<3) | 5 = 0x4d
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x4d, 0x00]);
    }

    #[test]
    fn mov_reg64_mem_rbp_disp8_rdx() {
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_rbp_disp(&mut buf, Reg64::Rdx, -16);
        // mod=01 (0x40), disp8
        // 0x40 | (2<<3) | 5 = 0x55
        assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x55, 0xf0]);
    }

    #[test]
    fn mov_reg64_mem_rbp_disp32_r11() {
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_rbp_disp(&mut buf, Reg64::R11, 2000);
        // mod=10 (0x80), disp32
        // REX.W=1, R=1 (R11 is id 11 > 7): 0x4c
        // 0x80 | (3<<3) | 5 = 0x9d (R11 is id 11, id&7=3)
        assert_eq!(buf.as_slice(), &[0x4c, 0x8b, 0x9d, 0xd0, 0x07, 0x00, 0x00]);
    }

    #[test]
    fn jge_rel32() {
        let mut buf = CodeBuffer::new();
        jcc_rel32(&mut buf, Cond::Ge, 50);
        assert_eq!(buf.as_slice(), &[0x0f, 0x8d, 0x32, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn jle_rel32() {
        let mut buf = CodeBuffer::new();
        jcc_rel32(&mut buf, Cond::Le, -50);
        assert_eq!(buf.as_slice(), &[0x0f, 0x8e, 0xce, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn jgt_rel32() {
        let mut buf = CodeBuffer::new();
        jcc_rel32(&mut buf, Cond::Gt, 0);
        assert_eq!(buf.as_slice(), &[0x0f, 0x8f, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn jmp_rel8_short() {
        let mut buf = CodeBuffer::new();
        jmp_rel8(&mut buf, 10);
        assert_eq!(buf.as_slice(), &[0xeb, 0x0a]);
    }

    #[test]
    fn jmp_rel8_backward() {
        let mut buf = CodeBuffer::new();
        jmp_rel8(&mut buf, -10);
        assert_eq!(buf.as_slice(), &[0xeb, 0xf6]);
    }

    #[test]
    fn add_r10_r12() {
        let mut buf = CodeBuffer::new();
        add_reg64_reg64(&mut buf, Reg64::R10, Reg64::R12);
        // REX.W=1, R=1 (R12 is id 12 > 7), B=1 (R10 is id 10 > 7): 0x4d
        // 01 (opcode)
        // 0xC0 | (4<<3) | 2 = 0xe2 (R12 is id 12, id&7=4; R10 is id 10, id&7=2)
        assert_eq!(buf.as_slice(), &[0x4d, 0x01, 0xe2]);
    }

    #[test]
    fn xor_r15_r15() {
        let mut buf = CodeBuffer::new();
        xor_reg64_reg64(&mut buf, Reg64::R15, Reg64::R15);
        // REX.W=1, R=1 (R15 is id 15 > 7), B=1 (R15 is id 15 > 7): 0x4d
        // 31 (opcode)
        // 0xC0 | (7<<3) | 7 = 0xff (R15 is id 15, id&7=7 for both)
        assert_eq!(buf.as_slice(), &[0x4d, 0x31, 0xff]);
    }

    #[test]
    fn cmp_r9_r14() {
        let mut buf = CodeBuffer::new();
        cmp_reg64_reg64(&mut buf, Reg64::R9, Reg64::R14);
        // REX.W=1, R=1 (R14 is id 14 > 7), B=1 (R9 is id 9 > 7): 0x4d
        // 39 (opcode)
        // 0xC0 | (6<<3) | 1 = 0xf1 (R14 is id 14, id&7=6; R9 is id 9, id&7=1)
        assert_eq!(buf.as_slice(), &[0x4d, 0x39, 0xf1]);
    }

    #[test]
    fn test_r11_r13() {
        let mut buf = CodeBuffer::new();
        test_reg64_reg64(&mut buf, Reg64::R11, Reg64::R13);
        // REX.W=1, R=1 (R13 is id 13 > 7), B=1 (R11 is id 11 > 7): 0x4d
        // 85 (opcode)
        // 0xC0 | (5<<3) | 3 = 0xeb (R13 is id 13, id&7=5; R11 is id 11, id&7=3)
        assert_eq!(buf.as_slice(), &[0x4d, 0x85, 0xeb]);
    }

    #[test]
    fn sub_r8_rax() {
        let mut buf = CodeBuffer::new();
        sub_reg64_reg64(&mut buf, Reg64::R8, Reg64::Rax);
        // REX.W=1, R=0 (Rax is id 0 < 8), B=1 (R8 is id 8 > 7): 0x49
        // 29 (opcode)
        // 0xC0 | (0<<3) | 0 = 0xc0 (Rax is id 0, id&7=0; R8 is id 8, id&7=0)
        assert_eq!(buf.as_slice(), &[0x49, 0x29, 0xc0]);
    }

    #[test]
    fn push_rax_pop_rax() {
        let mut buf = CodeBuffer::new();
        push_reg64(&mut buf, Reg64::Rax);
        pop_reg64(&mut buf, Reg64::Rax);
        assert_eq!(buf.as_slice(), &[0x50, 0x58]);
    }

    #[test]
    fn push_r15_pop_r15() {
        let mut buf = CodeBuffer::new();
        push_reg64(&mut buf, Reg64::R15);
        pop_reg64(&mut buf, Reg64::R15);
        // REX.B + opcode: 0x41, 0x57 (push); 0x41, 0x5f (pop)
        assert_eq!(buf.as_slice(), &[0x41, 0x57, 0x41, 0x5f]);
    }

    #[test]
    fn mov_reg64_reg64_rax_rbx() {
        let mut buf = CodeBuffer::new();
        mov_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rbx);
        // REX.W=1, R=0 (Rbx is id 3 < 8), B=0 (Rax is id 0 < 8): 0x48
        // 89 (opcode)
        // 0xC0 | (3<<3) | 0 = 0xd8 (Rbx is id 3; Rax is id 0)
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0xd8]);
    }

    #[test]
    fn mov_mem_rbp_minus_128_rsi() {
        let mut buf = CodeBuffer::new();
        mov_mem_rbp_disp_reg64(&mut buf, -128, Reg64::Rsi);
        // disp=-128 fits in i8, so use mod=01 disp8
        // 0x40 | (6<<3) | 5 = 0x75
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x75, 0x80]);
    }

    #[test]
    fn mov_reg64_mem_rbp_disp8_rdi() {
        let mut buf = CodeBuffer::new();
        mov_reg64_mem_rbp_disp(&mut buf, Reg64::Rdi, 32);
        // disp=32 fits in i8, so use mod=01 disp8
        // 0x40 | (7<<3) | 5 = 0x7d
        assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x7d, 0x20]);
    }

    #[test]
    fn code_buffer_len() {
        let mut buf = CodeBuffer::new();
        assert!(buf.is_empty());
        mov_reg64_imm32(&mut buf, Reg64::Rax, 0);
        assert_eq!(buf.len(), 7);
        assert!(!buf.is_empty());
    }

    // ── Indexed load tests ──────────────────────────────────────

    #[test]
    fn emit_indexed_load_width_1_rax_rdi_rcx() {
        let mut buf = CodeBuffer::new();
        emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 1, false);
        // mov al, [rdi + rcx]
        // Opcode 8A, ModR/M 04, SIB (scale=00, index=001, base=111) = 0x0f
        assert_eq!(buf.as_slice(), &[0x8a, 0x04, 0x0f]);
    }

    #[test]
    fn emit_indexed_load_width_2_rax_rdi_rcx() {
        let mut buf = CodeBuffer::new();
        emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 2, false);
        // mov ax, [rdi + rcx * 2]
        // Prefix 66, Opcode 8B, ModR/M 04, SIB (scale=01, index=001, base=111) = 0x4f
        assert_eq!(buf.as_slice(), &[0x66, 0x8b, 0x04, 0x4f]);
    }

    #[test]
    fn emit_indexed_load_width_4_rax_rdi_rcx() {
        let mut buf = CodeBuffer::new();
        emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 4, false);
        // mov eax, [rdi + rcx * 4]
        // Opcode 8B, ModR/M 04, SIB (scale=10, index=001, base=111) = 0x8f
        assert_eq!(buf.as_slice(), &[0x8b, 0x04, 0x8f]);
    }

    #[test]
    fn emit_indexed_load_width_8_rax_rdi_rcx() {
        let mut buf = CodeBuffer::new();
        emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false);
        // mov rax, [rdi + rcx * 8]
        // REX.W 48, Opcode 8B, ModR/M 04, SIB (scale=11, index=001, base=111) = 0xcf
        assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x04, 0xcf]);
    }

    // ── Indexed store tests ─────────────────────────────────────

    #[test]
    fn emit_indexed_store_width_1_rax_rdi_rcx() {
        let mut buf = CodeBuffer::new();
        emit_indexed_store(&mut buf, Reg64::Rdi, Reg64::Rcx, Reg64::Rax, 1);
        // mov [rdi + rcx], al
        // Opcode 88, ModR/M 04, SIB (scale=00, index=001, base=111) = 0x0f
        assert_eq!(buf.as_slice(), &[0x88, 0x04, 0x0f]);
    }

    #[test]
    fn emit_indexed_store_width_2_rax_rdi_rcx() {
        let mut buf = CodeBuffer::new();
        emit_indexed_store(&mut buf, Reg64::Rdi, Reg64::Rcx, Reg64::Rax, 2);
        // mov [rdi + rcx * 2], ax
        // Prefix 66, Opcode 89, ModR/M 04, SIB (scale=01, index=001, base=111) = 0x4f
        assert_eq!(buf.as_slice(), &[0x66, 0x89, 0x04, 0x4f]);
    }

    #[test]
    fn emit_indexed_store_width_4_rax_rdi_rcx() {
        let mut buf = CodeBuffer::new();
        emit_indexed_store(&mut buf, Reg64::Rdi, Reg64::Rcx, Reg64::Rax, 4);
        // mov [rdi + rcx * 4], eax
        // Opcode 89, ModR/M 04, SIB (scale=10, index=001, base=111) = 0x8f
        assert_eq!(buf.as_slice(), &[0x89, 0x04, 0x8f]);
    }

    #[test]
    fn emit_indexed_store_width_8_rax_rdi_rcx() {
        let mut buf = CodeBuffer::new();
        emit_indexed_store(&mut buf, Reg64::Rdi, Reg64::Rcx, Reg64::Rax, 8);
        // mov [rdi + rcx * 8], rax
        // REX.W 48, Opcode 89, ModR/M 04, SIB (scale=11, index=001, base=111) = 0xcf
        assert_eq!(buf.as_slice(), &[0x48, 0x89, 0x04, 0xcf]);
    }
}
