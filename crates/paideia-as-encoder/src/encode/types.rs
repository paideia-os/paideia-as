//! Shared types and internal helpers for the encoder.
//!
//! Every encoder sub-module (`mov_arith`, `bit_ops`, …) imports this module via
//! `use super::types::*;` to pick up:
//! - the public register / condition types (`Reg64`, `Reg32`, `Cond`) and the
//!   [`CodeBuffer`] output target, and
//! - the crate-private REX / ModR/M helpers (`rex`, `rex_w`,
//!   `emit_mem_base_disp`, `emit_mem_sib_disp`) that every encoder needs.
//!
//! The public types are re-exported from `encode/mod.rs`; the helpers are not
//! part of the crate's public API but do have `pub(crate)` visibility so
//! sibling modules (and `encode_instruction::cmp_test`, which reads
//! `emit_mem_base_disp`) can call them.

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
/// Registers 0-7 use their legacy names (EAX, ECX, etc.); registers 8-15 are
/// the extended registers R8D through R15D (REX.B prefix required).
/// Phase-1 uses 64-bit instructions as the primary case; 32-bit forms are a follow-up.
/// Phase R15 PA-R15-001 (issue #956): added extended registers R8D–R15D.
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
    /// R8D (register id 8)
    R8d = 8,
    /// R9D (register id 9)
    R9d = 9,
    /// R10D (register id 10)
    R10d = 10,
    /// R11D (register id 11)
    R11d = 11,
    /// R12D (register id 12)
    R12d = 12,
    /// R13D (register id 13)
    R13d = 13,
    /// R14D (register id 14)
    R14d = 14,
    /// R15D (register id 15)
    R15d = 15,
}

/// Conditional jump condition codes (used in `0F 8X` two-byte opcodes).
///
/// The second byte of a two-byte jump is the opcode value below.
/// Note: JE/JZ, JNE/JNZ are aliases (same opcodes); the IR will map both to
/// the canonical encoder variants (Eq and Neq).
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
    /// JB (below, unsigned): `0F 82`
    Below = 0x82,
    /// JBE (below or equal, unsigned): `0F 86`
    BelowOrEqual = 0x86,
    /// JA (above, unsigned): `0F 87`
    Above = 0x87,
    /// JAE (above or equal, unsigned): `0F 83`
    AboveOrEqual = 0x83,
    /// JS (sign): `0F 88`
    Sign = 0x88,
    /// JNS (not sign): `0F 89`
    NotSign = 0x89,
    /// JO (overflow): `0F 80`
    Overflow = 0x80,
    /// JNO (not overflow): `0F 81`
    NotOverflow = 0x81,
    /// JP / JPE (parity / parity even): `0F 8A`
    Parity = 0x8A,
    /// JNP / JPO (not parity / parity odd): `0F 8B`
    NotParity = 0x8B,
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
pub(crate) fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | (u8::from(w) << 3) | (u8::from(r) << 2) | (u8::from(x) << 1) | u8::from(b)
}

// REX prefix for 64-bit operand (W=1, R=0, X=0, B=0).
pub(crate) fn rex_w() -> u8 {
    rex(true, false, false, false)
}

// Helper to emit ModR/M + SIB (if needed) + displacement for [base + disp].
// Handles SIB escape (when base is RSP) and BP escape (when base is RBP with disp=0).
//
// Arguments:
//   - reg_field: the value to encode in ModR/M.reg (already masked to 3 bits for use)
//   - base_id: full 8-bit register ID (will mask to 3 bits for ModR/M.r/m)
//   - disp: displacement value (0, signed i8, or signed i32)
pub(crate) fn emit_mem_base_disp(buf: &mut CodeBuffer, reg_field: u8, base_id: u8, disp: i32) {
    let base_low = base_id & 7;
    let reg_low = reg_field & 7;
    let sib_escape = base_low == 4; // base is RSP
    let bp_escape = base_low == 5; // base is RBP
    let (mod_bits, disp_len) = if disp == 0 && !bp_escape {
        (0x00u8, 0)
    } else if (-128..=127).contains(&disp) {
        (0x40u8, 1)
    } else {
        (0x80u8, 4)
    };
    if sib_escape {
        buf.bytes.push(mod_bits | (reg_low << 3) | 0b100);
        buf.bytes.push((0b00 << 6) | (0b100 << 3) | base_low);
    } else {
        buf.bytes.push(mod_bits | (reg_low << 3) | base_low);
    }
    match disp_len {
        0 => {}
        1 => buf.bytes.push(disp as u8),
        _ => buf.bytes.extend(disp.to_le_bytes()),
    }
}


/// Centralized SIB encoding helper for memory operands with scale, index, base, and displacement.
///
/// Handles the special case where RBP (id=5) or R13 (id=13) as base requires a disp8=0
/// escape even when disp=0 (since mod=00 with base=5 is reserved for RIP-relative).
///
/// # Arguments
/// - `buf`: code buffer to append bytes to
/// - `reg_field`: register field for ModR/M (3 bits, typically dst or src register)
/// - `base_id`: base register id (0-15; only low 3 bits used in SIB)
/// - `index_id`: index register id (0-15; only low 3 bits used in SIB)
/// - `scale_bits`: scale encoding (0=1x, 1=2x, 2=4x, 3=8x)
/// - `disp`: displacement value (0, disp8, or disp32)
///
/// Emits:
/// - ModR/M byte with scale-index-base indicator (rm=100)
/// - SIB byte
/// - Displacement bytes (0, 1, or 4 bytes) as needed
pub(crate) fn emit_mem_sib_disp(
    buf: &mut CodeBuffer,
    reg_field: u8,
    base_id: u8,
    index_id: u8,
    scale_bits: u8,
    disp: i32,
) {
    let base_low = base_id & 7;
    let bp_escape = base_low == 5;  // RBP / R13
    let (mod_bits, disp_len) = if disp == 0 && !bp_escape {
        (0x00u8, 0)
    } else if (-128..=127).contains(&disp) {
        (0x40u8, 1)
    } else {
        (0x80u8, 4)
    };
    buf.bytes.push(mod_bits | ((reg_field & 7) << 3) | 0b100);
    buf.bytes.push(((scale_bits & 3) << 6) | ((index_id & 7) << 3) | base_low);
    match disp_len {
        0 => {}
        1 => buf.bytes.push(disp as u8),
        _ => buf.bytes.extend(disp.to_le_bytes()),
    }
}
