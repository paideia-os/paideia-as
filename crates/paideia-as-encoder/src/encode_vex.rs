//! VEX prefix builder and AVX2 instruction encoders.
//!
//! Phase R18 PA-R18-011 (issue #1004): AVX2 baseline substrate for hash-table probe.
//! Implements VEX 2-byte (`C5`) and 3-byte (`C4`) prefix encoding per Intel SDM Vol 2A §2.3.5,
//! plus encoder stubs for Vpxor, Vpcmpeqb, Vpmovmskb, Vmovdqu.

use crate::encode::CodeBuffer;

/// VEX prefix payload for 2-byte form (C5).
///
/// Byte layout: `C5 xx`
/// - xx[7:6] = R (inverted from RegId high bit)
/// - xx[5:3] = vvvv (1's complement of second register)
/// - xx[2:1] = L (vector length: 0=128, 1=256)
/// - xx[0]   = pp (packed prefix: 0x0=none, 0x1=66h, 0x2=F3h, 0x3=F2h)
#[derive(Debug, Clone, Copy)]
pub struct Vex2 {
    /// Encoded VEX2 byte (follows the C5 marker in the stream).
    pub byte: u8,
}

impl Vex2 {
    /// Create a 2-byte VEX prefix.
    ///
    /// Parameters:
    /// - `r`: destination register high bit (bit 3 of RegId, inverted)
    /// - `vvvv`: source1 register (1's complement, 4 bits)
    /// - `l`: vector length (false=128-bit, true=256-bit)
    /// - `pp`: packed prefix encoding (0-3)
    #[must_use]
    pub fn new(r: bool, vvvv: u8, l: bool, pp: u8) -> Self {
        let mut byte = 0u8;
        if r {
            byte |= 0x80;
        }
        byte |= (vvvv & 0x0F) << 3;
        if l {
            byte |= 0x04;
        }
        byte |= pp & 0x03;
        Self { byte }
    }
}

/// VEX prefix payload for 3-byte form (C4).
///
/// Byte layout: `C4 xx yy`
/// - xx[7:6] = R, X, B (inverted from reg IDs; unused kept 1)
/// - xx[4:0] = mmmmm (map select: 0x01=0Fh escape, 0x02=0F38h, 0x03=0F3Ah)
/// - yy[7]   = W (64-bit operand; typically 0 for SIMD)
/// - yy[6:3] = vvvv (1's complement of second register)
/// - yy[2:1] = L (vector length)
/// - yy[0]   = pp (packed prefix)
#[derive(Debug, Clone, Copy)]
pub struct Vex3 {
    /// First encoded byte (follows the C4 marker; carries R/X/B and map_select).
    pub byte0: u8,
    /// Second encoded byte (carries W/vvvv/L/pp).
    pub byte1: u8,
}

impl Vex3 {
    /// Create a 3-byte VEX prefix.
    ///
    /// Parameters represent the actual register properties (not inverted):
    /// - `r_is_high`: true if dst register is high (8-15), false if low (0-7)
    /// - `x_is_high`: true if index register is high (8-15), false if low/absent (0-7)
    /// - `b_is_high`: true if base/src2 register is high (8-15), false if low (0-7)
    /// - `map_select`: opcode escape (1=0Fh, 2=0F38h, 3=0F3Ah)
    /// - `w`: 64-bit mode (typically 0 for AVX2 SIMD)
    /// - `vvvv`: source1 register (1's complement)
    /// - `l`: vector length
    /// - `pp`: packed prefix
    ///
    /// The function encodes VEX.R/X/B as the inverted forms per Intel SDM Vol 2A §2.3.5:
    /// - VEX.R = NOT(REX.R): set bit when register is LOW
    /// - VEX.X = NOT(REX.X): set bit when index is LOW
    /// - VEX.B = NOT(REX.B): set bit when base is LOW
    #[must_use]
    pub fn new(r_is_high: bool, x_is_high: bool, b_is_high: bool, map_select: u8, w: bool, vvvv: u8, l: bool, pp: u8) -> Self {
        let mut byte0 = 0u8;
        // VEX.R = NOT(REX.R): set bit when register is LOW (not high)
        if !r_is_high {
            byte0 |= 0x80;
        }
        // VEX.X = NOT(REX.X): set bit when index is LOW (not high)
        if !x_is_high {
            byte0 |= 0x40;
        }
        // VEX.B = NOT(REX.B): set bit when base is LOW (not high)
        if !b_is_high {
            byte0 |= 0x20;
        }
        byte0 |= map_select & 0x1F;

        let mut byte1 = 0u8;
        if w {
            byte1 |= 0x80;
        }
        byte1 |= (vvvv & 0x0F) << 3;
        if l {
            byte1 |= 0x04;
        }
        byte1 |= pp & 0x03;

        Self { byte0, byte1 }
    }
}

/// Determine whether a 2-byte VEX form is legal (returns true) or if 3-byte is required (returns false).
///
/// 3-byte VEX is required when:
/// - VEX.B = 0 (base/rm register is high: r8–r15)
/// - VEX.X = 0 (index register is high: r8–r15)
/// - VEX.W = 1 (64-bit operand)
/// - Map-select ≠ 0x01 (not the 0F escape)
#[must_use]
pub fn can_use_vex2(b_reg_id: u8, x_reg_id: Option<u8>) -> bool {
    // B bit is inverted: 1 means low register (0-7), 0 means high (8-15)
    let b_is_high = (b_reg_id & 0x08) != 0;
    let x_is_high = x_reg_id.map_or(false, |id| (id & 0x08) != 0);

    // 2-byte VEX allowed only if both B and X are low (or X is absent)
    !b_is_high && !x_is_high
}

/// Encode a register operand into its YMM ID and register position.
///
/// For YMM registers (RegId 37-52), returns the index (0-15) and enforces
/// that reg_id is in the YMM range. Returns `None` for invalid registers.
#[must_use]
pub fn ymm_id_from_regid(reg_id: u8) -> Option<u8> {
    if (37..=52).contains(&reg_id) {
        Some(reg_id - 37)
    } else {
        None
    }
}

/// Encode a 32-bit GPR operand into its Reg32 ID.
///
/// For GPR registers (RegId 0-15), returns the index (0-15).
/// Returns `None` for invalid registers.
#[must_use]
pub fn reg32_id_from_regid(reg_id: u8) -> Option<u8> {
    if reg_id < 16 {
        Some(reg_id)
    } else {
        None
    }
}

/// Encode Vpxor ymm dst, ymm src1, ymm src2 → VEX 66 0F EF /r.
///
/// Three operands: [Reg(dst), Reg(src1), Reg(src2)], all YMM.
/// Encoding: [VEX] 66 0F EF [ModR/M]
pub fn encode_vpxor_reg_reg_reg(
    buf: &mut CodeBuffer,
    dst_id: u8,
    src1_id: u8,
    src2_id: u8,
) {
    let dst_ymm = ymm_id_from_regid(dst_id).expect("vpxor: invalid dst register");
    let src1_ymm = ymm_id_from_regid(src1_id).expect("vpxor: invalid src1 register");
    let src2_ymm = ymm_id_from_regid(src2_id).expect("vpxor: invalid src2 register");

    // 3-byte VEX required if dst or src2 is high (>= 8)
    let dst_is_high = (dst_ymm & 0x08) != 0;
    let src2_is_high = (src2_ymm & 0x08) != 0;
    let need_3byte_vex = dst_is_high || src2_is_high;

    // Emit VEX prefix
    if !need_3byte_vex {
        // 2-byte VEX: C5 [R vvvv L pp]
        // R bit should be 1 when dst is low (0-7), 0 when high (8-15)
        let vex = Vex2::new(
            !dst_is_high,
            !src1_ymm & 0x0F,
            true,
            0x1,
        );
        buf.bytes.push(0xC5);
        buf.bytes.push(vex.byte);
    } else {
        // 3-byte VEX: C4 [R X B mmmmm] [W vvvv L pp]
        let vex = Vex3::new(
            dst_is_high,
            false, // X = high=false for register-register (no high index)
            src2_is_high,
            0x01,
            false,
            !src1_ymm & 0x0F,
            true,
            0x1,
        );
        buf.bytes.push(0xC4);
        buf.bytes.push(vex.byte0);
        buf.bytes.push(vex.byte1);
    }

    // Emit opcode EF (0F escape is encoded in VEX prefix)
    buf.bytes.push(0xEF);

    // Emit ModR/M: mod=11, reg=dst[2:0], rm=src2[2:0]
    let modrm = 0xC0 | ((dst_ymm & 0x07) << 3) | (src2_ymm & 0x07);
    buf.bytes.push(modrm);
}

/// Encode Vpcmpeqb ymm dst, ymm src1, ymm src2 → VEX 66 0F 74 /r.
pub fn encode_vpcmpeqb_reg_reg_reg(
    buf: &mut CodeBuffer,
    dst_id: u8,
    src1_id: u8,
    src2_id: u8,
) {
    let dst_ymm = ymm_id_from_regid(dst_id).expect("vpcmpeqb: invalid dst register");
    let src1_ymm = ymm_id_from_regid(src1_id).expect("vpcmpeqb: invalid src1 register");
    let src2_ymm = ymm_id_from_regid(src2_id).expect("vpcmpeqb: invalid src2 register");

    // 3-byte VEX required if dst or src2 is high
    let dst_is_high = (dst_ymm & 0x08) != 0;
    let src2_is_high = (src2_ymm & 0x08) != 0;
    let need_3byte_vex = dst_is_high || src2_is_high;

    // Emit VEX prefix
    if !need_3byte_vex {
        let vex = Vex2::new(
            !dst_is_high,
            !src1_ymm & 0x0F,
            true,
            0x1,
        );
        buf.bytes.push(0xC5);
        buf.bytes.push(vex.byte);
    } else {
        let vex = Vex3::new(
            dst_is_high,
            false,
            src2_is_high,
            0x01,
            false,
            !src1_ymm & 0x0F,
            true,
            0x1,
        );
        buf.bytes.push(0xC4);
        buf.bytes.push(vex.byte0);
        buf.bytes.push(vex.byte1);
    }

    // Emit opcode 74 (0F escape is encoded in VEX prefix)
    buf.bytes.push(0x74);

    // Emit ModR/M
    let modrm = 0xC0 | ((dst_ymm & 0x07) << 3) | (src2_ymm & 0x07);
    buf.bytes.push(modrm);
}

/// Encode Vpmovmskb r32 dst, ymm src → VEX 66 0F D7 /r.
///
/// Two operands: [Reg(dst_r32), Reg(src_ymm)].
pub fn encode_vpmovmskb_reg32_ymm(
    buf: &mut CodeBuffer,
    dst_id: u8,
    src_id: u8,
) {
    let dst_r32 = reg32_id_from_regid(dst_id).expect("vpmovmskb: invalid dst register");
    let src_ymm = ymm_id_from_regid(src_id).expect("vpmovmskb: invalid src register");

    // 3-byte VEX required if dst or src is high
    let dst_is_high = (dst_r32 & 0x08) != 0;
    let src_is_high = (src_ymm & 0x08) != 0;
    let need_3byte_vex = dst_is_high || src_is_high;

    // Emit VEX prefix
    if !need_3byte_vex {
        let vex = Vex2::new(
            !dst_is_high,
            0x0F, // vvvv = all 1's (unused for this instruction)
            true,
            0x1,
        );
        buf.bytes.push(0xC5);
        buf.bytes.push(vex.byte);
    } else {
        let vex = Vex3::new(
            dst_is_high,
            false,
            src_is_high,
            0x01,
            false,
            0x0F,
            true,
            0x1,
        );
        buf.bytes.push(0xC4);
        buf.bytes.push(vex.byte0);
        buf.bytes.push(vex.byte1);
    }

    // Emit opcode D7 (0F escape is encoded in VEX prefix)
    buf.bytes.push(0xD7);

    // Emit ModR/M: reg=dst[2:0], rm=src[2:0]
    let modrm = 0xC0 | ((dst_r32 & 0x07) << 3) | (src_ymm & 0x07);
    buf.bytes.push(modrm);
}

/// Encode Vmovdqu ymm dst, ymm src → VEX F3 0F 6F /r.
pub fn encode_vmovdqu_ymm_ymm(
    buf: &mut CodeBuffer,
    dst_id: u8,
    src_id: u8,
) {
    let dst_ymm = ymm_id_from_regid(dst_id).expect("vmovdqu: invalid dst register");
    let src_ymm = ymm_id_from_regid(src_id).expect("vmovdqu: invalid src register");

    // 3-byte VEX required if dst or src is high
    let dst_is_high = (dst_ymm & 0x08) != 0;
    let src_is_high = (src_ymm & 0x08) != 0;
    let need_3byte_vex = dst_is_high || src_is_high;

    // Emit VEX prefix
    if !need_3byte_vex {
        let vex = Vex2::new(
            !dst_is_high,
            0x0F,
            true,
            0x2, // pp = F3h
        );
        buf.bytes.push(0xC5);
        buf.bytes.push(vex.byte);
    } else {
        let vex = Vex3::new(
            dst_is_high,
            false,
            src_is_high,
            0x01,
            false,
            0x0F,
            true,
            0x2,
        );
        buf.bytes.push(0xC4);
        buf.bytes.push(vex.byte0);
        buf.bytes.push(vex.byte1);
    }

    // Emit opcode 6F (load form, 0F escape is encoded in VEX prefix)
    buf.bytes.push(0x6F);

    // Emit ModR/M
    let modrm = 0xC0 | ((dst_ymm & 0x07) << 3) | (src_ymm & 0x07);
    buf.bytes.push(modrm);
}

/// Encode Vmovdqu ymm dst, [rax] → VEX F3 0F 6F /r (load form).
pub fn encode_vmovdqu_ymm_mem(
    buf: &mut CodeBuffer,
    dst_id: u8,
    base_id: u8,
    disp: i32,
) {
    let dst_ymm = ymm_id_from_regid(dst_id).expect("vmovdqu: invalid dst register");
    let base_is_high = (base_id & 0x08) != 0;
    let dst_is_high = (dst_ymm & 0x08) != 0;
    let need_3byte_vex = dst_is_high || base_is_high;

    // Emit VEX prefix
    if !need_3byte_vex {
        let vex = Vex2::new(
            !dst_is_high,
            0x0F,
            true,
            0x2,
        );
        buf.bytes.push(0xC5);
        buf.bytes.push(vex.byte);
    } else {
        let vex = Vex3::new(
            dst_is_high,
            false,
            base_is_high,
            0x01,
            false,
            0x0F,
            true,
            0x2,
        );
        buf.bytes.push(0xC4);
        buf.bytes.push(vex.byte0);
        buf.bytes.push(vex.byte1);
    }

    // Emit opcode 6F (load form, 0F escape is encoded in VEX prefix)
    buf.bytes.push(0x6F);

    // Emit ModR/M + displacement
    if disp == 0 {
        // mod=00, reg=dst[2:0], rm=base[2:0]
        let modrm = (dst_ymm & 0x07) << 3 | (base_id & 0x07);
        buf.bytes.push(modrm);
    } else if (-128..=127).contains(&disp) {
        // mod=01, reg=dst[2:0], rm=base[2:0], disp8
        let modrm = 0x40 | (dst_ymm & 0x07) << 3 | (base_id & 0x07);
        buf.bytes.push(modrm);
        buf.bytes.push(disp as u8);
    } else {
        // mod=10, reg=dst[2:0], rm=base[2:0], disp32
        let modrm = 0x80 | (dst_ymm & 0x07) << 3 | (base_id & 0x07);
        buf.bytes.push(modrm);
        buf.bytes.extend(disp.to_le_bytes());
    }
}

/// Encode Vmovdqu ymm dst, [rip + disp32] → VEX F3 0F 6F /r (RIP-relative load form).
///
/// paideia-as#1295-b (paideia-os R21.M2 #832 unblock): the base-register+disp form
/// (`encode_vmovdqu_ymm_mem`) doesn't fit RIP-relative memory references — those
/// use mod=00, rm=101 (SIB-less, forced-disp32) rather than a base-register ModR/M.
/// This helper emits the RIP-relative form and returns the byte offset of the
/// disp32 field so the caller can attach a PcRel32 relocation.
///
/// The disp32 bytes are emitted as a placeholder (0x00000000); the caller MUST
/// attach a relocation (typically `RelocKind::PcRel32` with the symbol name
/// biased by `PC32_FIELD_BIAS = -4`) so the linker fixes up the actual offset.
///
/// Returns the byte offset of the disp32 within `buf.bytes` (relative to the
/// start of buf.bytes) — matching the pattern used by other RIP-relative
/// encoders in `encode_instruction.rs`.
pub fn encode_vmovdqu_ymm_riprel(
    buf: &mut CodeBuffer,
    dst_id: u8,
    disp: i32,
) -> usize {
    let dst_ymm = ymm_id_from_regid(dst_id).expect("vmovdqu: invalid dst register");
    let dst_is_high = (dst_ymm & 0x08) != 0;

    // 3-byte VEX required only if dst is high; RIP-rel has no base/index
    // register whose high bit could force the wider form.
    if !dst_is_high {
        // 2-byte VEX: R = NOT(dst_high) = 1, vvvv = 0x0F (unused for one-src ops),
        // L = 1 (256-bit), pp = 0x2 (F3 prefix).
        let vex = Vex2::new(true, 0x0F, true, 0x2);
        buf.bytes.push(0xC5);
        buf.bytes.push(vex.byte);
    } else {
        // 3-byte VEX: R = high, X = false, B = false, mmmmm=1 (0F escape).
        let vex = Vex3::new(true, false, false, 0x01, false, 0x0F, true, 0x2);
        buf.bytes.push(0xC4);
        buf.bytes.push(vex.byte0);
        buf.bytes.push(vex.byte1);
    }

    // Opcode 6F (load form).
    buf.bytes.push(0x6F);

    // ModR/M: mod=00, reg=dst[2:0], rm=101 (RIP-relative).
    let modrm = 0x05 | ((dst_ymm & 0x07) << 3);
    buf.bytes.push(modrm);

    // Record disp32 byte offset before pushing the placeholder — the caller
    // uses this to attach the PcRel32 relocation site.
    let disp_offset = buf.bytes.len();
    buf.bytes.extend(disp.to_le_bytes());
    disp_offset
}

/// Encode Vmovdqu [rip + disp32], ymm src → VEX F3 0F 7F /r (RIP-relative store form).
///
/// paideia-as#1295-b: mirror of `encode_vmovdqu_ymm_riprel` for the store form
/// (opcode 7F).  Same disp32 relocation contract: returns the byte offset of
/// the disp32 placeholder for the caller to attach a PcRel32 reloc.
pub fn encode_vmovdqu_riprel_ymm(
    buf: &mut CodeBuffer,
    disp: i32,
    src_id: u8,
) -> usize {
    let src_ymm = ymm_id_from_regid(src_id).expect("vmovdqu: invalid src register");
    let src_is_high = (src_ymm & 0x08) != 0;

    if !src_is_high {
        let vex = Vex2::new(true, 0x0F, true, 0x2);
        buf.bytes.push(0xC5);
        buf.bytes.push(vex.byte);
    } else {
        let vex = Vex3::new(true, false, false, 0x01, false, 0x0F, true, 0x2);
        buf.bytes.push(0xC4);
        buf.bytes.push(vex.byte0);
        buf.bytes.push(vex.byte1);
    }

    // Opcode 7F (store form).
    buf.bytes.push(0x7F);

    // ModR/M: mod=00, reg=src[2:0], rm=101 (RIP-relative).
    let modrm = 0x05 | ((src_ymm & 0x07) << 3);
    buf.bytes.push(modrm);

    let disp_offset = buf.bytes.len();
    buf.bytes.extend(disp.to_le_bytes());
    disp_offset
}

/// Encode Vmovdqu [rax], ymm src → VEX F3 0F 7F /r (store form).
pub fn encode_vmovdqu_mem_ymm(
    buf: &mut CodeBuffer,
    base_id: u8,
    disp: i32,
    src_id: u8,
) {
    let src_ymm = ymm_id_from_regid(src_id).expect("vmovdqu: invalid src register");
    let base_is_high = (base_id & 0x08) != 0;
    let src_is_high = (src_ymm & 0x08) != 0;
    let need_3byte_vex = base_is_high || src_is_high;

    // Emit VEX prefix
    if !need_3byte_vex {
        let vex = Vex2::new(
            !src_is_high,
            0x0F,
            true,
            0x2,
        );
        buf.bytes.push(0xC5);
        buf.bytes.push(vex.byte);
    } else {
        let vex = Vex3::new(
            src_is_high,
            false,
            base_is_high,
            0x01,
            false,
            0x0F,
            true,
            0x2,
        );
        buf.bytes.push(0xC4);
        buf.bytes.push(vex.byte0);
        buf.bytes.push(vex.byte1);
    }

    // Emit opcode 7F (store form, 0F escape is encoded in VEX prefix)
    buf.bytes.push(0x7F);

    // Emit ModR/M + displacement
    if disp == 0 {
        // mod=00, reg=src[2:0], rm=base[2:0]
        let modrm = (src_ymm & 0x07) << 3 | (base_id & 0x07);
        buf.bytes.push(modrm);
    } else if (-128..=127).contains(&disp) {
        // mod=01, reg=src[2:0], rm=base[2:0], disp8
        let modrm = 0x40 | (src_ymm & 0x07) << 3 | (base_id & 0x07);
        buf.bytes.push(modrm);
        buf.bytes.push(disp as u8);
    } else {
        // mod=10, reg=src[2:0], rm=base[2:0], disp32
        let modrm = 0x80 | (src_ymm & 0x07) << 3 | (base_id & 0x07);
        buf.bytes.push(modrm);
        buf.bytes.extend(disp.to_le_bytes());
    }
}
