//! System V ABI bridge generation for PaideiaOS → C interop.
//!
//! Per custom-assembler.md §8.6, when PaideiaOS code calls out to C
//! (System V ABI), R15 (the effect-environment pointer) must be saved
//! because the C ABI doesn't preserve it.
//!
//! This module emits the prologue and epilogue that bracket a System V call.
//! The complete thunk (marshalling, stack alignment, etc.) is generated at
//! the elaborator stage; this module provides the low-level byte emission.

use crate::encode::{CodeBuffer, pop_reg64, push_reg64};

/// Emit the System V-bridge prologue. Per custom-assembler.md §8.6, when
/// PaideiaOS code calls out to C (System V ABI), R15 must be saved because
/// the C ABI doesn't preserve it.
///
/// Phase-2-m11 minimum:
/// 1. push R15 (save the effect-environment pointer on the stack).
///
/// The C call happens between this and the matching epilogue.
///
/// Emits 2 bytes (REX.B + opcode) for R15.
pub fn emit_sysv_bridge_prologue(buf: &mut CodeBuffer) {
    // 1. push R15 (save the effect-environment pointer).
    push_reg64(buf, Reg64::R15);
}

/// Emit the System V-bridge epilogue.
///
/// Restores R15 from the stack after the C call returns:
/// 1. pop R15 (restore the saved environment pointer).
///
/// Emits 2 bytes (REX.B + opcode) for R15.
pub fn emit_sysv_bridge_epilogue(buf: &mut CodeBuffer) {
    // 1. pop R15 (restore the saved environment pointer).
    pop_reg64(buf, Reg64::R15);
}

use crate::encode::Reg64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysv_bridge_prologue_pushes_r15() {
        let mut buf = CodeBuffer::new();
        emit_sysv_bridge_prologue(&mut buf);

        // push r15: 41 57
        assert_eq!(buf.as_slice(), &[0x41, 0x57]);
    }

    #[test]
    fn sysv_bridge_epilogue_pops_r15() {
        let mut buf = CodeBuffer::new();
        emit_sysv_bridge_epilogue(&mut buf);

        // pop r15: 41 5f
        assert_eq!(buf.as_slice(), &[0x41, 0x5f]);
    }

    #[test]
    fn sysv_bridge_round_trip() {
        let mut buf = CodeBuffer::new();
        emit_sysv_bridge_prologue(&mut buf);
        emit_sysv_bridge_epilogue(&mut buf);

        // prologue + epilogue: push r15 + pop r15
        // 41 57 (push r15) + 41 5f (pop r15)
        assert_eq!(buf.as_slice(), &[0x41, 0x57, 0x41, 0x5f]);
    }

    #[test]
    fn prologue_and_epilogue_lengths() {
        let mut buf_prologue = CodeBuffer::new();
        emit_sysv_bridge_prologue(&mut buf_prologue);
        assert_eq!(buf_prologue.len(), 2);

        let mut buf_epilogue = CodeBuffer::new();
        emit_sysv_bridge_epilogue(&mut buf_epilogue);
        assert_eq!(buf_epilogue.len(), 2);

        let mut buf_both = CodeBuffer::new();
        emit_sysv_bridge_prologue(&mut buf_both);
        emit_sysv_bridge_epilogue(&mut buf_both);
        assert_eq!(buf_both.len(), 4);
    }
}
