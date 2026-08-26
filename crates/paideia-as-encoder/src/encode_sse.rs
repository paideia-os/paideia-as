//! Legacy-SSE (non-VEX) scalar float instruction encoders.
//!
//! paideia-os #1333, paideia-as#1333: scalar f32/f64 arithmetic codegen.
//! Mirrors `encode_vex.rs`'s per-family-file convention, but for the
//! mandatory-prefix (66/F2/F3) `0F` two-byte-opcode legacy SSE1/SSE2
//! encodings (SDM Vol 2A) rather than VEX-prefixed AVX2.
//!
//! Scope: register-register only. Memory-operand scalar-float forms
//! (spill/reload, `movsd xmm, [mem]`) are deferred — see issue #1333's
//! discussion for the follow-up.
//!
//! All instructions here address XMM registers via `RegId(53..=68)`
//! (XMM0–XMM15) and, for the convert/bitcast mnemonics, ordinary GPRs
//! via `RegId(0..=15)`.

use crate::encode::CodeBuffer;

/// Encode a register operand into its XMM index (0–15).
///
/// XMM registers occupy the compact `RegId` band 53–68 (see
/// `paideia-as-runtime::instruction::RegId` doc comment). Returns `None`
/// for any id outside that band.
#[must_use]
pub fn xmm_id_from_regid(reg_id: u8) -> Option<u8> {
    if (53..=68).contains(&reg_id) {
        Some(reg_id - 53)
    } else {
        None
    }
}

/// Emit the REX prefix iff any of W/R/X/B require it, per the legacy-SSE
/// convention: REX is optional (omitted when all of W/R/X/B are false),
/// unlike the mandatory REX.W idiom used elsewhere in this encoder for
/// 64-bit GPR ops.
fn maybe_rex(buf: &mut CodeBuffer, w: bool, r: bool, b: bool) {
    if w || r || b {
        buf.bytes.push(0x40 | (u8::from(w) << 3) | (u8::from(r) << 2) | u8::from(b));
    }
}

/// Emit ModR/M for a register-direct (mod=11) operand pair: reg field
/// `reg_id` (already the 0-15 index within its register class), rm field
/// `rm_id` (same).
fn modrm_reg_reg(buf: &mut CodeBuffer, reg_id: u8, rm_id: u8) {
    buf.bytes.push(0xC0 | ((reg_id & 7) << 3) | (rm_id & 7));
}

/// Encode a mandatory-prefix, no-REX.W, xmm-reg/xmm-reg SSE instruction:
/// `[prefix] 0F opcode /r` with reg=dst, rm=src (both XMM).
///
/// Covers movsd/movss/addsd/addss/subsd/subss/mulsd/mulss/divsd/divss/
/// sqrtsd/sqrtss/ucomisd/ucomiss/comisd/comiss.
pub fn encode_xmm_xmm(buf: &mut CodeBuffer, prefix: Option<u8>, opcode: u8, dst_xmm: u8, src_xmm: u8) {
    if let Some(p) = prefix {
        buf.bytes.push(p);
    }
    let dst_high = (dst_xmm & 0x08) != 0;
    let src_high = (src_xmm & 0x08) != 0;
    maybe_rex(buf, false, dst_high, src_high);
    buf.bytes.push(0x0F);
    buf.bytes.push(opcode);
    modrm_reg_reg(buf, dst_xmm, src_xmm);
}

/// Encode `cvtsi2sd`/`cvtsi2ss xmm dst, r64 src`: `[F2|F3] REX.W 0F 2A /r`.
/// reg=dst (xmm), rm=src (gpr). REX.W is always set (64-bit int source).
pub fn encode_cvtsi2s(buf: &mut CodeBuffer, prefix: u8, dst_xmm: u8, src_gpr: u8) {
    buf.bytes.push(prefix);
    let dst_high = (dst_xmm & 0x08) != 0;
    let src_high = (src_gpr & 0x08) != 0;
    buf.bytes.push(0x40 | 0x08 | (u8::from(dst_high) << 2) | u8::from(src_high));
    buf.bytes.push(0x0F);
    buf.bytes.push(0x2A);
    modrm_reg_reg(buf, dst_xmm, src_gpr);
}

/// Encode `cvttsd2si`/`cvttss2si r64 dst, xmm src` (truncating):
/// `[F2|F3] REX.W 0F 2C /r`. reg=dst (gpr), rm=src (xmm). REX.W always set.
pub fn encode_cvtts2si(buf: &mut CodeBuffer, prefix: u8, dst_gpr: u8, src_xmm: u8) {
    buf.bytes.push(prefix);
    let dst_high = (dst_gpr & 0x08) != 0;
    let src_high = (src_xmm & 0x08) != 0;
    buf.bytes.push(0x40 | 0x08 | (u8::from(dst_high) << 2) | u8::from(src_high));
    buf.bytes.push(0x0F);
    buf.bytes.push(0x2C);
    modrm_reg_reg(buf, dst_gpr, src_xmm);
}

/// Encode `movd`/`movq` bitcast between a GPR and an XMM register.
///
/// `to_xmm = true`  → load form  `66 [REX.W] 0F 6E /r` (reg=xmm dst, rm=gpr src).
/// `to_xmm = false` → store form `66 [REX.W] 0F 7E /r` (reg=xmm src, rm=gpr dst).
/// `rex_w` selects movd (false, 32-bit) vs movq (true, 64-bit).
pub fn encode_movd_movq_bitcast(buf: &mut CodeBuffer, rex_w: bool, to_xmm: bool, xmm_id: u8, gpr_id: u8) {
    buf.bytes.push(0x66);
    let xmm_high = (xmm_id & 0x08) != 0;
    let gpr_high = (gpr_id & 0x08) != 0;
    // reg field is always the xmm register; rm field is always the gpr.
    maybe_rex(buf, rex_w, xmm_high, gpr_high);
    buf.bytes.push(0x0F);
    buf.bytes.push(if to_xmm { 0x6E } else { 0x7E });
    modrm_reg_reg(buf, xmm_id, gpr_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmm_id_from_regid_maps_53_to_68() {
        assert_eq!(xmm_id_from_regid(53), Some(0));
        assert_eq!(xmm_id_from_regid(68), Some(15));
        assert_eq!(xmm_id_from_regid(52), None);
        assert_eq!(xmm_id_from_regid(69), None);
    }

    #[test]
    fn movsd_xmm0_xmm1_emits_f2_0f_10_c1() {
        let mut buf = CodeBuffer::new();
        encode_xmm_xmm(&mut buf, Some(0xF2), 0x10, 0, 1);
        assert_eq!(buf.bytes, vec![0xF2, 0x0F, 0x10, 0xC1]);
    }

    #[test]
    fn addsd_xmm8_xmm9_sets_rex_r_and_b() {
        let mut buf = CodeBuffer::new();
        encode_xmm_xmm(&mut buf, Some(0xF2), 0x58, 8, 9);
        // REX = 0100 0101 = 0x45 (W=0,R=1,X=0,B=1)
        assert_eq!(buf.bytes, vec![0xF2, 0x45, 0x0F, 0x58, 0xC1]);
    }

    #[test]
    fn cvtsi2sd_xmm0_rax_sets_rex_w() {
        let mut buf = CodeBuffer::new();
        encode_cvtsi2s(&mut buf, 0xF2, 0, 0);
        assert_eq!(buf.bytes, vec![0xF2, 0x48, 0x0F, 0x2A, 0xC0]);
    }
}
