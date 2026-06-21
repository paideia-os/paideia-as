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

/// Encode `sar reg64, imm8` (arithmetic right shift by immediate).
///
/// Instruction: REX.W C1 /7 ib
/// ModR/M: 0xF8 | (reg & 7) (register 7 in the reg field means SAR)
/// Bytes: `48+REX C1 (0xF8 | (reg&7)) imm8`
///
/// Example: `sar rax, 3` → `48 c1 f8 03`
pub fn sar_reg64_imm8(buf: &mut CodeBuffer, reg: Reg64, imm: u8) {
    let reg_id = reg as u8;
    let rex_byte = rex(true, false, false, (reg_id >> 3) != 0);
    buf.bytes.push(rex_byte);
    buf.bytes.push(0xC1);
    buf.bytes.push(0xF8 | (reg_id & 7));
    buf.bytes.push(imm);
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
pub fn cmp_mem_reg64_reg64(buf: &mut CodeBuffer, base: Reg64, disp: i32, src: Reg64) {
    let base_id = base as u8;
    let src_id = src as u8;
    let rex_byte = rex(true, (src_id >> 3) != 0, false, (base_id >> 3) != 0);

    buf.bytes.push(rex_byte);
    buf.bytes.push(0x39);

    if (-128..=127).contains(&disp) {
        // Use mod=01, disp8
        buf.bytes.push(0x40 | ((src_id & 7) << 3) | (base_id & 7));
        buf.bytes.push(disp as u8);
    } else {
        // Use mod=10, disp32
        buf.bytes.push(0x80 | ((src_id & 7) << 3) | (base_id & 7));
        buf.bytes.extend(disp.to_le_bytes());
    }
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

/// Encode conditional jump `jcc rel8` (short form).
///
/// Instruction: 7X cb (where X is the condition code, displacement is relative to end of instruction)
/// Total size: 2 bytes
///
/// Only valid when displacement fits in i8 (-128..=127).
pub fn jcc_rel8(buf: &mut CodeBuffer, cond: Cond, rel: i8) {
    // Convert Cond to rel8 opcode (0x70 + condition code)
    let rel8_opcode = match cond {
        Cond::Eq => 0x74,
        Cond::Neq => 0x75,
        Cond::Lt => 0x7C,
        Cond::Ge => 0x7D,
        Cond::Le => 0x7E,
        Cond::Gt => 0x7F,
    };
    buf.bytes.push(rel8_opcode);
    buf.bytes.push(rel as u8);
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

/// Encode I/O port read instruction: `in al/ax/eax, dx`.
///
/// The SDM fixes the register as al (width=1), ax (width=2), or eax (width=4).
/// The port address register is always DX (implicit in the encoding).
///
/// # Instructions
/// - `in al, dx`: `EC` (1 byte)
/// - `in ax, dx`: `66 ED` (2 bytes, with operand-size prefix)
/// - `in eax, dx`: `ED` (1 byte)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `width`: operand width (1 for al, 2 for ax, 4 for eax)
pub fn encode_in_dx(buf: &mut CodeBuffer, width: u8) {
    match width {
        1 => {
            // in al, dx: EC
            buf.bytes.push(0xEC);
        }
        2 => {
            // in ax, dx: 66 ED (operand-size prefix)
            buf.bytes.push(0x66);
            buf.bytes.push(0xED);
        }
        4 => {
            // in eax, dx: ED
            buf.bytes.push(0xED);
        }
        _ => {
            // Unreachable for valid widths
        }
    }
}

/// Encode I/O port write instruction: `out dx, al/ax/eax`.
///
/// The SDM fixes the register as al (width=1), ax (width=2), or eax (width=4).
/// The port address register is always DX (implicit in the encoding).
///
/// # Instructions
/// - `out dx, al`: `EE` (1 byte)
/// - `out dx, ax`: `66 EF` (2 bytes, with operand-size prefix)
/// - `out dx, eax`: `EF` (1 byte)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `width`: operand width (1 for al, 2 for ax, 4 for eax)
pub fn encode_out_dx(buf: &mut CodeBuffer, width: u8) {
    match width {
        1 => {
            // out dx, al: EE
            buf.bytes.push(0xEE);
        }
        2 => {
            // out dx, ax: 66 EF (operand-size prefix)
            buf.bytes.push(0x66);
            buf.bytes.push(0xEF);
        }
        4 => {
            // out dx, eax: EF
            buf.bytes.push(0xEF);
        }
        _ => {
            // Unreachable for valid widths
        }
    }
}

/// Encode zero-operand control and system instructions.
///
/// # Instructions
/// - `CLI` (Clear Interrupt Flag): `FA` (1 byte)
/// - `STI` (Set Interrupt Flag): `FB` (1 byte)
/// - `HLT` (Halt): `F4` (1 byte)
/// - `NOP` (No Operation): `90` (1 byte)
/// - `SWAPGS` (Swap GS Base): `0F 01 F8` (3 bytes)
/// - `CPUID` (CPU Identification): `0F A2` (2 bytes)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `mnem_byte`: the zero-operand instruction type code
///   - 0x90 for NOP
///   - 0xF4 for HLT
///   - 0xFA for CLI
///   - 0xFB for STI
///   - 0x81 (sentinel) for SWAPGS
///   - 0x82 (sentinel) for CPUID
pub fn encode_zero_operand(buf: &mut CodeBuffer, mnem_byte: u8) {
    match mnem_byte {
        0x90 => {
            // NOP: 90
            buf.bytes.push(0x90);
        }
        0xF4 => {
            // HLT: F4
            buf.bytes.push(0xF4);
        }
        0xFA => {
            // CLI: FA
            buf.bytes.push(0xFA);
        }
        0xFB => {
            // STI: FB
            buf.bytes.push(0xFB);
        }
        0x81 => {
            // SWAPGS: 0F 01 F8 (special encoding; sentinel)
            buf.bytes.push(0x0F);
            buf.bytes.push(0x01);
            buf.bytes.push(0xF8);
        }
        0x82 => {
            // CPUID: 0F A2 (two-byte encoding; sentinel)
            buf.bytes.push(0x0F);
            buf.bytes.push(0xA2);
        }
        _ => {
            // Unreachable for valid mnemonics
        }
    }
}

/// Encode MSR write instruction: `wrmsr` (no operands).
///
/// Model-Specific Register Write: writes the value in EDX:EAX to the MSR
/// specified by the MSR index in ECX. This is a privileged instruction.
///
/// # Instructions
/// - `wrmsr`: `0F 30` (2 bytes)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_wrmsr(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0x30);
}

/// Encode MSR read instruction: `rdmsr` (no operands).
///
/// Model-Specific Register Read: reads the MSR specified by the MSR index
/// in ECX into EDX:EAX. This is a privileged instruction.
///
/// # Instructions
/// - `rdmsr`: `0F 32` (2 bytes)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_rdmsr(buf: &mut CodeBuffer) {
    buf.bytes.push(0x0F);
    buf.bytes.push(0x32);
}

/// Encode software interrupt instruction: `int imm8`.
///
/// Generates a software interrupt with the specified interrupt number.
/// The interrupt number is encoded as an 8-bit immediate value.
///
/// # Instructions
/// - `int N`: `CD <imm8>` (2 bytes)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `imm`: interrupt number (must fit in u8)
pub fn encode_int_imm8(buf: &mut CodeBuffer, imm: u8) {
    buf.bytes.push(0xCD);
    buf.bytes.push(imm);
}

/// Encode interrupt return (32-bit): `iret`.
///
/// Returns from an interrupt handler using the stack-based interrupt frame.
/// In 32-bit mode, pops EIP, CS, and EFLAGS from the stack.
///
/// # Instructions
/// - `iret`: `CF` (1 byte)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_iret(buf: &mut CodeBuffer) {
    buf.bytes.push(0xCF);
}

/// Encode interrupt return (64-bit): `iretq`.
///
/// Returns from an interrupt handler using the stack-based interrupt frame.
/// In 64-bit mode, pops RIP, CS, and RFLAGS from the stack.
/// Requires REX.W prefix to distinguish from 32-bit `iret`.
///
/// # Instructions
/// - `iretq`: `48 CF` (2 bytes, REX.W prefix)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_iretq(buf: &mut CodeBuffer) {
    buf.bytes.push(0x48); // REX.W
    buf.bytes.push(0xCF);
}

/// Encode system return from fast syscall: `sysret`.
///
/// Returns from a fast system call made via `syscall` instruction.
/// In 64-bit mode, loads RIP and CS from MSR_SYSRET_CS, and RFLAGS from R11.
/// Operates in ring 3 only.
///
/// # Instructions
/// - `sysret`: `48 0F 07` (3 bytes, REX.W prefix + two-byte opcode)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_sysret(buf: &mut CodeBuffer) {
    buf.bytes.push(0x48); // REX.W
    buf.bytes.push(0x0F);
    buf.bytes.push(0x07);
}

/// Encode control register MOV instruction: `mov cr_idx, gpr` or `mov gpr, cr_idx`.
///
/// Both forms use the two-byte opcode 0F 22 (write to CR) or 0F 20 (read from CR).
/// CR8 requires REX.R=1 to extend the reg field (cr_idx >= 8).
///
/// # Instructions
/// - `mov cr0, rax`: `0F 22 C0` (3 bytes)
/// - `mov cr3, rax`: `0F 22 D8` (3 bytes, cr_idx=3 in reg field)
/// - `mov cr4, rax`: `0F 22 E0` (3 bytes, cr_idx=4 in reg field)
/// - `mov cr8, rax`: `44 0F 22 C0` (4 bytes, REX.R=1 for extended cr_idx)
/// - `mov rax, cr0`: `0F 20 C0` (3 bytes)
/// - `mov rax, cr3`: `0F 20 D8` (3 bytes)
/// - `mov rax, cr4`: `0F 20 E0` (3 bytes)
/// - `mov rax, cr8`: `44 0F 20 C0` (4 bytes, REX.R=1)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
/// - `write`: true for `mov cr_idx, gpr` (write to CR); false for `mov gpr, cr_idx` (read from CR)
/// - `cr_idx`: control register index (0-4 or 8; phase-5 supports CR0..CR4 + CR8 only)
/// - `gpr_idx`: general-purpose register index (0-15)
pub fn encode_mov_cr(buf: &mut CodeBuffer, write: bool, cr_idx: u8, gpr_idx: u8) {
    // Emit REX.R if cr_idx >= 8 (extends the reg field to support CR8)
    if cr_idx >= 8 {
        buf.bytes.push(0x44); // REX with R=1
    }

    // Emit two-byte opcode
    buf.bytes.push(0x0F);
    buf.bytes.push(if write { 0x22 } else { 0x20 });

    // Emit ModR/M byte: mod=11, reg=cr_idx & 7, r/m=gpr_idx & 7
    let modrm = 0xC0 | ((cr_idx & 7) << 3) | (gpr_idx & 7);
    buf.bytes.push(modrm);
}

/// Encode MOV to/from debug register (0F 23 /r for write, 0F 21 /r for read).
///
/// # Arguments
/// - `write`: true for `mov dr_idx, gpr` (write to DR); false for `mov gpr, dr_idx` (read from DR)
/// - `dr_idx`: debug register index (0-7; phase-5 supports DR0..DR7 only; no aliasing logic)
/// - `gpr_idx`: general-purpose register index (0-15)
pub fn encode_mov_dr(buf: &mut CodeBuffer, write: bool, dr_idx: u8, gpr_idx: u8) {
    // No REX prefix needed; DR0..DR7 are directly encoded in ModR/M.reg (bits [5:3])
    // DR8+ do not exist in x86_64.

    // Emit two-byte opcode
    buf.bytes.push(0x0F);
    buf.bytes.push(if write { 0x23 } else { 0x21 });

    // Emit ModR/M byte: mod=11, reg=dr_idx & 7, r/m=gpr_idx & 7
    let modrm = 0xC0 | ((dr_idx & 7) << 3) | (gpr_idx & 7);
    buf.bytes.push(modrm);
}

/// Encode descriptor-table load instructions: `lgdt [base + disp]` or `lidt [base + disp]`.
///
/// Both lgdt and lidt follow the same encoding pattern:
/// - Opcode: 0F 01 /2 (lgdt) or 0F 01 /3 (lidt)
/// - Operand: memory address [base + disp]
///
/// # Arguments
/// - `buf`: code buffer to append instructions to
/// - `base_reg`: base register ID (0-15 for GPRs)
/// - `disp`: displacement from base (-2^31..2^31-1)
/// - `reg_digit`: 2 for lgdt, 3 for lidt (the /digit field in ModR/M.reg)
pub fn encode_descriptor_table_load(
    buf: &mut CodeBuffer,
    base_reg: Reg64,
    disp: i32,
    reg_digit: u8,
) {
    let base_id = base_reg as u8;

    // Emit two-byte opcode (no REX prefix needed for this instruction)
    buf.bytes.push(0x0F);
    buf.bytes.push(0x01);

    // Encode displacement and ModR/M byte
    if disp == 0 {
        // Use mod=00, no displacement
        let modrm = ((reg_digit & 7) << 3) | (base_id & 7);
        buf.bytes.push(modrm);
    } else if (-128..=127).contains(&disp) {
        // Use mod=01, disp8
        let modrm = 0x40 | ((reg_digit & 7) << 3) | (base_id & 7);
        buf.bytes.push(modrm);
        buf.bytes.push(disp as u8);
    } else {
        // Use mod=10, disp32
        let modrm = 0x80 | ((reg_digit & 7) << 3) | (base_id & 7);
        buf.bytes.push(modrm);
        buf.bytes.extend(disp.to_le_bytes());
    }
}

/// Encode repeat store quadword instruction: `rep stosq` (no operands).
///
/// Stores RAX to memory at [RDI], then decrements RCX and repeats until RCX is zero.
/// Used primarily for .bss section zeroing with RAX=0, RCX=size in quadwords, RDI=base.
///
/// # Instructions
/// - `rep stosq`: `F3 48 AB` (3 bytes: rep prefix, REX.W, stosq opcode)
///
/// # Arguments
/// - `buf`: code buffer to append instruction to
pub fn encode_rep_stosq(buf: &mut CodeBuffer) {
    buf.bytes.push(0xF3); // rep prefix
    buf.bytes.push(0x48); // REX.W for 64-bit operand
    buf.bytes.push(0xAB); // stosq opcode
}

/// Encode `jmp far [mem]` with SIB or RIP-relative addressing.
///
/// Far jump to memory uses `FF /5` with REX.W prefix (`48`).
/// Instruction: `REX.W FF [ModR/M] [SIB] [disp]`
///
/// For SIB form `[base]` with disp:
/// - disp=0: ModR/M = 00_101_base_id (where base_id is the low 3 bits of base register)
/// - disp8: ModR/M = 01_101_base_id + disp8
/// - disp32: ModR/M = 10_101_base_id + disp32_le
///
/// For RIP-relative form `[rip + disp32]`:
/// - ModR/M = 00_101_101 (0x2D) + disp32_le
pub fn encode_far_jmp(buf: &mut CodeBuffer, base: Option<Reg64>, disp: i32) {
    let reg_field = 5u8; // /5 for far jmp

    if let Some(base_reg) = base {
        // SIB form: [base + disp]
        let base_id = base_reg as u8;
        let base_b = (base_id >> 3) != 0; // REX.B bit

        buf.bytes.push(rex(true, false, false, base_b)); // REX.W
        buf.bytes.push(0xFF);

        if disp == 0 {
            // mod=00: [base]
            buf.bytes.push((reg_field << 3) | (base_id & 7));
        } else if (-128..=127).contains(&disp) {
            // mod=01: [base + disp8]
            buf.bytes.push(0x40 | (reg_field << 3) | (base_id & 7));
            buf.bytes.push(disp as u8);
        } else {
            // mod=10: [base + disp32]
            buf.bytes.push(0x80 | (reg_field << 3) | (base_id & 7));
            buf.bytes.extend(disp.to_le_bytes());
        }
    } else {
        // RIP-relative form: [rip + disp32]
        // ModR/M.rm = 101 (5) signals RIP-relative in 64-bit mode with mod=00
        buf.bytes.push(rex_w());
        buf.bytes.push(0xFF);
        buf.bytes.push((reg_field << 3) | 5); // ModR/M with rm=5 for RIP-relative
        buf.bytes.extend(disp.to_le_bytes());
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

    // ── Borrowed references codegen tests (m4-006) ──────────────
    // At the x86_64 byte level, &T, &mut T, and *T are identical pointers.
    // These tests verify that all three reference forms encode identically.

    #[test]
    fn emit_indexed_load_for_ref_uses_same_sib_form_as_ptr() {
        let mut buf = CodeBuffer::new();
        // Simulate loading from a borrowed reference (&T); logically a pointer at codegen
        emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false);
        // mov rax, [rdi + rcx * 8] — identical to *T encoding
        // REX.W 48, Opcode 8B, ModR/M 04, SIB 0xcf
        assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x04, 0xcf]);
    }

    #[test]
    fn emit_indexed_load_for_ref_mut_uses_same_sib_form_as_ptr() {
        let mut buf = CodeBuffer::new();
        // Simulate loading from a mutable borrowed reference (&mut T); also a pointer at codegen
        emit_indexed_load(&mut buf, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false);
        // mov rax, [rdi + rcx * 8] — identical to *T and &T encoding
        // REX.W 48, Opcode 8B, ModR/M 04, SIB 0xcf
        assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x04, 0xcf]);
    }

    #[test]
    fn emit_indexed_load_ref_byte_sequence_matches_ptr_byte_sequence() {
        let mut buf_ptr = CodeBuffer::new();
        let mut buf_ref = CodeBuffer::new();
        let mut buf_mut_ref = CodeBuffer::new();

        // Emit identical width=8 indexed loads via all three conceptual forms
        emit_indexed_load(&mut buf_ptr, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false); // *T
        emit_indexed_load(&mut buf_ref, Reg64::Rax, Reg64::Rdi, Reg64::Rcx, 8, false); // &T
        emit_indexed_load(
            &mut buf_mut_ref,
            Reg64::Rax,
            Reg64::Rdi,
            Reg64::Rcx,
            8,
            false,
        ); // &mut T

        // All three must produce identical byte sequences per m4-006
        assert_eq!(buf_ptr.as_slice(), buf_ref.as_slice());
        assert_eq!(buf_ref.as_slice(), buf_mut_ref.as_slice());

        // Verify the exact byte sequence: 48 8b 04 cf
        assert_eq!(buf_ptr.as_slice(), &[0x48, 0x8b, 0x04, 0xcf]);
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

    #[test]
    fn emit_sub_rax_rdi_byte_sequence_is_48_29_f8() {
        let mut buf = CodeBuffer::new();
        sub_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rdi);
        // sub rax, rdi
        // REX.W=1: 0x48
        // Opcode: 0x29
        // ModR/M: 0xC0 | (7<<3) | 0 = 0xf8 (RDI is id 7, RAX is id 0)
        assert_eq!(buf.as_slice(), &[0x48, 0x29, 0xf8]);
    }

    #[test]
    fn emit_sar_rax_3_byte_sequence_is_48_c1_f8_03() {
        let mut buf = CodeBuffer::new();
        sar_reg64_imm8(&mut buf, Reg64::Rax, 3);
        // sar rax, 3
        // REX.W=1: 0x48
        // Opcode: 0xC1
        // ModR/M: 0xF8 | (0 & 7) = 0xf8 (RAX is id 0)
        // Immediate: 0x03
        assert_eq!(buf.as_slice(), &[0x48, 0xc1, 0xf8, 0x03]);
    }

    #[test]
    fn emit_ptr_sub_helper_u8_byte_case_skips_shift() {
        let mut buf = CodeBuffer::new();
        // For u8 (width 1), ptr_sub returns the raw difference (no shift).
        // Simulate: sub rax, rdi (no sar)
        sub_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rdi);
        assert_eq!(buf.as_slice(), &[0x48, 0x29, 0xf8]);
        // Width 1 requires no shift; the byte count is element count.
    }

    // ── Tightened encoding tests ────────────────────────────────

    #[test]
    fn add_reg64_imm8_small_value() {
        let mut buf = CodeBuffer::new();
        add_reg64_imm8(&mut buf, Reg64::Rax, 5);
        // REX.W=1: 0x48
        // Opcode: 0x83 (immediate-to-reg with sign-extended imm8)
        // ModR/M: 0xC0 | 0 = 0xc0 (RAX is id 0, /0 for add)
        // Immediate: 0x05
        assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xc0, 0x05]);
    }

    #[test]
    fn add_reg64_imm8_negative_value() {
        let mut buf = CodeBuffer::new();
        add_reg64_imm8(&mut buf, Reg64::Rcx, -10);
        // REX.W=1: 0x48
        // Opcode: 0x83
        // ModR/M: 0xC0 | 1 = 0xc1 (RCX is id 1, /0 for add)
        // Immediate: -10 as u8 = 0xf6
        assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xc1, 0xf6]);
    }

    #[test]
    fn add_reg64_imm32_fitting_value() {
        let mut buf = CodeBuffer::new();
        add_reg64_imm32(&mut buf, Reg64::Rax, 0x1234);
        // REX.W=1: 0x48
        // Opcode: 0x81 (immediate-to-reg with imm32)
        // ModR/M: 0xC0 | 0 = 0xc0 (RAX is id 0, /0 for add)
        // Immediate: 0x1234 in little-endian = 0x34, 0x12, 0x00, 0x00
        assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xc0, 0x34, 0x12, 0x00, 0x00]);
    }

    #[test]
    fn add_reg64_imm32_with_high_register() {
        let mut buf = CodeBuffer::new();
        add_reg64_imm32(&mut buf, Reg64::R12, 0x5000);
        // REX.W=1, B=1 (R12 is id 12 > 7): 0x49
        // Opcode: 0x81
        // ModR/M: 0xC0 | 4 = 0xc4 (R12 is id 12, id&7=4, /0 for add)
        // Immediate: 0x5000 in little-endian = 0x00, 0x50, 0x00, 0x00
        assert_eq!(buf.as_slice(), &[0x49, 0x81, 0xc4, 0x00, 0x50, 0x00, 0x00]);
    }

    #[test]
    fn jcc_rel8_within_range() {
        let mut buf = CodeBuffer::new();
        jcc_rel8(&mut buf, Cond::Eq, 50);
        // Opcode for JE rel8: 0x74
        // Displacement: 0x32 (50 in decimal)
        assert_eq!(buf.as_slice(), &[0x74, 0x32]);
    }

    #[test]
    fn jcc_rel8_negative_displacement() {
        let mut buf = CodeBuffer::new();
        jcc_rel8(&mut buf, Cond::Neq, -10);
        // Opcode for JNE rel8: 0x75
        // Displacement: -10 as u8 = 0xf6
        assert_eq!(buf.as_slice(), &[0x75, 0xf6]);
    }

    #[test]
    fn jcc_rel8_boundary_values() {
        let mut buf = CodeBuffer::new();
        jcc_rel8(&mut buf, Cond::Lt, 127);
        assert_eq!(buf.as_slice(), &[0x7c, 0x7f]);

        buf.bytes.clear();
        jcc_rel8(&mut buf, Cond::Ge, -128);
        assert_eq!(buf.as_slice(), &[0x7d, 0x80]);
    }

    #[test]
    fn jcc_rel8_all_conditions() {
        // Test all condition codes map to correct rel8 opcodes
        let test_cases = vec![
            (Cond::Eq, 0x74),
            (Cond::Neq, 0x75),
            (Cond::Lt, 0x7C),
            (Cond::Ge, 0x7D),
            (Cond::Le, 0x7E),
            (Cond::Gt, 0x7F),
        ];

        for (cond, expected_opcode) in test_cases {
            let mut buf = CodeBuffer::new();
            jcc_rel8(&mut buf, cond, 5);
            assert_eq!(buf.as_slice()[0], expected_opcode, "cond: {:?}", cond);
            assert_eq!(buf.as_slice()[1], 0x05);
        }
    }

    // ── Record construction tests ───────────────────────────────

    #[test]
    fn emit_field_access_offset_8_emits_48_8b_47_08() {
        let mut buf = CodeBuffer::new();
        emit_field_access(&mut buf, Reg64::Rax, Reg64::Rdi, 8, 8);
        // mov rax, [rdi + 8]
        // REX.W=1: 0x48
        // Opcode: 0x8B
        // ModR/M: 0x40 | (0<<3) | 7 = 0x47 (RAX reg=0, RDI rm=7, mod=01 for disp8)
        // disp8: 0x08
        assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x47, 0x08]);
    }

    #[test]
    fn emit_field_access_offset_0_uses_mod_00_no_disp() {
        let mut buf = CodeBuffer::new();
        emit_field_access(&mut buf, Reg64::Rax, Reg64::Rdi, 0, 8);
        // mov rax, [rdi + 0]
        // REX.W=1: 0x48
        // Opcode: 0x8B
        // ModR/M: 0x40 | (0<<3) | 7 = 0x47 (with disp8=0 for offset 0)
        // disp8: 0x00
        assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x47, 0x00]);
    }

    #[test]
    fn emit_field_access_large_offset_uses_disp32() {
        let mut buf = CodeBuffer::new();
        emit_field_access(&mut buf, Reg64::Rsi, Reg64::Rbx, 1000, 8);
        // mov rsi, [rbx + 1000]
        // REX.W=1: 0x48
        // Opcode: 0x8B
        // ModR/M: 0x80 | (6<<3) | 3 = 0xb3 (RSI reg=6, RBX rm=3, mod=10 for disp32)
        // disp32: 1000 = 0xe8 0x03 0x00 0x00
        assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0xb3, 0xe8, 0x03, 0x00, 0x00]);
    }

    #[test]
    fn emit_record_cons_emits_n_stores() {
        let mut buf = CodeBuffer::new();
        // Emit two field stores: offset 0 with RSI, offset 8 with RDX
        let field_stores = &[(0, Reg64::Rsi, 8), (8, Reg64::Rdx, 8)];
        emit_record_cons(&mut buf, Reg64::Rdi, field_stores);

        // First store: mov [rdi + 0], rsi
        // REX.W: 0x48
        // Opcode: 0x89
        // ModR/M: 0x40 | (6<<3) | 7 = 0x77
        // disp8: 0x00
        // Expected: 48 89 77 00

        // Second store: mov [rdi + 8], rdx
        // REX.W: 0x48
        // Opcode: 0x89
        // ModR/M: 0x40 | (2<<3) | 7 = 0x57
        // disp8: 0x08
        // Expected: 48 89 57 08

        assert_eq!(
            buf.as_slice(),
            &[0x48, 0x89, 0x77, 0x00, 0x48, 0x89, 0x57, 0x08]
        );
    }

    // ── Enum construction tests ─────────────────────────────────

    #[test]
    fn emit_enum_cons_emits_discriminant_then_payload_stores() {
        let mut buf = CodeBuffer::new();
        // Enum with discriminant 2, one payload field at offset 8 in RSI
        let payload_stores = &[(8, Reg64::Rsi, 8)];
        emit_enum_cons(&mut buf, Reg64::Rdi, 2, payload_stores);

        // Expected:
        // 1. mov rax, 2 (discriminant)
        //    REX.W: 0x48, opcode: 0xb8, imm64: 0x02 0x00 0x00 0x00 0x00 0x00 0x00 0x00
        // 2. mov [rdi + 0], rax
        //    REX.W: 0x48, opcode: 0x89, ModR/M: 0x07 (mod=00, reg=0, rm=7)
        // 3. mov [rdi + 8], rsi
        //    REX.W: 0x48, opcode: 0x89, ModR/M: 0x77, disp8: 0x08

        let expected = [
            0x48, 0xb8, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // mov rax, 2
            0x48, 0x89, 0x07, // mov [rdi], rax
            0x48, 0x89, 0x77, 0x08, // mov [rdi + 8], rsi
        ];
        assert_eq!(buf.as_slice(), &expected);
    }

    #[test]
    fn emit_enum_discriminant_loads_offset_0() {
        let mut buf = CodeBuffer::new();
        emit_enum_discriminant(&mut buf, Reg64::Rcx, Reg64::Rsi);
        // mov rcx, [rsi + 0]
        // REX.W: 0x48
        // Opcode: 0x8B
        // ModR/M: 0x40 | (1<<3) | 6 = 0x4e (RCX reg=1, RSI rm=6, mod=01 disp8)
        // disp8: 0x00
        assert_eq!(buf.as_slice(), &[0x48, 0x8b, 0x4e, 0x00]);
    }

    #[test]
    fn emit_match_arm_branch_compares_then_jccs() {
        let mut buf = CodeBuffer::new();
        let patch_offset = emit_match_arm_branch(&mut buf, Reg64::Rax, 3, Cond::Neq);

        // Expected:
        // cmp rax, 3
        //   REX.W: 0x48, opcode: 0x81, ModR/M: 0xf8, imm32: 0x03 0x00 0x00 0x00
        // jne rel32
        //   opcode: 0x0f, 0x85, rel32: 0x00 0x00 0x00 0x00 (placeholder)

        let bytes = buf.as_slice();
        assert_eq!(bytes[0], 0x48); // REX.W
        assert_eq!(bytes[1], 0x81); // cmp opcode
        assert_eq!(bytes[2], 0xf8); // ModR/M
        assert_eq!(bytes[3..7], [0x03, 0x00, 0x00, 0x00]); // imm32
        assert_eq!(bytes[7], 0x0f); // jcc opcode high
        assert_eq!(bytes[8], 0x85); // jne condition
        // patch_offset should point to the rel32 bytes (offset 9)
        assert_eq!(patch_offset, 9);
    }

    #[test]
    fn emit_record_cons_with_8byte_fields_alignment_correct() {
        let mut buf = CodeBuffer::new();
        // Record with 3 fields at natural 8-byte boundaries
        let field_stores = &[
            (0, Reg64::Rsi, 8),  // field 0 @ offset 0
            (8, Reg64::Rdx, 8),  // field 1 @ offset 8
            (16, Reg64::Rcx, 8), // field 2 @ offset 16
        ];
        emit_record_cons(&mut buf, Reg64::Rdi, field_stores);

        // Should emit 3 stores, each 4 bytes (REX.W + opcode + modrm + disp8)
        assert_eq!(buf.len(), 12);

        // Verify first store: mov [rdi + 0], rsi
        assert_eq!(&buf.as_slice()[0..4], &[0x48, 0x89, 0x77, 0x00]);
        // Verify second store: mov [rdi + 8], rdx
        assert_eq!(&buf.as_slice()[4..8], &[0x48, 0x89, 0x57, 0x08]);
        // Verify third store: mov [rdi + 16], rcx
        assert_eq!(&buf.as_slice()[8..12], &[0x48, 0x89, 0x4f, 0x10]);
    }

    // ── Zero-operand instruction tests (phase-5 m2-002) ──────────

    #[test]
    fn encode_zero_operand_nop_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_zero_operand(&mut buf, 0x90); // NOP

        assert_eq!(buf.as_slice(), &[0x90]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Nop);
    }

    #[test]
    fn encode_zero_operand_hlt_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_zero_operand(&mut buf, 0xF4); // HLT

        assert_eq!(buf.as_slice(), &[0xF4]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Hlt);
    }

    #[test]
    fn encode_zero_operand_cli_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_zero_operand(&mut buf, 0xFA); // CLI

        assert_eq!(buf.as_slice(), &[0xFA]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cli);
    }

    #[test]
    fn encode_zero_operand_sti_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_zero_operand(&mut buf, 0xFB); // STI

        assert_eq!(buf.as_slice(), &[0xFB]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Sti);
    }

    #[test]
    fn encode_zero_operand_swapgs_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_zero_operand(&mut buf, 0x81); // SWAPGS (sentinel)

        assert_eq!(buf.as_slice(), &[0x0F, 0x01, 0xF8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Swapgs);
    }

    #[test]
    fn encode_zero_operand_cpuid_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_zero_operand(&mut buf, 0x82); // CPUID (sentinel)

        assert_eq!(buf.as_slice(), &[0x0F, 0xA2]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cpuid);
    }

    // ── I/O port instruction tests (phase-5 m2-003) ──────────────

    #[test]
    fn encode_in_dx_width_1_emits_ec() {
        let mut buf = CodeBuffer::new();
        encode_in_dx(&mut buf, 1);
        // in al, dx: EC
        assert_eq!(buf.as_slice(), &[0xEC]);
    }

    #[test]
    fn encode_in_dx_width_1_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_in_dx(&mut buf, 1);

        assert_eq!(buf.as_slice(), &[0xEC]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_in_dx_width_2_emits_66_ed() {
        let mut buf = CodeBuffer::new();
        encode_in_dx(&mut buf, 2);
        // in ax, dx: 66 ED
        assert_eq!(buf.as_slice(), &[0x66, 0xED]);
    }

    #[test]
    fn encode_in_dx_width_2_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_in_dx(&mut buf, 2);

        assert_eq!(buf.as_slice(), &[0x66, 0xED]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_in_dx_width_4_emits_ed() {
        let mut buf = CodeBuffer::new();
        encode_in_dx(&mut buf, 4);
        // in eax, dx: ED
        assert_eq!(buf.as_slice(), &[0xED]);
    }

    #[test]
    fn encode_in_dx_width_4_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_in_dx(&mut buf, 4);

        assert_eq!(buf.as_slice(), &[0xED]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::In);
    }

    #[test]
    fn encode_out_dx_width_1_emits_ee() {
        let mut buf = CodeBuffer::new();
        encode_out_dx(&mut buf, 1);
        // out dx, al: EE
        assert_eq!(buf.as_slice(), &[0xEE]);
    }

    #[test]
    fn encode_out_dx_width_1_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_out_dx(&mut buf, 1);

        assert_eq!(buf.as_slice(), &[0xEE]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    #[test]
    fn encode_out_dx_width_2_emits_66_ef() {
        let mut buf = CodeBuffer::new();
        encode_out_dx(&mut buf, 2);
        // out dx, ax: 66 EF
        assert_eq!(buf.as_slice(), &[0x66, 0xEF]);
    }

    #[test]
    fn encode_out_dx_width_2_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_out_dx(&mut buf, 2);

        assert_eq!(buf.as_slice(), &[0x66, 0xEF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    #[test]
    fn encode_out_dx_width_4_emits_ef() {
        let mut buf = CodeBuffer::new();
        encode_out_dx(&mut buf, 4);
        // out dx, eax: EF
        assert_eq!(buf.as_slice(), &[0xEF]);
    }

    #[test]
    fn encode_out_dx_width_4_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_out_dx(&mut buf, 4);

        assert_eq!(buf.as_slice(), &[0xEF]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Out);
    }

    // ── Phase-5 m2-005: control register MOV instructions ─────────

    // Write (mov cr_idx, rax) tests: 6 tests covering CR0, CR2, CR3, CR4, CR8 with rax
    // and round-trip verification via iced-x86

    #[test]
    fn encode_mov_cr0_rax_emits_0f22c0() {
        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, true, 0, 0); // write=true, cr_idx=0, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xC0]);
    }

    #[test]
    fn encode_mov_cr0_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, true, 0, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_cr3_rax_emits_0f22d8() {
        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, true, 3, 0); // write=true, cr_idx=3, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xD8]);
    }

    #[test]
    fn encode_mov_cr3_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, true, 3, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xD8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_cr4_rax_emits_0f22e0() {
        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, true, 4, 0); // write=true, cr_idx=4, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xE0]);
    }

    #[test]
    fn encode_mov_cr4_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, true, 4, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x22, 0xE0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_cr8_rax_emits_440f22c0() {
        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, true, 8, 0); // write=true, cr_idx=8, gpr_idx=0 (rax)
        // CR8 requires REX.R=1: 0x44, then 0x0F 0x22, then 0xC0 (reg=0, r/m=0)
        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x22, 0xC0]);
    }

    #[test]
    fn encode_mov_cr8_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, true, 8, 0);
        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x22, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    // Read (mov rax, cr_idx) tests: 6 tests covering CR0, CR2, CR3, CR4, CR8 from rax
    // and round-trip verification via iced-x86

    #[test]
    fn encode_mov_rax_cr0_emits_0f20c0() {
        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, false, 0, 0); // write=false, cr_idx=0, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xC0]);
    }

    #[test]
    fn encode_mov_rax_cr0_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, false, 0, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_rax_cr3_emits_0f20d8() {
        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, false, 3, 0); // write=false, cr_idx=3, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xD8]);
    }

    #[test]
    fn encode_mov_rax_cr3_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, false, 3, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xD8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_rax_cr4_emits_0f20e0() {
        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, false, 4, 0); // write=false, cr_idx=4, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xE0]);
    }

    #[test]
    fn encode_mov_rax_cr4_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, false, 4, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x20, 0xE0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_rax_cr8_emits_440f20c0() {
        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, false, 8, 0); // write=false, cr_idx=8, gpr_idx=0 (rax)
        // CR8 requires REX.R=1: 0x44, then 0x0F 0x20, then 0xC0 (reg=0, r/m=0)
        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x20, 0xC0]);
    }

    #[test]
    fn encode_mov_rax_cr8_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_cr(&mut buf, false, 8, 0);
        assert_eq!(buf.as_slice(), &[0x44, 0x0F, 0x20, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    // Write (mov dr_idx, rax) tests: 4 tests covering DR0, DR2, DR6, DR7 to rax
    // and round-trip verification via iced-x86

    #[test]
    fn encode_mov_dr0_rax_emits_0f23c0() {
        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, true, 0, 0); // write=true, dr_idx=0, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xC0]);
    }

    #[test]
    fn encode_mov_dr0_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, true, 0, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_dr2_rax_emits_0f23d0() {
        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, true, 2, 0); // write=true, dr_idx=2, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xD0]);
    }

    #[test]
    fn encode_mov_dr2_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, true, 2, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xD0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_dr6_rax_emits_0f23f0() {
        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, true, 6, 0); // write=true, dr_idx=6, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF0]);
    }

    #[test]
    fn encode_mov_dr6_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, true, 6, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_dr7_rax_emits_0f23f8() {
        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, true, 7, 0); // write=true, dr_idx=7, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF8]);
    }

    #[test]
    fn encode_mov_dr7_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, true, 7, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x23, 0xF8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    // Read (mov rax, dr_idx) tests: 4 tests covering RAX from DR0, DR2, DR6, DR7
    // and round-trip verification via iced-x86

    #[test]
    fn encode_mov_rax_dr0_emits_0f21c0() {
        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, false, 0, 0); // write=false, dr_idx=0, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);
    }

    #[test]
    fn encode_mov_rax_dr0_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, false, 0, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xC0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_rax_dr2_emits_0f21d0() {
        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, false, 2, 0); // write=false, dr_idx=2, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xD0]);
    }

    #[test]
    fn encode_mov_rax_dr2_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, false, 2, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xD0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_rax_dr6_emits_0f21f0() {
        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, false, 6, 0); // write=false, dr_idx=6, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF0]);
    }

    #[test]
    fn encode_mov_rax_dr6_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, false, 6, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF0]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    #[test]
    fn encode_mov_rax_dr7_emits_0f21f8() {
        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, false, 7, 0); // write=false, dr_idx=7, gpr_idx=0 (rax)
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF8]);
    }

    #[test]
    fn encode_mov_rax_dr7_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        encode_mov_dr(&mut buf, false, 7, 0);
        assert_eq!(buf.as_slice(), &[0x0F, 0x21, 0xF8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Mov);
    }

    // ── CMP round-trip tests (phase 6 m4-001) ───────────────────

    #[test]
    fn cmp_rax_rdi_emits_4839f8() {
        let mut buf = CodeBuffer::new();
        cmp_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rdi);
        // REX.W=1, opcode 39, ModR/M: 0xC0 | (7<<3) | 0 = 0xf8
        assert_eq!(buf.as_slice(), &[0x48, 0x39, 0xf8]);
    }

    #[test]
    fn cmp_rax_rdi_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        cmp_reg64_reg64(&mut buf, Reg64::Rax, Reg64::Rdi);
        assert_eq!(buf.as_slice(), &[0x48, 0x39, 0xf8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
    }

    #[test]
    fn cmp_mem_rdi_24_rcx_emits_48394f18() {
        let mut buf = CodeBuffer::new();
        cmp_mem_reg64_reg64(&mut buf, Reg64::Rdi, 24, Reg64::Rcx);
        // REX.W=1, opcode 39, ModR/M with disp8: 0x40 | (1<<3) | 7 = 0x4f, disp8=24=0x18
        assert_eq!(buf.as_slice(), &[0x48, 0x39, 0x4f, 0x18]);
    }

    #[test]
    fn cmp_mem_rdi_24_rcx_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        cmp_mem_reg64_reg64(&mut buf, Reg64::Rdi, 24, Reg64::Rcx);
        assert_eq!(buf.as_slice(), &[0x48, 0x39, 0x4f, 0x18]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
    }

    #[test]
    fn cmp_rax_imm8_0_emits_4883f800() {
        let mut buf = CodeBuffer::new();
        cmp_reg64_imm8(&mut buf, Reg64::Rax, 0);
        // REX.W=1, opcode 83, ModR/M: 0xF8 | 0 = 0xf8, imm8=0
        assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xf8, 0x00]);
    }

    #[test]
    fn cmp_rax_imm8_0_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        cmp_reg64_imm8(&mut buf, Reg64::Rax, 0);
        assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xf8, 0x00]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
    }

    #[test]
    fn cmp_rcx_imm8_127_emits_4883f97f() {
        let mut buf = CodeBuffer::new();
        cmp_reg64_imm8(&mut buf, Reg64::Rcx, 127);
        // REX.W=1, opcode 83, ModR/M: 0xF8 | 1 = 0xf9, imm8=127
        assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xf9, 0x7f]);
    }

    #[test]
    fn cmp_rcx_imm8_127_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        cmp_reg64_imm8(&mut buf, Reg64::Rcx, 127);
        assert_eq!(buf.as_slice(), &[0x48, 0x83, 0xf9, 0x7f]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
    }

    #[test]
    fn cmp_rdx_imm32_256_emits_4881fa00010000() {
        let mut buf = CodeBuffer::new();
        cmp_reg64_imm32(&mut buf, Reg64::Rdx, 256);
        // REX.W=1, opcode 81, ModR/M: 0xF8 | 2 = 0xfa, imm32=256 in LE
        assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xfa, 0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn cmp_rdx_imm32_256_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        cmp_reg64_imm32(&mut buf, Reg64::Rdx, 256);
        assert_eq!(buf.as_slice(), &[0x48, 0x81, 0xfa, 0x00, 0x01, 0x00, 0x00]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
    }

    #[test]
    fn cmp_r8_imm8_neg1_emits_4983f8ff() {
        let mut buf = CodeBuffer::new();
        cmp_reg64_imm8(&mut buf, Reg64::R8, -1);
        // REX.W=1, B=1 (R8 is id 8 > 7): 0x49, opcode 83, ModR/M: 0xF8 | 0 = 0xf8, imm8=-1=0xff
        assert_eq!(buf.as_slice(), &[0x49, 0x83, 0xf8, 0xff]);
    }

    #[test]
    fn cmp_r8_imm8_neg1_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        cmp_reg64_imm8(&mut buf, Reg64::R8, -1);
        assert_eq!(buf.as_slice(), &[0x49, 0x83, 0xf8, 0xff]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
    }

    #[test]
    fn cmp_r15_imm32_0x7fffffff_emits_49811fff_ffffffff7f() {
        let mut buf = CodeBuffer::new();
        cmp_reg64_imm32(&mut buf, Reg64::R15, 0x7fffffff);
        // REX.W=1, R=1 (R15 is id 15 > 7): 0x4d, opcode 81, ModR/M: 0xF8 | 7 = 0xff, imm32=0x7fffffff in LE
        assert_eq!(buf.as_slice(), &[0x49, 0x81, 0xff, 0xff, 0xff, 0xff, 0x7f]);
    }

    #[test]
    fn cmp_r15_imm32_0x7fffffff_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        cmp_reg64_imm32(&mut buf, Reg64::R15, 0x7fffffff);
        assert_eq!(buf.as_slice(), &[0x49, 0x81, 0xff, 0xff, 0xff, 0xff, 0x7f]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
    }

    #[test]
    fn cmp_mem_rbp_minus_8_rax_emits_4839455f8() {
        let mut buf = CodeBuffer::new();
        cmp_mem_reg64_reg64(&mut buf, Reg64::Rbp, -8, Reg64::Rax);
        // REX.W=1, opcode 39, ModR/M with disp8: 0x40 | (0<<3) | 5 = 0x45, disp8=-8=0xf8
        assert_eq!(buf.as_slice(), &[0x48, 0x39, 0x45, 0xf8]);
    }

    #[test]
    fn cmp_mem_rbp_minus_8_rax_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        cmp_mem_reg64_reg64(&mut buf, Reg64::Rbp, -8, Reg64::Rax);
        assert_eq!(buf.as_slice(), &[0x48, 0x39, 0x45, 0xf8]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
    }

    #[test]
    fn cmp_mem_rsi_1000_r9_emits_48399c06e8030000() {
        let mut buf = CodeBuffer::new();
        cmp_mem_reg64_reg64(&mut buf, Reg64::Rsi, 1000, Reg64::R9);
        // REX.W=1, R=1 (R9 is id 9 > 7): 0x4c, opcode 39, ModR/M with disp32: 0x80 | (1<<3) | 6 = 0x8e, disp32=1000 in LE
        assert_eq!(buf.as_slice(), &[0x4c, 0x39, 0x8e, 0xe8, 0x03, 0x00, 0x00]);
    }

    #[test]
    fn cmp_mem_rsi_1000_r9_round_trips_through_iced_x86() {
        use iced_x86::{Decoder, DecoderOptions, Mnemonic as IcedMnem};

        let mut buf = CodeBuffer::new();
        cmp_mem_reg64_reg64(&mut buf, Reg64::Rsi, 1000, Reg64::R9);
        assert_eq!(buf.as_slice(), &[0x4c, 0x39, 0x8e, 0xe8, 0x03, 0x00, 0x00]);

        let mut decoder = Decoder::new(64, buf.as_slice(), DecoderOptions::NONE);
        let instr = decoder.decode();
        assert_eq!(instr.mnemonic(), IcedMnem::Cmp);
    }
}
