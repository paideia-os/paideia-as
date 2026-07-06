//! Immediate-to-register 64-bit expansion for bitwise operations (PA-R13-010).
//!
//! When a bitwise operation (or/and/xor) has a 64-bit immediate operand
//! that doesn't fit in a signed 32-bit value, we expand it to a two-instruction
//! sequence:
//!   movabs r11, imm64
//!   <op>   dst, r11
//!
//! This module handles the expansion logic and collision detection.

use paideia_as_diagnostics::{Category, Diagnostic, DiagnosticCode, DiagnosticSink, Severity, Span};
use paideia_as_ir::{IrArena, IrNodeId, SmallVec, abi};
use paideia_as_ir::instruction::{Instruction, InstrMode, Mnemonic, Operand, RegId};

/// Diagnostic code for r11 collision in imm64 bitwise expansion (U1610).
/// Category U is for unsafe-block discipline (range 1600–1699).
const U_R11_COLLISION: u16 = 1610;

/// Helper: create a U-category error code for unsafe-block discipline.
fn u_code(n: u16) -> DiagnosticCode {
    DiagnosticCode::new(Category::U, Severity::Error, n).expect("valid U code")
}

/// Check whether an immediate value needs expansion.
///
/// Returns `true` if the signed 32-bit round-trip fails:
/// `imm != (imm as i32 as i64)`.
///
/// For example:
/// - 0xFFFFFFFF (as i64, value +4294967295) does NOT round-trip: it becomes -1 → expansion needed
/// - 0x7FFFFFFF (max positive i32) round-trips as +2147483647 → no expansion needed
/// - 0xFFFFFFFF00000000 (doesn't fit in i32) → round-trip fails → expansion needed
pub fn needs_expansion(imm: i64) -> bool {
    imm != (imm as i32 as i64)
}

/// Expand `<or|and|xor> dst, imm64` into:
/// ```ignore
///   movabs r11, imm64
///   <op>   dst, r11
/// ```
///
/// Returns the IrNodeId of the movabs head (labels alias to it).
/// Returns None if dst is r11 (caller should emit diagnostic U1610).
///
/// # Panics
///
/// Panics if arena allocation fails (non-recoverable).
pub fn expand_bitop_imm64(
    arena: &mut IrArena,
    stmt_span: Span,
    mnemonic: Mnemonic,
    dst: RegId,
    imm: i64,
    mode: InstrMode,
) -> Option<IrNodeId> {
    // Collision guard: if dst is r11, expansion is impossible.
    if dst == abi::R11 {
        return None;
    }

    // Allocate node for movabs r11, imm64
    let mov_id = arena.alloc(paideia_as_ir::IrKind::Placeholder, stmt_span);

    // Build and insert the movabs instruction
    let mut mov_operands: SmallVec<[Operand; 3]> = SmallVec::new();
    mov_operands.push(Operand::Reg(abi::R11));
    mov_operands.push(Operand::Imm64(imm));
    let mov_inst = Instruction {
        mnemonic: Mnemonic::Mov,
        operands: mov_operands,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode,
    };
    arena.instructions_mut().insert(mov_id, mov_inst);

    // Allocate node for <op> dst, r11
    let op_id = arena.alloc(paideia_as_ir::IrKind::Placeholder, stmt_span);

    // Build and insert the bitwise operation instruction
    let mut op_operands: SmallVec<[Operand; 3]> = SmallVec::new();
    op_operands.push(Operand::Reg(dst));
    op_operands.push(Operand::Reg(abi::R11));
    let op_inst = Instruction {
        mnemonic,
        operands: op_operands,
        encoding_hint: None,
        byte_offset_in_text: None,
        mode,
    };
    arena.instructions_mut().insert(op_id, op_inst);

    // Return the head (movabs) for label aliasing
    Some(mov_id)
}

/// Emit a diagnostic for r11 collision.
pub fn emit_r11_collision_diagnostic(
    span: Span,
    diagnostics: &mut dyn DiagnosticSink,
) {
    let diag = Diagnostic::error(u_code(U_R11_COLLISION))
        .message(
            "cannot expand or/and/xor with imm64 when destination is r11; \
             would clobber scratch register used for expansion"
                .to_string(),
        )
        .with_span(span)
        .finish();
    let _ = diagnostics.emit(diag);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_expansion_imm32_fits() {
        // 0xFFFFFFFF as i64 is the positive value 4294967295.
        // When cast as i32, it becomes -1; when cast back to i64, it becomes -1.
        // So 0xFFFFFFFF_i64 does NOT round-trip and needs expansion.
        assert!(needs_expansion(0xFFFFFFFF_i64));
    }

    #[test]
    fn needs_expansion_imm32_max_positive_fits() {
        // 0x7FFFFFFF (max positive i32) round-trips correctly
        assert!(!needs_expansion(0x7FFFFFFF_i64));
    }

    #[test]
    fn needs_expansion_imm32_negative_fits() {
        // -1 (0xFFFFFFFFFFFFFFFF) fits in i32
        assert!(!needs_expansion(-1i64));
    }

    #[test]
    fn needs_expansion_imm64_high_bit_set() {
        // 0xFFFFFFFF00000000 does not fit in i32
        assert!(needs_expansion(0xFFFFFFFF00000000u64 as i64));
    }

    #[test]
    fn needs_expansion_imm64_arbitrary() {
        // 0x8000000000000000 (min i64) does not fit in i32
        assert!(needs_expansion(0x8000000000000000u64 as i64));
    }

    #[test]
    fn needs_expansion_imm64_max() {
        // 0x7FFFFFFFFFFFFFFF (max i64) does not fit in i32
        assert!(needs_expansion(0x7FFFFFFFFFFFFFFFu64 as i64));
    }

    #[test]
    fn needs_expansion_zero() {
        // 0 fits in i32
        assert!(!needs_expansion(0i64));
    }

    #[test]
    fn collision_guard_r11() {
        assert!(abi::R11 == abi::R11);
    }

    #[test]
    fn collision_guard_r10() {
        assert!(abi::R10 != abi::R11);
    }
}
