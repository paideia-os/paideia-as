//! Shared encoder helpers: register-id conversions, condition-code
//! translation, segment-prefix detection, and small primitive predicates
//! used across the mnemonic-family sub-modules.
//!
//! Extracted from the former single-file `encode_instruction.rs`
//! (paideia-as#1400). All items are `pub(super)` — the sub-modules
//! reach them via `use super::*;`.

use super::*;

/// Whether a 64-bit ADD with the given operand can be shortened to 32-bit.
///
/// True when the high 32 bits are known to be zero/unused (e.g., the
/// 32-bit form clears the high bits implicitly).
pub(super) fn can_shorten_add_to_32bit(high_bits_used: bool) -> bool {
    !high_bits_used
}

/// Whether a signed displacement can be represented as a signed 8-bit
/// integer (rel8 short branches).
pub(super) fn can_use_rel8(displacement: i64) -> bool {
    (-128..=127).contains(&displacement)
}

/// Convert an IR register ID to an encoder Reg64.
pub(super) fn reg64_from(id: RegId) -> Result<Reg64, EncodeError> {
    match id.0 {
        0 => Ok(Reg64::Rax),
        1 => Ok(Reg64::Rcx),
        2 => Ok(Reg64::Rdx),
        3 => Ok(Reg64::Rbx),
        4 => Ok(Reg64::Rsp),
        5 => Ok(Reg64::Rbp),
        6 => Ok(Reg64::Rsi),
        7 => Ok(Reg64::Rdi),
        8 => Ok(Reg64::R8),
        9 => Ok(Reg64::R9),
        10 => Ok(Reg64::R10),
        11 => Ok(Reg64::R11),
        12 => Ok(Reg64::R12),
        13 => Ok(Reg64::R13),
        14 => Ok(Reg64::R14),
        15 => Ok(Reg64::R15),
        _ => Err(EncodeError::Unsupported("invalid register id")),
    }
}

pub(super) fn reg32_from(id: RegId) -> Result<Reg32, EncodeError> {
    match id.0 {
        0 => Ok(Reg32::Eax),
        1 => Ok(Reg32::Ecx),
        2 => Ok(Reg32::Edx),
        3 => Ok(Reg32::Ebx),
        4 => Ok(Reg32::Esp),
        5 => Ok(Reg32::Ebp),
        6 => Ok(Reg32::Esi),
        7 => Ok(Reg32::Edi),
        8 => Ok(Reg32::R8d),
        9 => Ok(Reg32::R9d),
        10 => Ok(Reg32::R10d),
        11 => Ok(Reg32::R11d),
        12 => Ok(Reg32::R12d),
        13 => Ok(Reg32::R13d),
        14 => Ok(Reg32::R14d),
        15 => Ok(Reg32::R15d),
        _ => Err(EncodeError::Unsupported("invalid register id")),
    }
}

/// Convert an IR Scale to a numeric byte width for indexed loads.
/// Convert an IR Cond to an encoder Cond.
pub(super) fn cond_from(ir_cond: IrCond) -> Result<Cond, EncodeError> {
    match ir_cond {
        IrCond::Eq => Ok(Cond::Eq),
        IrCond::Ne => Ok(Cond::Neq),
        IrCond::Lt => Ok(Cond::Lt),
        IrCond::Ge => Ok(Cond::Ge),
        IrCond::Le => Ok(Cond::Le),
        IrCond::Gt => Ok(Cond::Gt),
        IrCond::Below => Ok(Cond::Below),
        IrCond::BelowOrEqual => Ok(Cond::BelowOrEqual),
        IrCond::Above => Ok(Cond::Above),
        IrCond::AboveOrEqual => Ok(Cond::AboveOrEqual),
        IrCond::Zero => Ok(Cond::Eq),     // jz is alias for je (0x84)
        IrCond::NonZero => Ok(Cond::Neq), // jnz is alias for jne (0x85)
        IrCond::Sign => Ok(Cond::Sign),
        IrCond::NotSign => Ok(Cond::NotSign),
        IrCond::Overflow => Ok(Cond::Overflow),
        IrCond::NotOverflow => Ok(Cond::NotOverflow),
        IrCond::Parity => Ok(Cond::Parity),
        IrCond::NotParity => Ok(Cond::NotParity),
    }
}

/// PA-R13-002: Find and return the first MemSeg operand, if any.
pub(super) fn find_mem_seg(operands: &[Operand]) -> Option<(usize, paideia_as_ir::SegPrefix)> {
    for (i, op) in operands.iter().enumerate() {
        if let Operand::MemSeg { seg, .. } = op {
            return Some((i, *seg));
        }
    }
    None
}

/// Translate a `Scale` enum into the 2-bit SIB scale field.
///
/// The four SIB scale factors encode as 0, 1, 2, 3 for 1x, 2x, 4x, 8x
/// respectively (Intel SDM Vol. 2 §2.1.5). Kept as a small helper because
/// this three-line match repeats in every SIB-taking encoder arm; centralising
/// it lets the scale-to-bits mapping change in one place if a future extension
/// (e.g. AVX-512 x16 gather) ever needs a wider scale field.
#[inline]
pub(super) fn sib_scale_bits(scale: Scale) -> u8 {
    match scale {
        Scale::X1 => 0,
        Scale::X2 => 1,
        Scale::X4 => 2,
        Scale::X8 => 3,
    }
}

/// Resolve an 8-bit register ID to (masked_id, needs_rex) for use in setcc.
///
/// Returns:
/// - (id, false) for al-bl and r8b-r15b (standard low-byte registers)
/// - (id, true) for spl/bpl/sil/dil (high-byte regs requiring REX prefix)
/// - (id, false) for r8b-r15b (extended regs, REX.B handled by setcc_reg8)
pub(super) fn resolve_reg8(reg_id: RegId) -> (u8, bool) {
    match reg_id.0 {
        // Standard low-byte registers: al, cl, dl, bl (0-3)
        0..=3 => (reg_id.0 as u8, false),
        // spl/bpl/sil/dil (33-36) — mapped to 4-7 with needs_rex = true
        33 => (4, true),
        34 => (5, true),
        35 => (6, true),
        36 => (7, true),
        // Extended low-byte registers: r8b-r15b (8-15)
        8..=15 => (reg_id.0 as u8, false),
        // Anything else is invalid
        _ => (reg_id.0 as u8, false),
    }
}


// Helper to emit a REX prefix byte (copied from encode.rs for use in encode_lea).
pub(super) fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | (u8::from(w) << 3) | (u8::from(r) << 2) | (u8::from(x) << 1) | u8::from(b)
}
