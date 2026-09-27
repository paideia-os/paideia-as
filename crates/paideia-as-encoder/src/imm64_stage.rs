//! Shared imm64 auto-staging helper (paideia-as#1548 / PAS-DEBT-B4-005).
//!
//! Several arithmetic / bitwise mnemonics — `cmp`, `and`, `or`, `xor`,
//! `add`, `sub` — have no `r/m64, imm64` form on x86_64. The largest
//! immediate the ISA carries for these is imm32, and it is
//! sign-extended into the 64-bit destination. When a caller passes a
//! true imm64 that does not round-trip through i32, the encoder cannot
//! emit a single instruction; it must lower to a two-instruction
//! sequence via a scratch register:
//!
//!     movabs r11, imm64        ; REX.W B8+rd io — the only x86 form
//!     <op>   r/m64, r11        ; existing reg-reg encoder
//!
//! This mirrors the `mov [mem], imm64` auto-staging introduced in
//! paideia-as#1526 (PAS-DEBT-B4-004) for the store shape. R11 is the
//! reserved caller-saved scratch register documented across the
//! emitter (see also `emit_store_record.rs`, `mov::encode_mov` MemSib
//! arms, and `imm64_expand.rs` U1615).
//!
//! Call `stage_imm64_r11` right before the reg-reg fallback: on
//! success it emits `movabs r11, imm64` and returns `Reg64::R11` for
//! the caller to feed into its reg-reg encoder. On R11 collision (the
//! operand register IS r11 — the movabs would clobber the very value
//! the following instruction reads) it returns
//! `EncodeError::Unsupported(collision_msg)` so the elaborator can
//! surface a diagnostic rather than a silent miscompile.
//!
//! The imm-fits-in-i32 short-form decision is left to each caller —
//! this helper fires only for the true-imm64 arm.

use crate::encode::{mov_reg64_imm64, CodeBuffer, Reg64};
use crate::encode_instruction::EncodeError;

/// Emit `movabs r11, imm64` and return R11 as the scratch source
/// register, unless the caller's operand register IS r11 (in which
/// case the movabs would clobber it before the following instruction
/// could consume it). On collision the returned error carries the
/// caller-supplied `collision_msg` so diagnostics stay mnemonic-specific.
///
/// Callers should follow a successful call with the reg-reg encoding
/// form for their mnemonic, passing the returned `Reg64::R11` as the
/// source operand:
///
/// ```ignore
/// let src = stage_imm64_r11(buf, dst_reg, imm as u64,
///     "and r11, imm64: scratch-reg r11 collision — pick a different destination register")?;
/// and_reg64_reg64(buf, dst_reg, src);
/// ```
#[inline]
pub(crate) fn stage_imm64_r11(
    buf: &mut CodeBuffer,
    operand_reg: Reg64,
    imm: u64,
    collision_msg: &'static str,
) -> Result<Reg64, EncodeError> {
    if operand_reg == Reg64::R11 {
        return Err(EncodeError::Unsupported(collision_msg));
    }
    mov_reg64_imm64(buf, Reg64::R11, imm);
    Ok(Reg64::R11)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_imm64_r11_emits_movabs_and_returns_r11_for_non_r11_dst() {
        let mut buf = CodeBuffer::new();
        let scratch = stage_imm64_r11(&mut buf, Reg64::Rax, 0x1122_3344_5566_7788, "oops")
            .expect("non-r11 dst must succeed");
        assert_eq!(scratch, Reg64::R11);
        // movabs r11, 0x1122334455667788 → 49 BB 88 77 66 55 44 33 22 11
        assert_eq!(
            buf.bytes,
            vec![0x49, 0xBB, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11]
        );
    }

    #[test]
    fn stage_imm64_r11_rejects_r11_collision() {
        let mut buf = CodeBuffer::new();
        let err = stage_imm64_r11(&mut buf, Reg64::R11, 0xdead_beef_cafe_babe, "r11-collision");
        match err {
            Err(EncodeError::Unsupported(msg)) => assert_eq!(msg, "r11-collision"),
            other => panic!("expected Unsupported r11-collision, got {other:?}"),
        }
        // No bytes must have been written on the failure path.
        assert!(buf.bytes.is_empty());
    }
}
