//! Handler-bracketed region prologue / epilogue generation.
//!
//! Per `design/toolchain/calling-convention.md` §3.1–3.3:
//! R15 holds the effect-environment / handler-table pointer. On entering
//! `handle e { ... }`, the prologue saves the outer environment pointer on
//! the stack and installs the new one. On exit, the epilogue restores the
//! outer pointer.

use crate::encode::{CodeBuffer, Reg64, mov_reg64_reg64, pop_reg64, push_reg64};

/// Emit the byte sequence that opens a handler-bracketed region.
///
/// Per calling-convention.md, R15 holds the effect-environment pointer.
/// On entering `handle e { ... }`:
///
/// 1. push R15 (save the outer environment pointer on the stack).
/// 2. mov R15, <new_env_addr_reg> (point R15 at the new handler env).
///
/// The new-env-addr is in `new_env_reg` (typically RAX, set by the
/// caller from the handler-value evaluation).
///
/// Emits 2 + 3 = 5 bytes on R15 with extended registers (REX.B required).
pub fn emit_handler_open(buf: &mut CodeBuffer, new_env_reg: Reg64) {
    // 1. push R15 (save the outer environment pointer).
    push_reg64(buf, Reg64::R15);

    // 2. mov R15, new_env_reg (point R15 at the new handler env).
    mov_reg64_reg64(buf, Reg64::R15, new_env_reg);
}

/// Emit the byte sequence that closes a handler-bracketed region.
///
/// Restores R15 from the stack:
/// 1. pop R15 (restore the outer environment pointer).
///
/// Emits 2 bytes (REX.B + opcode) for R15.
pub fn emit_handler_close(buf: &mut CodeBuffer) {
    // 1. pop R15 (restore the outer environment pointer).
    pop_reg64(buf, Reg64::R15);
}

/// Emit a nested-handler "chain" sequence — used when one handler
/// installs inside another. The new handler's env points at a record
/// whose `.parent` field is the saved outer R15 value.
///
/// Phase-2-m11 minimum: emits handler_open + a marker that the
/// installer should populate the parent slot. Real chain-construction
/// instructions land at the link stage.
pub fn emit_handler_chain(buf: &mut CodeBuffer, new_env_reg: Reg64) {
    // Phase-2-m11: same as handler_open; link-stage populates parent slot
    emit_handler_open(buf, new_env_reg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_open_pushes_r15_and_moves_new_env() {
        let mut buf = CodeBuffer::new();
        emit_handler_open(&mut buf, Reg64::Rax);

        // Sequence: push r15 (2 bytes) + mov r15, rax (3 bytes)
        // push r15: 41 57
        // mov r15, rax:
        //   mov_reg64_reg64(dst=R15, src=Rax)
        //   dst_id = 15, src_id = 0
        //   REX: W=1, R=(0>>3)=0, B=(15>>3)=1 → 0x49
        //   89 (opcode for mov)
        //   ModR/M: 0xC0 | ((0&7)<<3) | (15&7) = 0xC0 | 0 | 7 = 0xc7
        assert_eq!(buf.as_slice(), &[0x41, 0x57, 0x49, 0x89, 0xc7]);
    }

    #[test]
    fn handler_close_pops_r15() {
        let mut buf = CodeBuffer::new();
        emit_handler_close(&mut buf);

        // pop r15: 41 5f
        assert_eq!(buf.as_slice(), &[0x41, 0x5f]);
    }

    #[test]
    fn handler_open_then_close_is_symmetric() {
        let mut buf = CodeBuffer::new();
        let initial_len = buf.len();

        emit_handler_open(&mut buf, Reg64::Rax);
        let after_open = buf.len();

        emit_handler_close(&mut buf);
        let after_close = buf.len();

        // Sanity check: we emitted bytes
        assert!(after_open > initial_len);
        assert!(after_close > after_open);

        // The byte sequence should be: push r15, mov r15 rax, pop r15
        // 41 57 (push r15) + 49 89 c7 (mov r15, rax) + 41 5f (pop r15)
        assert_eq!(buf.as_slice(), &[0x41, 0x57, 0x49, 0x89, 0xc7, 0x41, 0x5f]);
    }

    #[test]
    fn handler_open_with_different_source_registers() {
        let mut buf = CodeBuffer::new();
        emit_handler_open(&mut buf, Reg64::Rcx);

        // push r15 (41 57) + mov r15, rcx (49 89 cf)
        //   mov_reg64_reg64(dst=R15, src=Rcx)
        //   dst_id = 15, src_id = 1
        //   REX: W=1, R=(1>>3)=0, B=(15>>3)=1 → 0x49
        //   89 (opcode)
        //   ModR/M: 0xC0 | ((1&7)<<3) | (15&7) = 0xC0 | 8 | 7 = 0xcf
        assert_eq!(buf.as_slice(), &[0x41, 0x57, 0x49, 0x89, 0xcf]);
    }

    #[test]
    fn handler_chain_opens_handler() {
        let mut buf = CodeBuffer::new();
        emit_handler_chain(&mut buf, Reg64::Rdx);

        // Same as emit_handler_open: push r15 + mov r15, rdx
        // push r15 (41 57) + mov r15, rdx (49 89 d7)
        //   mov_reg64_reg64(dst=R15, src=Rdx)
        //   dst_id = 15, src_id = 2
        //   REX: W=1, R=(2>>3)=0, B=(15>>3)=1 → 0x49
        //   89 (opcode)
        //   ModR/M: 0xC0 | ((2&7)<<3) | (15&7) = 0xC0 | 16 | 7 = 0xd7
        assert_eq!(buf.as_slice(), &[0x41, 0x57, 0x49, 0x89, 0xd7]);
    }
}
