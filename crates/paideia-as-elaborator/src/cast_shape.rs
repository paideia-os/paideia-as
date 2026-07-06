//! Integer-cast lowering table.
//!
//! Extracted from `emit_walker.rs` during the v0.17 refactor. Defines the
//! `(src, dst)` shape of a cast, the plan produced by [`cast_plan`], and the
//! (mnemonic, encoding-hint, byte-size) triple each plan lowers to.
//!
//! PA8 m3-002 (#826).

use paideia_as_ir::instruction::Mnemonic;

/// The `(src, dst)` width-and-signedness shape of an integer cast.
///
/// Widths are in bytes (1, 2, 4, or 8). Signedness selects between
/// sign-extension (`movsx`) and zero-extension (`movzx` / 32-bit `mov`) for
/// widening conversions; for narrowing and same-width conversions the
/// signedness of the *source* is irrelevant to the emitted instruction (the
/// low bits are reinterpreted unchanged) but is retained for completeness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CastShape {
    /// Source operand width in bytes (1, 2, 4, 8).
    pub src_width: u8,
    /// Destination operand width in bytes (1, 2, 4, 8).
    pub dst_width: u8,
    /// `true` if the source type is signed.
    pub src_signed: bool,
    /// `true` if the destination type is signed.
    pub dst_signed: bool,
}

/// The lowered plan for a single integer cast: which conversion instruction
/// (if any) realises the [`CastShape`].
///
/// Produced by [`cast_plan`]. `Nop` is a same-width reinterpret that emits no
/// conversion instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CastPlan {
    /// Sign-extend a 1/2/4-byte source into a 64-bit register (`movsx{b,w}q`,
    /// `movsxd`). The `u8` is the source width carried in `operand_size`.
    SignExtend(u8),
    /// Zero-extend a 1/2-byte source into a 64-bit register (`movzx`). The
    /// `u8` is the source width carried in `operand_size`.
    ZeroExtend(u8),
    /// 32-bit register move (`mov r32, r32`): used for unsigned widening of a
    /// 4-byte source — the 32-bit write implicitly clears bits 63:32.
    Mov32,
    /// Narrowing register move: write the low `u8` bytes of the destination
    /// (`mov r{8,16,32}`). The `u8` is the destination width.
    Narrow(u8),
    /// Same-width reinterpret: no instruction emitted.
    Nop,
}

impl CastPlan {
    /// Lower this plan to `(mnemonic, encoding_hint, estimated_byte_size)`, or
    /// `None` for a [`CastPlan::Nop`].
    ///
    /// Estimated sizes match the encoder:
    /// - `movsxd` (4-byte src): REX.W + 0x63 + ModRM = 3 bytes
    /// - `movsx{b,w}q` (1/2-byte src): REX.W + 0x0F + opcode + ModRM = 4 bytes
    /// - `movzx` (1/2-byte src): REX.W + 0x0F + opcode + ModRM = 4 bytes
    /// - `mov r32, r32`: opcode + ModRM = 2 bytes (no REX.W for RAX/RDI)
    /// - narrowing `mov`: opcode + ModRM = 2 bytes (low registers)
    #[must_use]
    pub fn instruction(self) -> Option<(Mnemonic, Option<paideia_as_ir::EncodingHint>, u32)> {
        match self {
            CastPlan::SignExtend(src_width) => {
                let opcode = if src_width == 4 { 0x63 } else { 0x0F };
                let size = if src_width == 4 { 3 } else { 4 };
                Some((
                    Mnemonic::Movsx,
                    Some(paideia_as_ir::EncodingHint {
                        opcode,
                        operand_size: src_width,
                    }),
                    size,
                ))
            }
            CastPlan::ZeroExtend(src_width) => {
                let opcode = if src_width == 1 { 0xB6 } else { 0xB7 };
                Some((
                    Mnemonic::Movzx,
                    Some(paideia_as_ir::EncodingHint {
                        opcode,
                        operand_size: src_width,
                    }),
                    4,
                ))
            }
            CastPlan::Mov32 => Some((
                Mnemonic::Mov,
                Some(paideia_as_ir::EncodingHint {
                    opcode: 0x8B,
                    operand_size: 4,
                }),
                2,
            )),
            CastPlan::Narrow(dst_width) => Some((
                Mnemonic::Mov,
                Some(paideia_as_ir::EncodingHint {
                    opcode: 0x8B,
                    operand_size: dst_width,
                }),
                2,
            )),
            CastPlan::Nop => None,
        }
    }
}

/// Dispatch an integer [`CastShape`] to its [`CastPlan`].
///
/// PA8 m3-002 (#826). Replaces the prior "always `movsxd`" behaviour with the
/// real x86_64 dispatch table keyed by `(src_width, dst_width, src_signed,
/// dst_signed)`:
///
/// | condition                          | plan                  |
/// |------------------------------------|-----------------------|
/// | `dst_width < src_width` (narrowing)| `Narrow(dst_width)`   |
/// | `dst_width == src_width`           | `Nop`                 |
/// | widening, `src_signed`             | `SignExtend(src_width)`|
/// | widening, unsigned, `src_width==4` | `Mov32`               |
/// | widening, unsigned, `src_width<4`  | `ZeroExtend(src_width)`|
///
/// Note narrowing and same-width are signedness-agnostic: the low bits are
/// reinterpreted unchanged, so no sign/zero extension is required. Widening's
/// extension is governed by the *source* signedness (an `i8` widens by sign,
/// a `u8` by zero), independent of the destination's signedness.
#[must_use]
pub fn cast_plan(shape: CastShape) -> CastPlan {
    let CastShape {
        src_width,
        dst_width,
        src_signed,
        ..
    } = shape;

    if dst_width < src_width {
        CastPlan::Narrow(dst_width)
    } else if dst_width == src_width {
        CastPlan::Nop
    } else if src_signed {
        CastPlan::SignExtend(src_width)
    } else if src_width == 4 {
        CastPlan::Mov32
    } else {
        CastPlan::ZeroExtend(src_width)
    }
}
