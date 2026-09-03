//! `f16` — IEEE 754 binary16 type intrinsic descriptor.
//!
//! **Row:** v0.30-M1-003 (paideia-as#1381), Wave 0 Batch 4.
//!
//! `f16` is a first-class scalar type in the paideia-as surface language.
//! It carries no operator support of its own on x86_64 — every arithmetic
//! or comparison operation is lowered to a widened `f32` computation
//! bracketed by `VCVTPH2PS` / `VCVTPS2PH` (F16C, CPUID.01H:ECX.F16C[bit 29]).
//! The composition path that motivates it in this round is scRGB-linear
//! shading in the v0.30 Vulkan / SPIR-V lift, where fp16 storage cuts
//! HDR framebuffer bandwidth in half without visible banding.
//!
//! # Scope of this file
//!
//! * **Type descriptor** — [`F16Descriptor`] + [`F16_DESCRIPTOR`] tell the
//!   elaborator the width, alignment, and canonical name it should hand
//!   back when a source-level `f16` name resolves. The parser looks the
//!   type up nominally through the existing `TypeData::Name` machinery in
//!   `paideia-as-ast`; this descriptor is what the resolver / layout
//!   query returns for that name.
//! * **Bit-pattern wrapper** — [`F16`] carries a raw `u16` and gives the
//!   constant folder + tests a hand-rolled software round-trip
//!   ([`F16::from_f32`], [`F16::to_f32`]) that does **not** depend on any
//!   platform F16C intrinsic. The M1 elaborator uses it to fold
//!   compile-time `f16` literals; the M2 encoder row swaps in
//!   `VCVTPS2PH`/`VCVTPH2PS` for the runtime path but keeps this module
//!   as the reference oracle for its differential tests.
//!
//! # What this row deliberately does *not* do
//!
//! * No `VCVTPH2PS` / `VCVTPS2PH` machine-code emit. That is the Wave-1
//!   encoder row (paired with the F16C feature gate in
//!   `paideia-as-target` and the SSE encoder rework landed in commit
//!   `65b6f19`).
//! * No `f16` arithmetic (`+`, `*`, comparisons). Those are lowered by
//!   the M2 IR pass to `promote_to_f32 → op_f32 → narrow_to_f16`.
//! * No dependency on the `half` crate. `paideia-as-intrinsic` has zero
//!   non-workspace deps and stays that way — a 60-LoC hand-rolled
//!   IEEE 754 binary16 encoder is cheaper than a crate audit + supply-chain
//!   surface.
//!
//! # IEEE 754 binary16 reference
//!
//! ```text
//!   bit    15   14 ─── 10   9 ────────── 0
//!         [ S ][   E (5)  ][    M (10)    ]
//! ```
//!
//! * Sign `S` — 1 bit.
//! * Biased exponent `E` — 5 bits, bias 15. `E = 0` marks zero/subnormal,
//!   `E = 0x1F` marks infinity/NaN, `E ∈ [1, 30]` marks normal.
//! * Significand `M` — 10 stored bits; normal encoding has an implicit
//!   leading `1.` before `M`.
//!
//! Value formulas (per IEEE 754-2008 §3.4):
//!
//! * Normal:    `(-1)^S × 2^(E − 15) × (1 + M / 2^10)`
//! * Subnormal: `(-1)^S × 2^-14 × (M / 2^10) = M × 2^-24`
//! * Range: min positive subnormal `2^-24 ≈ 5.96e-8`, max finite
//!   `65504 = (2 − 2^-10) × 2^15`.

// ---------------------------------------------------------------------------
// Type descriptor
// ---------------------------------------------------------------------------

/// Canonical source-level name the paideia-as parser accepts for this type.
///
/// Kept as a `&'static str` (not a UTF-8 byte slice, not an interned
/// symbol) so the elaborator's nominal-lookup table can `str::eq_ignore_case`
/// or `==` against it without pulling in the interner from
/// `paideia-as-frontend`. The Wave-1 encoder row registers this exact
/// name in the primitive-type table.
pub const F16_TYPE_NAME: &str = "f16";

/// Storage width of an `f16` value in bits.
pub const F16_BITS: u32 = 16;

/// Natural alignment of an `f16` value in bytes.
///
/// SysV AMD64 aligns 16-bit scalars on their own width, and `VCVTPS2PH`
/// stores its 16-bit lanes at 2-byte granularity — so 2 is both the
/// language-level and the codegen-level answer. A vector-of-`f16` (`[f16; N]`)
/// inherits the element alignment; the SPIR-V / Vulkan lift may promote
/// this to 8- or 16-byte where the shader ABI demands it, without
/// changing the scalar answer.
pub const F16_ALIGN_BYTES: u32 = 2;

/// Compile-time descriptor the elaborator returns for the `f16` type name.
///
/// A record rather than free constants so a downstream table can hold
/// `&'static F16Descriptor` alongside sibling descriptors (`f32`, `f64`,
/// `f128`, `bf16`) without a wide-tuple bespoke shape for every scalar.
/// The three fields are the *interface*; anything internal to the fp16
/// implementation (rounding mode, subnormal policy) is a fact about the
/// module, not the descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct F16Descriptor {
    /// Source-level type name (`"f16"`).
    pub name: &'static str,
    /// Storage width in bits (16).
    pub bits: u32,
    /// Natural alignment in bytes (2).
    pub align_bytes: u32,
}

/// The single stable descriptor value for the `f16` type.
pub const F16_DESCRIPTOR: F16Descriptor = F16Descriptor {
    name: F16_TYPE_NAME,
    bits: F16_BITS,
    align_bytes: F16_ALIGN_BYTES,
};

// ---------------------------------------------------------------------------
// Bit-pattern wrapper
// ---------------------------------------------------------------------------

/// IEEE 754 binary16 bit-pattern wrapper.
///
/// The inner `u16` is the raw storage encoding — sign in bit 15, biased
/// exponent in bits 14..10, fraction in bits 9..0. Two distinct NaN
/// payloads compare unequal at the bit level even though both are NaN,
/// which is deliberate: this type reflects *encoding* identity, not
/// numeric equality. Use [`F16::to_f32`] and native `f32` comparisons
/// when numeric semantics are wanted.
///
/// `Copy` + `Eq` + `Hash` are derived because the type is a plain 16-bit
/// bit pattern — the ordering semantics of `PartialOrd`/`Ord` on IEEE
/// bit patterns are not the numeric ordering (NaN interposes), so those
/// are deliberately *not* derived. A downstream numeric-comparison helper
/// belongs on the value obtained via [`F16::to_f32`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct F16(pub u16);

impl F16 {
    /// Positive zero (`+0.0`).
    pub const ZERO: Self = Self(0x0000);
    /// Negative zero (`-0.0`).
    pub const NEG_ZERO: Self = Self(0x8000);
    /// Positive infinity.
    pub const INFINITY: Self = Self(0x7C00);
    /// Negative infinity.
    pub const NEG_INFINITY: Self = Self(0xFC00);
    /// Canonical quiet NaN.
    ///
    /// The choice matches Intel's F16C convention (top fraction bit set
    /// to mark quiet, all other fraction bits zero, sign zero). Any NaN
    /// input to [`F16::from_f32`] preserves its own payload; this
    /// constant is what synthesised NaNs (constant folder default,
    /// `0.0 / 0.0`) emit.
    pub const NAN: Self = Self(0x7E00);
    /// Smallest positive subnormal (`2^-24`).
    pub const MIN_POSITIVE_SUBNORMAL: Self = Self(0x0001);
    /// Smallest positive normal (`2^-14`).
    pub const MIN_POSITIVE_NORMAL: Self = Self(0x0400);
    /// Largest finite value (`(2 − 2^-10) × 2^15 = 65504`).
    pub const MAX: Self = Self(0x7BFF);

    /// Wrap a raw 16-bit encoding without interpretation.
    #[inline]
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// Return the raw 16-bit encoding.
    #[inline]
    pub const fn to_bits(self) -> u16 {
        self.0
    }

    /// True if the encoding is any NaN (`E == 0x1F && M != 0`).
    #[inline]
    pub const fn is_nan(self) -> bool {
        (self.0 & 0x7C00) == 0x7C00 && (self.0 & 0x03FF) != 0
    }

    /// True if the encoding is `±∞` (`E == 0x1F && M == 0`).
    #[inline]
    pub const fn is_infinite(self) -> bool {
        (self.0 & 0x7FFF) == 0x7C00
    }

    /// True if the encoding is finite (not NaN, not infinite).
    #[inline]
    pub const fn is_finite(self) -> bool {
        (self.0 & 0x7C00) != 0x7C00
    }

    /// True if the encoding is subnormal (`E == 0 && M != 0`).
    #[inline]
    pub const fn is_subnormal(self) -> bool {
        (self.0 & 0x7C00) == 0 && (self.0 & 0x03FF) != 0
    }

    // -----------------------------------------------------------------------
    // binary16 → binary32
    // -----------------------------------------------------------------------

    /// Widen this `f16` to a native `f32`, exactly.
    ///
    /// Every finite `f16` is representable in `f32` (binary16's range and
    /// precision are strict subsets of binary32's), so this conversion is
    /// *lossless* for normals and subnormals. Infinities and NaNs
    /// preserve their sign; NaNs preserve their 10-bit payload in the
    /// upper 10 bits of the 23-bit `f32` fraction (lower 13 bits zero).
    ///
    /// The routine is branchless-ish — one `match` on the biased
    /// exponent, one leading-zero-count-shaped loop for subnormals —
    /// which lets the constant folder use it in `const`-shaped contexts
    /// after Rust 1.83 lifts the loop restriction; today it stays a
    /// non-`const fn` to keep MSRV honest against the workspace's
    /// `rust-version` pin.
    pub fn to_f32(self) -> f32 {
        let h = self.0 as u32;
        let sign = (h >> 15) & 0x1;
        let exp = (h >> 10) & 0x1F;
        let mant = h & 0x03FF;

        let (f32_exp_field, f32_frac) = if exp == 0 {
            if mant == 0 {
                // ±0 — encode as f32 zero of matching sign.
                return f32::from_bits(sign << 31);
            }
            // Subnormal: normalise into the f32 normal band.
            //
            // Start with the 10-bit mantissa and track how many
            // left-shifts it takes to place the leading 1 at bit 10
            // (i.e. into the f32-normal "implicit 1." position). Each
            // shift walks the unbiased exponent one step below the
            // subnormal boundary (which itself sits at f16-unbiased −14),
            // so the resulting f32-biased exponent is
            //   127 + (unbiased_f16) = 127 + (−14 + e_delta)
            //   = 113 + e_delta        where e_delta ≤ −1
            // and `e_delta` here starts at 0 and decrements once per
            // shift. See module-header table for the value formulas.
            let mut m = mant;
            let mut e_delta: i32 = 0;
            while (m & 0x0400) == 0 {
                m <<= 1;
                e_delta -= 1;
            }
            // Strip the implicit 1 now sitting at bit 10; keep only the
            // 10 fraction bits, then shift left into the 23-bit f32
            // fraction slot (23 − 10 = 13).
            let frac = (m & 0x03FF) << 13;
            let f32_exp_field = (113 + e_delta) as u32;
            (f32_exp_field, frac)
        } else if exp == 0x1F {
            // Inf/NaN — copy sign, force f32 exp=0xFF, place payload in
            // the top 10 bits of the 23-bit f32 fraction.
            (0xFF, mant << 13)
        } else {
            // Normal — rebias 15 → 127, widen fraction 10 → 23.
            (exp + (127 - 15), mant << 13)
        };

        f32::from_bits((sign << 31) | (f32_exp_field << 23) | f32_frac)
    }

    // -----------------------------------------------------------------------
    // binary32 → binary16
    // -----------------------------------------------------------------------

    /// Narrow a native `f32` to `f16` under round-to-nearest, ties-to-even.
    ///
    /// * ±0, ±∞ preserved bit-for-bit (with the sign carried through).
    /// * NaN preserved as NaN (payload is truncated to the top 10 bits;
    ///   if that truncation would zero the payload we force the least
    ///   significant bit to preserve NaN-ness rather than silently
    ///   promote to infinity).
    /// * `f32` normals whose magnitude exceeds `F16::MAX` overflow to
    ///   `±∞`. Overflow triggered by round-up on the max-finite boundary
    ///   also collapses to `±∞` (mantissa carry propagates into the
    ///   exponent, which we detect after the round step).
    /// * `f32` normals whose magnitude falls in the `f16` subnormal range
    ///   are rounded and re-encoded as subnormals. The round-up carry
    ///   from a max subnormal into the smallest normal is handled
    ///   naturally by the encoding (`0x03FF + 1 = 0x0400`, which is
    ///   exactly `E=1, M=0` — the smallest normal).
    /// * `f32` values below half of `F16`'s smallest positive subnormal
    ///   (i.e. below `2^-25`) round to `±0`. At exactly `2^-25` RNE
    ///   picks the even representable, which is `0`.
    /// * `f32` subnormals are always below the `2^-25` threshold
    ///   (`f32` min normal is `2^-126`), so they collapse to `±0` — no
    ///   special path required.
    pub fn from_f32(value: f32) -> Self {
        let x = value.to_bits();
        let sign = ((x >> 31) & 0x1) as u16;
        let exp = ((x >> 23) & 0xFF) as i32;
        let mant = x & 0x007F_FFFF;

        // Inf / NaN --------------------------------------------------------
        if exp == 0xFF {
            if mant == 0 {
                // Infinity, sign preserved.
                return F16((sign << 15) | 0x7C00);
            }
            // NaN — take the top 10 payload bits. If truncation would
            // zero the payload (very small NaN payloads that live only
            // in the low 13 f32 fraction bits), force the LSB so the
            // result stays a NaN rather than an accidental infinity.
            let mut h_mant = (mant >> 13) as u16;
            if h_mant == 0 {
                h_mant = 1;
            }
            return F16((sign << 15) | 0x7C00 | h_mant);
        }

        // Zero (and f32 subnormals — they all underflow past `2^-25`) ------
        if exp == 0 {
            return F16(sign << 15);
        }

        // Rebias into the f16 exponent slot.
        let new_exp = exp - 127 + 15;

        // Overflow: value ≥ 2^16 — collapse to ±∞ (mantissa doesn't
        // matter, RNE cannot pull it back into the finite range).
        if new_exp >= 0x1F {
            return F16((sign << 15) | 0x7C00);
        }

        // Subnormal / underflow band.
        //
        // A subnormal f16 encodes value `M × 2^-24`, so the target
        // mantissa is the 24-bit `(1.M_f32)` shifted right by
        // `14 − new_exp` (see module-header derivation). At the extreme
        // end (`new_exp ≤ −11`, i.e. `shift ≥ 25`) even the maximum
        // pre-shift mantissa lies strictly below the half-ULP boundary
        // of the smallest f16 subnormal, so RNE picks zero — bail early
        // to avoid the shift-by-too-much branch inside the round step.
        if new_exp <= 0 {
            let shift = (14 - new_exp) as u32;
            if shift >= 25 {
                return F16(sign << 15);
            }
            // Restore the implicit leading 1, producing a 24-bit value.
            let m24 = mant | 0x0080_0000;
            // Round-to-nearest, ties-to-even on the discarded low bits.
            let round_bit = 1u32 << (shift - 1);
            let sticky_mask = round_bit - 1;
            let sticky = m24 & sticky_mask;
            let round = m24 & round_bit;
            let mut m_shift = m24 >> shift;
            let lsb = m_shift & 1;
            if round != 0 && (sticky != 0 || lsb != 0) {
                m_shift += 1;
                // If rounding carries past the subnormal mantissa's
                // 10-bit range, `m_shift` becomes `0x0400`, which is
                // *precisely* the encoding of the smallest normal
                // (E=1, M=0). No fix-up needed.
            }
            return F16((sign << 15) | (m_shift as u16));
        }

        // Normal range — RNE on the low 13 bits of the f32 fraction.
        let round_bit = 1u32 << 12;
        let sticky_mask = round_bit - 1;
        let sticky = mant & sticky_mask;
        let round = mant & round_bit;
        let mut h_mant = mant >> 13;
        let mut h_exp = new_exp as u32;
        let lsb = h_mant & 1;
        if round != 0 && (sticky != 0 || lsb != 0) {
            h_mant += 1;
            if h_mant == 0x0400 {
                // Mantissa carry propagates into the exponent.
                h_mant = 0;
                h_exp += 1;
                if h_exp >= 0x1F {
                    // Round-up pushed the value past `F16::MAX` — ±∞.
                    return F16((sign << 15) | 0x7C00);
                }
            }
        }
        F16((sign << 15) | ((h_exp as u16) << 10) | (h_mant as u16))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- descriptor -------------------------------------------------------

    #[test]
    fn f16_descriptor_reports_stable_shape() {
        assert_eq!(F16_DESCRIPTOR.name, "f16");
        assert_eq!(F16_DESCRIPTOR.bits, 16);
        assert_eq!(F16_DESCRIPTOR.align_bytes, 2);
        // The free constants and the descriptor must not drift apart —
        // any downstream table can pin either without observing a skew.
        assert_eq!(F16_DESCRIPTOR.name, F16_TYPE_NAME);
        assert_eq!(F16_DESCRIPTOR.bits, F16_BITS);
        assert_eq!(F16_DESCRIPTOR.align_bytes, F16_ALIGN_BYTES);
    }

    // ---- ± zero round-trip -----------------------------------------------

    #[test]
    fn f16_zero_round_trip() {
        // +0.0
        let z = F16::from_f32(0.0);
        assert_eq!(z.to_bits(), 0x0000, "+0.0 should encode as 0x0000");
        assert_eq!(z.to_f32().to_bits(), 0.0_f32.to_bits());

        // -0.0 (sign bit set, exp=0, mant=0)
        let nz = F16::from_f32(-0.0);
        assert_eq!(nz.to_bits(), 0x8000, "-0.0 should encode as 0x8000");
        assert_eq!(nz.to_f32().to_bits(), (-0.0_f32).to_bits());
    }

    // ---- 1.0 round-trip --------------------------------------------------

    #[test]
    fn f16_one_round_trip() {
        let one = F16::from_f32(1.0);
        // f16 encoding of 1.0: sign=0, E=15 (biased), M=0.
        assert_eq!(one.to_bits(), 0x3C00, "1.0 should encode as 0x3C00");
        assert_eq!(one.to_f32(), 1.0);

        let neg_one = F16::from_f32(-1.0);
        assert_eq!(neg_one.to_bits(), 0xBC00);
        assert_eq!(neg_one.to_f32(), -1.0);
    }

    // ---- subnormal min ---------------------------------------------------

    #[test]
    fn f16_min_positive_subnormal_round_trip() {
        // Reference value: 2^-24 ≈ 5.96e-8. Representable *exactly* as f32.
        let expected = f32::from_bits(0x33800000); // 2^-24
        let s = F16::MIN_POSITIVE_SUBNORMAL;
        assert_eq!(s.to_bits(), 0x0001);
        assert!(s.is_subnormal());
        assert_eq!(s.to_f32(), expected);

        // Round-trip the other direction — the exact 2^-24 in f32 must
        // land on the smallest positive subnormal on narrowing.
        let round_trip = F16::from_f32(expected);
        assert_eq!(round_trip.to_bits(), 0x0001);

        // Also cover a non-min subnormal (halfway inside the range).
        // f16 encoding 0x0200 = 512 × 2^-24 = 2^-15.
        let mid_sub = F16::from_bits(0x0200);
        assert!(mid_sub.is_subnormal());
        assert_eq!(mid_sub.to_f32(), f32::from_bits(0x38000000)); // 2^-15
        assert_eq!(F16::from_f32(f32::from_bits(0x38000000)).to_bits(), 0x0200);
    }

    // ---- ±inf and NaN preservation --------------------------------------

    #[test]
    fn f16_infinity_preserved_in_both_directions() {
        assert_eq!(F16::from_f32(f32::INFINITY).to_bits(), 0x7C00);
        assert_eq!(F16::from_f32(f32::NEG_INFINITY).to_bits(), 0xFC00);

        assert!(F16::INFINITY.is_infinite());
        assert!(!F16::INFINITY.is_nan());
        assert!(!F16::INFINITY.is_finite());

        assert_eq!(F16::INFINITY.to_f32(), f32::INFINITY);
        assert_eq!(F16::NEG_INFINITY.to_f32(), f32::NEG_INFINITY);
    }

    #[test]
    fn f16_nan_preserved_in_both_directions() {
        // Narrowing: canonical quiet NaN in f32 (0x7FC00000) narrows to a
        // quiet NaN in f16 — top fraction bit set, other payload bits
        // zeroed by the >>13 truncation.
        let f32_qnan = f32::from_bits(0x7FC00000);
        let n = F16::from_f32(f32_qnan);
        assert!(n.is_nan(), "narrowed f32 qNaN should still be NaN");
        assert_eq!(n.to_bits() & 0x7C00, 0x7C00);
        assert_ne!(n.to_bits() & 0x03FF, 0);

        // Widening: F16::NAN widened to f32 must still be a NaN.
        let widened = F16::NAN.to_f32();
        assert!(widened.is_nan());

        // Edge case: an f32 NaN whose payload lives only in the low 13
        // fraction bits (e.g. 0x7F800001). Truncation would zero the
        // top-10 payload window; the encoder must force the LSB so the
        // result stays a NaN rather than becoming +∞.
        let tiny_payload = f32::from_bits(0x7F800001);
        assert!(tiny_payload.is_nan());
        let narrowed = F16::from_f32(tiny_payload);
        assert!(
            narrowed.is_nan(),
            "small-payload f32 NaN must round to a NaN, not infinity (got 0x{:04X})",
            narrowed.to_bits(),
        );

        // Sign propagates on NaN (the sign of a NaN is not semantically
        // meaningful, but the encoder should preserve the bit).
        let neg_nan = f32::from_bits(0xFFC00000);
        let n2 = F16::from_f32(neg_nan);
        assert!(n2.is_nan());
        assert_eq!(n2.to_bits() >> 15, 1);
    }

    // ---- ties-to-even boundary ------------------------------------------

    #[test]
    fn f16_round_ties_to_even() {
        // Halfway case A (even LSB → round down):
        //   1.0 + 2^-11 sits exactly midway between the f16 values
        //   1.0 (0x3C00, LSB=0) and 1.0 + 2^-10 (0x3C01, LSB=1).
        //   RNE picks the even one → 0x3C00.
        // 1.0 in f32 = 0x3F800000; 2^-11 sets fraction bit 12 → 0x1000.
        let half_up_from_one = f32::from_bits(0x3F800000 | 0x1000);
        let r = F16::from_f32(half_up_from_one);
        assert_eq!(
            r.to_bits(),
            0x3C00,
            "halfway between 1.0 and 1.0+2^-10 with even LSB should round to 1.0 (got 0x{:04X})",
            r.to_bits(),
        );

        // Halfway case B (odd LSB → round up):
        //   1.0 + 2^-10 + 2^-11 sits midway between 0x3C01 (LSB=1) and
        //   0x3C02 (LSB=0). RNE picks the even one → 0x3C02.
        let half_up_from_ulp = f32::from_bits(0x3F800000 | 0x2000 | 0x1000);
        let r2 = F16::from_f32(half_up_from_ulp);
        assert_eq!(
            r2.to_bits(),
            0x3C02,
            "halfway between 0x3C01 and 0x3C02 with odd LSB should round up to 0x3C02 (got 0x{:04X})",
            r2.to_bits(),
        );

        // Non-halfway sanity check: slightly past halfway must round up
        // even when the LSB is even.
        let past_half = f32::from_bits(0x3F800000 | 0x1001);
        let r3 = F16::from_f32(past_half);
        assert_eq!(r3.to_bits(), 0x3C01);
    }

    // ---- overflow / underflow edges -------------------------------------

    #[test]
    fn f16_overflow_rounds_to_infinity() {
        // 2^16 exactly — one ULP above F16::MAX's exponent — must
        // collapse to ±∞ under RNE.
        assert_eq!(F16::from_f32(65536.0).to_bits(), 0x7C00);
        assert_eq!(F16::from_f32(-65536.0).to_bits(), 0xFC00);

        // F16::MAX itself round-trips faithfully.
        assert_eq!(F16::from_f32(65504.0).to_bits(), F16::MAX.to_bits());
        assert_eq!(F16::MAX.to_f32(), 65504.0);

        // 65520.0 sits above F16::MAX (65504) and above the halfway
        // point (65520) between F16::MAX and 2^16; RNE picks 2^16 → ∞.
        assert_eq!(F16::from_f32(65520.0).to_bits(), 0x7C00);
    }

    #[test]
    fn f16_underflow_below_half_min_subnormal_rounds_to_zero() {
        // 2^-25 = half the smallest positive subnormal. RNE at halfway
        // between 0 (even) and MIN_POSITIVE_SUBNORMAL (odd) picks zero.
        let half_min = f32::from_bits(0x33000000); // 2^-25
        assert_eq!(F16::from_f32(half_min).to_bits(), 0x0000);

        // Any f32 subnormal is much smaller than 2^-25 → underflow to ±0.
        let f32_subnormal = f32::from_bits(0x0000_0001);
        assert_eq!(F16::from_f32(f32_subnormal).to_bits(), 0x0000);
        assert_eq!(F16::from_f32(-f32_subnormal).to_bits(), 0x8000);
    }

    #[test]
    fn f16_subnormal_round_up_carries_into_smallest_normal() {
        // A value just above the largest subnormal, close enough that
        // RNE lifts it into the smallest normal (E=1, M=0 → 0x0400).
        // Max subnormal = 0x03FF × 2^-24 ≈ 6.0975e-5; smallest normal
        // = 2^-14 ≈ 6.1035e-5. Any f32 exactly halfway between them
        // (0x03FF × 2^-24 + 2^-25) has bit pattern 0x387FC000 in f32:
        //   sign 0, exp 112 (2^-15), fraction 0x7FC000
        // Verified by construction below; RNE lifts it to 0x0400.
        let just_shy = f32::from_bits(0x387FE000);
        let r = F16::from_f32(just_shy);
        assert_eq!(
            r.to_bits(),
            0x0400,
            "subnormal round-up should carry into smallest normal (got 0x{:04X})",
            r.to_bits(),
        );
    }

    #[test]
    fn f16_classification_predicates() {
        assert!(F16::ZERO.is_finite());
        assert!(!F16::ZERO.is_nan());
        assert!(!F16::ZERO.is_infinite());
        assert!(!F16::ZERO.is_subnormal());

        assert!(F16::MIN_POSITIVE_SUBNORMAL.is_subnormal());
        assert!(F16::MIN_POSITIVE_SUBNORMAL.is_finite());

        assert!(!F16::MIN_POSITIVE_NORMAL.is_subnormal());
        assert!(F16::MIN_POSITIVE_NORMAL.is_finite());

        assert!(F16::MAX.is_finite());
        assert!(!F16::MAX.is_subnormal());

        assert!(F16::NAN.is_nan());
        assert!(!F16::NAN.is_infinite());
        assert!(!F16::NAN.is_finite());
    }
}
