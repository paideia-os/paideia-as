//! Operand-shape classifier for instruction dispatch (Phase 6, m1-001).
//!
//! This module provides the `DispatchKind` enum and `classify` function to
//! categorize x86_64 MOV instructions based on their operand shapes, enabling
//! m1-002 and later dispatch phases to route instructions to specialized
//! encoders or handlers.
//!
//! # Register Encoding (compact)
//!
//! The classifier uses compact register encoding:
//! - GPR (rax–r15): `Reg(0..16)`
//! - Control registers (cr0–cr8): `Reg(16..25)`
//! - Debug registers (dr0–dr7): `Reg(25..33)`
//!
//! # Examples
//!
//! ```ignore
//! use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};
//! use paideia_as_encoder::dispatch::{classify, DispatchKind};
//! use smallvec::SmallVec;
//!
//! // MOV cr3, rdi → MovToCr
//! let inst = Instruction {
//!     mnemonic: Mnemonic::Mov,
//!     operands: {
//!         let mut ops = SmallVec::new();
//!         ops.push(Operand::Reg(RegId(19)));  // cr3 = 16 + 3
//!         ops.push(Operand::Reg(RegId(7)));   // rdi = 7
//!         ops
//!     },
//!     encoding_hint: None,
//! };
//! assert_eq!(classify(&inst), DispatchKind::MovToCr);
//!
//! // MOV rdi, cr3 → MovFromCr
//! let inst = Instruction {
//!     mnemonic: Mnemonic::Mov,
//!     operands: {
//!         let mut ops = SmallVec::new();
//!         ops.push(Operand::Reg(RegId(7)));   // rdi = 7
//!         ops.push(Operand::Reg(RegId(19))); // cr3 = 16 + 3
//!         ops
//!     },
//!     encoding_hint: None,
//! };
//! assert_eq!(classify(&inst), DispatchKind::MovFromCr);
//!
//! // MOV rax, 42 → MovGeneric
//! let inst = Instruction {
//!     mnemonic: Mnemonic::Mov,
//!     operands: {
//!         let mut ops = SmallVec::new();
//!         ops.push(Operand::Reg(RegId(0)));   // rax = 0
//!         ops.push(Operand::Imm64(42));
//!         ops
//!     },
//!     encoding_hint: None,
//! };
//! assert_eq!(classify(&inst), DispatchKind::MovGeneric);
//! ```

use paideia_as_ir::instruction::{Instruction, Mnemonic, Operand, RegId};

/// Operand-shape classifier for instruction dispatch.
///
/// Categorizes x86_64 instructions based on operand shapes to route them to
/// specialized encoders or handlers in m1-002 and beyond.
///
/// For non-MOV mnemonics, always returns `Generic`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum DispatchKind {
    /// Generic MOV: register-to-register (GPR only), register-to-memory,
    /// memory-to-register, or immediate-to-register (non-CR/DR).
    MovGeneric,
    /// MOV to Control Register: destination is CR, source is GPR.
    ///
    /// Example: `MOV cr3, rdi`
    MovToCr,
    /// MOV from Control Register: source is CR, destination is GPR.
    ///
    /// Example: `MOV rdi, cr3`
    MovFromCr,
    /// MOV to Debug Register: destination is DR, source is GPR.
    ///
    /// Example: `MOV dr0, rax`
    MovToDr,
    /// MOV from Debug Register: source is DR, destination is GPR.
    ///
    /// Example: `MOV rax, dr0`
    MovFromDr,
    /// Non-MOV instruction: always route to generic dispatch.
    ///
    /// Covers all non-MOV mnemonics (Add, Sub, Cmp, Jcc, Jmp, etc.).
    Generic,
}

/// Test whether a register ID falls in the Control Register range (cr0–cr8).
///
/// Compact encoding: CR uses indices 16..25.
#[inline]
fn is_control_register(reg_id: RegId) -> bool {
    let id = reg_id.0;
    (16..25).contains(&id)
}

/// Test whether a register ID falls in the Debug Register range (dr0–dr7).
///
/// Compact encoding: DR uses indices 25..33.
#[inline]
fn is_debug_register(reg_id: RegId) -> bool {
    let id = reg_id.0;
    (25..33).contains(&id)
}

/// Test whether a register ID is a General-Purpose Register (rax–r15).
///
/// Compact encoding: GPR uses indices 0..16.
#[inline]
fn is_general_purpose_register(reg_id: RegId) -> bool {
    let id = reg_id.0;
    (0..16).contains(&id)
}

/// Classify an instruction by operand shape for dispatch.
///
/// Dispatches MOV instructions based on destination and source register types:
/// - If destination is CR and source is GPR: `MovToCr`
/// - If source is CR and destination is GPR: `MovFromCr`
/// - If destination is DR and source is GPR: `MovToDr`
/// - If source is DR and destination is GPR: `MovFromDr`
/// - Otherwise (MOV with other operand shapes): `MovGeneric`
/// - Non-MOV mnemonics: `Generic`
///
/// # Arguments
///
/// * `inst` - The instruction to classify.
///
/// # Returns
///
/// A `DispatchKind` categorizing the instruction's operand shape.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(classify(&mov_cr3_rdi), DispatchKind::MovToCr);
/// assert_eq!(classify(&mov_rdi_cr3), DispatchKind::MovFromCr);
/// assert_eq!(classify(&mov_rax_imm), DispatchKind::MovGeneric);
/// assert_eq!(classify(&cli), DispatchKind::Generic);
/// ```
#[must_use]
pub fn classify(inst: &Instruction) -> DispatchKind {
    // Non-MOV instructions always dispatch as Generic.
    if inst.mnemonic != Mnemonic::Mov {
        return DispatchKind::Generic;
    }

    // MOV instructions need at least 2 operands (dst, src).
    if inst.operands.len() < 2 {
        return DispatchKind::MovGeneric;
    }

    let dst = &inst.operands[0];
    let src = &inst.operands[1];

    // Extract register IDs from Operand::Reg variants.
    // MOV with non-register operands (e.g., memory) fall through to MovGeneric.
    let (dst_reg, src_reg) = match (dst, src) {
        (Operand::Reg(d), Operand::Reg(s)) => (*d, *s),
        // Var operands should have been resolved by resolve_var_operands pass.
        (Operand::Var { .. }, _) | (_, Operand::Var { .. }) => {
            unreachable!("Operand::Var reached classifier — resolve_var_operands pass was skipped")
        }
        // Memory or immediate operands not involving CR/DR → MovGeneric
        _ => return DispatchKind::MovGeneric,
    };

    // Check for CR (control register) patterns.
    if is_control_register(dst_reg) && is_general_purpose_register(src_reg) {
        return DispatchKind::MovToCr;
    }
    if is_control_register(src_reg) && is_general_purpose_register(dst_reg) {
        return DispatchKind::MovFromCr;
    }

    // Check for DR (debug register) patterns.
    if is_debug_register(dst_reg) && is_general_purpose_register(src_reg) {
        return DispatchKind::MovToDr;
    }
    if is_debug_register(src_reg) && is_general_purpose_register(dst_reg) {
        return DispatchKind::MovFromDr;
    }

    // All other MOV patterns (GPR-to-GPR, reg-to-memory, memory-to-reg, imm-to-reg).
    DispatchKind::MovGeneric
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::SmallVec;

    /// Helper to create a MOV instruction with register operands.
    fn mov_reg_reg(dst_id: u8, src_id: u8) -> Instruction {
        let mut operands = SmallVec::new();
        operands.push(Operand::Reg(RegId(dst_id)));
        operands.push(Operand::Reg(RegId(src_id)));
        Instruction {
            mnemonic: Mnemonic::Mov,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
        }
    }

    /// Helper to create a MOV instruction with register destination and immediate source.
    fn mov_reg_imm(dst_id: u8, imm: i64) -> Instruction {
        let mut operands = SmallVec::new();
        operands.push(Operand::Reg(RegId(dst_id)));
        operands.push(Operand::Imm64(imm));
        Instruction {
            mnemonic: Mnemonic::Mov,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
        }
    }

    /// Helper to create a CLI (clear interrupt flag) instruction.
    fn cli() -> Instruction {
        Instruction {
            mnemonic: Mnemonic::Cli,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
        }
    }

    /// Helper to create an LGDT instruction.
    fn lgdt() -> Instruction {
        Instruction {
            mnemonic: Mnemonic::Lgdt,
            operands: SmallVec::new(),
            encoding_hint: None,
            byte_offset_in_text: None,
        }
    }

    // ── MOV to Control Register tests ────────────────────────────────

    /// MOV cr0, rax → MovToCr
    #[test]
    fn mov_with_cr_dst_returns_mov_to_cr_cr0_rax() {
        // cr0 = 16, rax = 0
        let inst = mov_reg_reg(16, 0);
        assert_eq!(classify(&inst), DispatchKind::MovToCr);
    }

    /// MOV cr3, rdi → MovToCr
    #[test]
    fn mov_with_cr_dst_returns_mov_to_cr_cr3_rdi() {
        // cr3 = 16 + 3 = 19, rdi = 7
        let inst = mov_reg_reg(19, 7);
        assert_eq!(classify(&inst), DispatchKind::MovToCr);
    }

    // ── MOV from Control Register tests ──────────────────────────────

    /// MOV rax, cr0 → MovFromCr
    #[test]
    fn mov_with_cr_src_returns_mov_from_cr_rax_cr0() {
        // rax = 0, cr0 = 16
        let inst = mov_reg_reg(0, 16);
        assert_eq!(classify(&inst), DispatchKind::MovFromCr);
    }

    /// MOV rdi, cr3 → MovFromCr
    #[test]
    fn mov_with_cr_src_returns_mov_from_cr_rdi_cr3() {
        // rdi = 7, cr3 = 16 + 3 = 19
        let inst = mov_reg_reg(7, 19);
        assert_eq!(classify(&inst), DispatchKind::MovFromCr);
    }

    // ── MOV to Debug Register tests ──────────────────────────────────

    /// MOV dr0, rax → MovToDr
    #[test]
    fn mov_with_dr_dst_returns_mov_to_dr_dr0_rax() {
        // dr0 = 25, rax = 0
        let inst = mov_reg_reg(25, 0);
        assert_eq!(classify(&inst), DispatchKind::MovToDr);
    }

    /// MOV dr7, rcx → MovToDr
    #[test]
    fn mov_with_dr_dst_returns_mov_to_dr_dr7_rcx() {
        // dr7 = 25 + 7 = 32, rcx = 1
        let inst = mov_reg_reg(32, 1);
        assert_eq!(classify(&inst), DispatchKind::MovToDr);
    }

    // ── MOV from Debug Register tests ────────────────────────────────

    /// MOV rax, dr0 → MovFromDr
    #[test]
    fn mov_with_dr_src_returns_mov_from_dr_rax_dr0() {
        // rax = 0, dr0 = 25
        let inst = mov_reg_reg(0, 25);
        assert_eq!(classify(&inst), DispatchKind::MovFromDr);
    }

    /// MOV rcx, dr7 → MovFromDr
    #[test]
    fn mov_with_dr_src_returns_mov_from_dr_rcx_dr7() {
        // rcx = 1, dr7 = 25 + 7 = 32
        let inst = mov_reg_reg(1, 32);
        assert_eq!(classify(&inst), DispatchKind::MovFromDr);
    }

    // ── MOV Generic tests ───────────────────────────────────────────

    /// MOV rax, 42 → MovGeneric
    #[test]
    fn mov_imm_returns_mov_generic() {
        let inst = mov_reg_imm(0, 42);
        assert_eq!(classify(&inst), DispatchKind::MovGeneric);
    }

    /// MOV rax, rdi (GPR-to-GPR) → MovGeneric
    #[test]
    fn mov_reg_reg_gpr_only_returns_mov_generic() {
        // rax = 0, rdi = 7 (both GPR)
        let inst = mov_reg_reg(0, 7);
        assert_eq!(classify(&inst), DispatchKind::MovGeneric);
    }

    // ── Non-MOV instruction tests ────────────────────────────────────

    /// CLI (no operands) → Generic
    #[test]
    fn cli_returns_generic() {
        let inst = cli();
        assert_eq!(classify(&inst), DispatchKind::Generic);
    }

    /// LGDT (no operands, privileged) → Generic
    #[test]
    fn lgdt_returns_generic() {
        let inst = lgdt();
        assert_eq!(classify(&inst), DispatchKind::Generic);
    }
}
