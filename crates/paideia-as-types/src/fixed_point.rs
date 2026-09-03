//! `@fixed_point(bits_int, bits_frac)` type modifier — Wave 0 Batch 2, v0.31 M1.
//!
//! A `FixedPoint` value describes a Qm.n number laid out over a signed or
//! unsigned two's-complement integer of `bits_int + bits_frac` bits. The
//! total width must be one of the supported machine widths: 8, 16, 32, or 64
//! bits. The underlying integer stores `x * 2^bits_frac` where `x` is the
//! represented rational.
//!
//! # Kind
//!
//! At the surface level the modifier appears as
//! `@fixed_point(bits_int, bits_frac) T: type` where `T` is a supporting
//! integer type. The elaborator will thread this type through arithmetic
//! (v0.31 M1-002+); this module supplies the interned descriptor plus the
//! type-preserving arithmetic used by the constant folder and by the
//! runtime helpers.
//!
//! # Arithmetic
//!
//! * `fp_add` / `fp_sub` — require the two operands to share the same
//!   `bits_int`, `bits_frac`, and `signed` flag. The raw sum/difference is
//!   returned in the same shape. Overflowing the operand's declared width
//!   returns `FixedPointError::Overflow` (trap-on-overflow default; wrap
//!   semantics arrive as an opt-in flag in v0.31 M2).
//! * `fp_mul` — accepts any two `FixedPoint` shapes and returns a value
//!   whose descriptor is `(a.bits_int + b.bits_int, a.bits_frac + b.bits_frac)`.
//!   In the canonical same-shape case this collapses to `(2i, 2f)` per
//!   issue #1383. The intermediate uses `i128` so the full product is
//!   representable; the descriptor's own total width must still be a
//!   supported machine width or the operation reports `InvalidWidth`.
//! * `fp_div` — programmer-controlled fractional shift. Given operands of
//!   the same shape `(i, f)` and a shift `s ∈ [0, f]`, the quotient is
//!   `(a.raw << s) / b.raw` and the result descriptor is `(i, f - s)`.
//!
//! # Unblocks
//!
//! G6 color-space transforms — the BT.2020 3×3 matrix arithmetic runs on
//! `@fixed_point(2, 30)` Q2.30 fixed-point through this facility.

use core::fmt;

/// The `@fixed_point(bits_int, bits_frac)` type descriptor.
///
/// The pair `(bits_int, bits_frac)` names a Qm.n encoding. `signed`
/// selects between two's-complement and unsigned storage. The sum
/// `bits_int + bits_frac` is the width of the underlying integer and
/// must be a supported machine width — 8, 16, 32, or 64.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FixedPoint {
    /// Integer bits (whole part). Includes the sign bit when `signed` is
    /// true — the Qm.n convention: a signed Q1.7 fits in 8 bits with
    /// range `[-1, 1 - 2^-7]`.
    pub bits_int: u8,
    /// Fractional bits (below the radix point).
    pub bits_frac: u8,
    /// True for two's-complement storage, false for unsigned.
    pub signed: bool,
}

/// A concrete fixed-point value: descriptor plus raw two's-complement
/// (or unsigned) storage. `raw` is held in an `i128` slot so both signed
/// 64-bit and unsigned 64-bit values round-trip losslessly regardless of
/// the declared machine width — one representation covers every shape
/// this module supports.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FixedPointValue {
    /// Type descriptor.
    pub fp: FixedPoint,
    /// The scaled integer `x * 2^bits_frac`, held in the widening
    /// `i128` slot. For signed descriptors the value is in
    /// `[-2^(total_bits-1), 2^(total_bits-1) - 1]`; for unsigned it is in
    /// `[0, 2^total_bits - 1]`.
    pub raw: i128,
}

/// Errors reported by `FixedPoint` construction and arithmetic.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FixedPointError {
    /// `bits_int + bits_frac` is not one of the supported machine widths.
    InvalidWidth {
        /// The rejected total-bit count.
        total_bits: u32,
    },
    /// Arithmetic exceeded the declared width (trap-on-overflow default).
    Overflow,
    /// `fp_add` / `fp_sub` / `fp_div` require both operands to share the
    /// same `FixedPoint` descriptor.
    ShapeMismatch,
    /// `fp_div` shift exceeds the operand's `bits_frac`.
    ShiftOutOfRange,
    /// Division by zero.
    DivideByZero,
    /// The raw storage does not fit in the declared width (encode-side
    /// range check).
    RawOutOfRange,
}

impl fmt::Display for FixedPointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWidth { total_bits } => {
                write!(
                    f,
                    "@fixed_point total width {total_bits} is not 8, 16, 32, or 64"
                )
            }
            Self::Overflow => f.write_str("@fixed_point arithmetic overflowed the declared width"),
            Self::ShapeMismatch => {
                f.write_str("@fixed_point operands must share the same (bits_int, bits_frac, signed)")
            }
            Self::ShiftOutOfRange => {
                f.write_str("@fixed_point division shift exceeds operand fractional bits")
            }
            Self::DivideByZero => f.write_str("@fixed_point division by zero"),
            Self::RawOutOfRange => {
                f.write_str("@fixed_point raw value does not fit in the declared width")
            }
        }
    }
}

impl std::error::Error for FixedPointError {}

/// Supported machine widths for the underlying integer storage.
const SUPPORTED_WIDTHS: [u8; 4] = [8, 16, 32, 64];

impl FixedPoint {
    /// Construct a `FixedPoint` descriptor, validating that
    /// `bits_int + bits_frac ∈ {8, 16, 32, 64}`.
    pub fn new(bits_int: u8, bits_frac: u8, signed: bool) -> Result<Self, FixedPointError> {
        let total = u32::from(bits_int) + u32::from(bits_frac);
        if !SUPPORTED_WIDTHS.iter().any(|&w| u32::from(w) == total) {
            return Err(FixedPointError::InvalidWidth { total_bits: total });
        }
        Ok(Self {
            bits_int,
            bits_frac,
            signed,
        })
    }

    /// Total storage width in bits (`bits_int + bits_frac`).
    #[must_use]
    pub fn total_bits(self) -> u8 {
        self.bits_int + self.bits_frac
    }

    /// The scaling factor `2^bits_frac` as an `i128` — the multiplier that
    /// maps a rational to its raw storage.
    #[must_use]
    pub fn scale(self) -> i128 {
        1i128 << self.bits_frac
    }

    /// Inclusive lower bound of the raw storage in `i128`.
    #[must_use]
    pub fn raw_min(self) -> i128 {
        if self.signed {
            -(1i128 << (self.total_bits() - 1))
        } else {
            0
        }
    }

    /// Inclusive upper bound of the raw storage in `i128`.
    #[must_use]
    pub fn raw_max(self) -> i128 {
        if self.signed {
            (1i128 << (self.total_bits() - 1)) - 1
        } else {
            (1i128 << self.total_bits()) - 1
        }
    }

    /// Encode a raw scaled integer (`x * 2^bits_frac`) into a
    /// `FixedPointValue`. Range-checks that the raw fits in the declared
    /// width.
    pub fn encode_raw(self, raw: i128) -> Result<FixedPointValue, FixedPointError> {
        if raw < self.raw_min() || raw > self.raw_max() {
            return Err(FixedPointError::RawOutOfRange);
        }
        Ok(FixedPointValue { fp: self, raw })
    }

    /// Decode a `FixedPointValue` into `(numerator, denominator)` where
    /// `denominator == 2^bits_frac`. The rational value is
    /// `numerator / denominator`.
    #[must_use]
    pub fn decode(self, v: FixedPointValue) -> (i128, i128) {
        (v.raw, self.scale())
    }
}

/// Same-shape addition. Returns `FixedPointError::ShapeMismatch` if the
/// two descriptors differ, or `FixedPointError::Overflow` if the sum
/// leaves the declared range.
pub fn fp_add(a: FixedPointValue, b: FixedPointValue) -> Result<FixedPointValue, FixedPointError> {
    if a.fp != b.fp {
        return Err(FixedPointError::ShapeMismatch);
    }
    let sum = a.raw + b.raw;
    a.fp.encode_raw(sum).map_err(|e| match e {
        FixedPointError::RawOutOfRange => FixedPointError::Overflow,
        other => other,
    })
}

/// Same-shape subtraction — same overflow discipline as `fp_add`.
///
/// Kept module-private for now (issue #1383 only re-exports `fp_add` and
/// `fp_mul`); the surface-syntax elaborator (v0.31 M1-002+) will lift the
/// re-export.
#[allow(dead_code)]
pub fn fp_sub(a: FixedPointValue, b: FixedPointValue) -> Result<FixedPointValue, FixedPointError> {
    if a.fp != b.fp {
        return Err(FixedPointError::ShapeMismatch);
    }
    let diff = a.raw - b.raw;
    a.fp.encode_raw(diff).map_err(|e| match e {
        FixedPointError::RawOutOfRange => FixedPointError::Overflow,
        other => other,
    })
}

/// Widening multiplication. The result descriptor is
/// `(a.bits_int + b.bits_int, a.bits_frac + b.bits_frac, a.signed || b.signed)`.
/// The intermediate uses `i128` so the full product is representable; the
/// result descriptor is then validated as a supported machine width.
pub fn fp_mul(a: FixedPointValue, b: FixedPointValue) -> Result<FixedPointValue, FixedPointError> {
    let result_signed = a.fp.signed || b.fp.signed;
    let result_int = a
        .fp
        .bits_int
        .checked_add(b.fp.bits_int)
        .ok_or(FixedPointError::Overflow)?;
    let result_frac = a
        .fp
        .bits_frac
        .checked_add(b.fp.bits_frac)
        .ok_or(FixedPointError::Overflow)?;
    let result_fp = FixedPoint::new(result_int, result_frac, result_signed)?;
    let product = a
        .raw
        .checked_mul(b.raw)
        .ok_or(FixedPointError::Overflow)?;
    result_fp.encode_raw(product).map_err(|e| match e {
        FixedPointError::RawOutOfRange => FixedPointError::Overflow,
        other => other,
    })
}

/// Same-shape division with programmer-controlled fractional shift.
///
/// `shift` names how many fractional bits to *drop* from the result
/// descriptor: the result is `@fixed_point(bits_int, bits_frac - shift,
/// signed)`. Concretely the intermediate is scaled up by `bits_frac -
/// shift` before the integer division — that is what preserves the
/// remaining fractional precision:
///
/// ```text
/// let k = bits_frac - shift;         // result fractional bits
/// quotient = (a.raw << k) / b.raw;   // Q(bits_int, k)
/// ```
///
/// `shift == 0` keeps the full fractional precision of the operand;
/// `shift == bits_frac` collapses the result to integer form (Q(i, 0)).
///
/// Reports `ShapeMismatch` when descriptors differ, `DivideByZero` when
/// `b.raw == 0`, `ShiftOutOfRange` when `shift > bits_frac`, `InvalidWidth`
/// when the resulting `(bits_int, bits_frac - shift)` sum is not a
/// supported machine width, and `Overflow` when the quotient does not fit
/// in the result descriptor's declared range.
///
/// Kept module-private for now — see `fp_sub`.
#[allow(dead_code)]
pub fn fp_div(
    a: FixedPointValue,
    b: FixedPointValue,
    shift: u8,
) -> Result<FixedPointValue, FixedPointError> {
    if a.fp != b.fp {
        return Err(FixedPointError::ShapeMismatch);
    }
    if b.raw == 0 {
        return Err(FixedPointError::DivideByZero);
    }
    if shift > a.fp.bits_frac {
        return Err(FixedPointError::ShiftOutOfRange);
    }
    let result_frac = a.fp.bits_frac - shift;
    let result_fp = FixedPoint::new(a.fp.bits_int, result_frac, a.fp.signed)?;
    // Scale `a.raw` up by 2^result_frac before the integer division so
    // that the quotient carries `result_frac` fractional bits. Uses
    // `checked_mul` (not `checked_shl`) because `i128::checked_shl` does
    // not detect arithmetic overflow when a raw at the edge of an
    // unsigned 64-bit descriptor is shifted past bit 127.
    let scale = 1i128
        .checked_shl(u32::from(result_frac))
        .ok_or(FixedPointError::Overflow)?;
    let dividend = a
        .raw
        .checked_mul(scale)
        .ok_or(FixedPointError::Overflow)?;
    let quotient = dividend / b.raw;
    result_fp.encode_raw(quotient).map_err(|e| match e {
        FixedPointError::RawOutOfRange => FixedPointError::Overflow,
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ---------- construction ------------------------------------------

    #[test]
    fn fixed_point_new_rejects_odd_widths() {
        assert!(matches!(
            FixedPoint::new(3, 4, true),
            Err(FixedPointError::InvalidWidth { total_bits: 7 })
        ));
        assert!(matches!(
            FixedPoint::new(5, 12, false),
            Err(FixedPointError::InvalidWidth { total_bits: 17 })
        ));
    }

    #[test]
    fn fixed_point_new_accepts_supported_widths() {
        for &w in &SUPPORTED_WIDTHS {
            let bits_int = w / 2;
            let bits_frac = w - bits_int;
            assert!(FixedPoint::new(bits_int, bits_frac, true).is_ok());
            assert!(FixedPoint::new(bits_int, bits_frac, false).is_ok());
        }
    }

    #[test]
    fn fixed_point_raw_range_signed() {
        let fp = FixedPoint::new(4, 4, true).unwrap();
        assert_eq!(fp.raw_min(), -128);
        assert_eq!(fp.raw_max(), 127);
    }

    #[test]
    fn fixed_point_raw_range_unsigned() {
        let fp = FixedPoint::new(4, 4, false).unwrap();
        assert_eq!(fp.raw_min(), 0);
        assert_eq!(fp.raw_max(), 255);
    }

    // ---------- encode / decode round-trip ---------------------------

    #[test]
    fn encode_decode_round_trip_zero() {
        let fp = FixedPoint::new(8, 8, true).unwrap();
        let v = fp.encode_raw(0).unwrap();
        let (num, denom) = fp.decode(v);
        assert_eq!(num, 0);
        assert_eq!(denom, 256);
    }

    #[test]
    fn encode_rejects_out_of_range() {
        let fp = FixedPoint::new(4, 4, true).unwrap();
        assert_eq!(fp.encode_raw(128), Err(FixedPointError::RawOutOfRange));
        assert_eq!(fp.encode_raw(-129), Err(FixedPointError::RawOutOfRange));
        let ufp = FixedPoint::new(4, 4, false).unwrap();
        assert_eq!(ufp.encode_raw(-1), Err(FixedPointError::RawOutOfRange));
        assert_eq!(ufp.encode_raw(256), Err(FixedPointError::RawOutOfRange));
    }

    // ---------- add / sub --------------------------------------------

    #[test]
    fn add_saturates_via_overflow_trap() {
        let fp = FixedPoint::new(4, 4, true).unwrap();
        let a = fp.encode_raw(100).unwrap();
        let b = fp.encode_raw(50).unwrap();
        assert_eq!(fp_add(a, b), Err(FixedPointError::Overflow));
    }

    #[test]
    fn add_shape_mismatch_reported() {
        let fp_a = FixedPoint::new(4, 4, true).unwrap();
        let fp_b = FixedPoint::new(8, 8, true).unwrap();
        let a = fp_a.encode_raw(1).unwrap();
        let b = fp_b.encode_raw(1).unwrap();
        assert_eq!(fp_add(a, b), Err(FixedPointError::ShapeMismatch));
    }

    #[test]
    fn add_natural_case() {
        // Q4.4 signed: 1.5 + 0.25 = 1.75 → raw 24 + 4 = 28.
        let fp = FixedPoint::new(4, 4, true).unwrap();
        let a = fp.encode_raw(24).unwrap();
        let b = fp.encode_raw(4).unwrap();
        let s = fp_add(a, b).unwrap();
        assert_eq!(s.raw, 28);
        assert_eq!(s.fp, fp);
    }

    #[test]
    fn sub_underflow_trap() {
        let fp = FixedPoint::new(4, 4, false).unwrap(); // unsigned Q4.4
        let a = fp.encode_raw(3).unwrap();
        let b = fp.encode_raw(4).unwrap();
        assert_eq!(fp_sub(a, b), Err(FixedPointError::Overflow));
    }

    // ---------- mul --------------------------------------------------

    #[test]
    fn mul_widens_to_double_shape() {
        // Q2.6 signed × Q2.6 signed → Q4.12 signed. 0.5 * 0.5 = 0.25.
        let fp = FixedPoint::new(2, 6, true).unwrap();
        let half = fp.encode_raw(32).unwrap(); // 0.5 in Q2.6
        let product = fp_mul(half, half).unwrap();
        assert_eq!(product.fp, FixedPoint::new(4, 12, true).unwrap());
        // 0.25 in Q4.12 is raw 1024.
        assert_eq!(product.raw, 1024);
    }

    #[test]
    fn mul_asymmetric_shapes_combine() {
        // Q2.6 signed × Q1.7 signed → Q3.13 signed (total 16, valid).
        let a_fp = FixedPoint::new(2, 6, true).unwrap();
        let b_fp = FixedPoint::new(1, 7, true).unwrap();
        let a = a_fp.encode_raw(64).unwrap(); // 1.0 in Q2.6
        let b = b_fp.encode_raw(64).unwrap(); // 0.5 in Q1.7
        let product = fp_mul(a, b).unwrap();
        assert_eq!(product.fp, FixedPoint::new(3, 13, true).unwrap());
        assert_eq!(product.raw, 4096); // 0.5 in Q3.13
    }

    #[test]
    fn mul_result_width_must_be_supported() {
        // Q8.8 signed × Q8.8 signed → 16+16 = 32, still supported. Good.
        let fp = FixedPoint::new(8, 8, true).unwrap();
        let a = fp.encode_raw(256).unwrap(); // 1.0
        let b = fp.encode_raw(256).unwrap(); // 1.0
        assert!(fp_mul(a, b).is_ok());
        // Q16.16 signed × Q16.16 signed → 64, still supported.
        let fp32 = FixedPoint::new(16, 16, true).unwrap();
        let one = fp32.encode_raw(65536).unwrap();
        assert!(fp_mul(one, one).is_ok());
        // Q32.32 signed × Q32.32 signed → 128, unsupported.
        let fp64 = FixedPoint::new(32, 32, true).unwrap();
        let one64 = fp64.encode_raw(1i128 << 32).unwrap();
        assert!(matches!(
            fp_mul(one64, one64),
            Err(FixedPointError::InvalidWidth { total_bits: 128 })
        ));
    }

    // ---------- div --------------------------------------------------

    #[test]
    fn div_natural_case_with_shift() {
        // Q4.12 signed. 3.0 / 2.0 = 1.5. Keep all fractional bits (shift=0)
        // → result descriptor still (4, 12).
        let fp = FixedPoint::new(4, 12, true).unwrap();
        let three = fp.encode_raw(3 * 4096).unwrap();
        let two = fp.encode_raw(2 * 4096).unwrap();
        let q = fp_div(three, two, 0).unwrap();
        assert_eq!(q.fp, FixedPoint::new(4, 12, true).unwrap());
        assert_eq!(q.raw, 6144); // 1.5 in Q4.12
    }

    #[test]
    fn div_shift_narrows_fractional_bits() {
        // Q4.12 signed with shift=8 → result descriptor (4, 4) — the total
        // width drops from 16 to 8, both supported. 3.0 / 2.0 = 1.5;
        // Q4.4 representation of 1.5 is raw 24.
        let fp = FixedPoint::new(4, 12, true).unwrap();
        let three = fp.encode_raw(3 * 4096).unwrap();
        let two = fp.encode_raw(2 * 4096).unwrap();
        let q = fp_div(three, two, 8).unwrap();
        assert_eq!(q.fp, FixedPoint::new(4, 4, true).unwrap());
        assert_eq!(q.raw, 24);
    }

    #[test]
    fn div_shift_full_collapses_to_integer() {
        // Q4.12 signed with shift=12 (== bits_frac) → result Q(4, 0),
        // total 4 bits — not a supported machine width. So this
        // reports InvalidWidth, exercising the width guard on the
        // narrow side.
        let fp = FixedPoint::new(4, 12, true).unwrap();
        let a = fp.encode_raw(3 * 4096).unwrap();
        let b = fp.encode_raw(2 * 4096).unwrap();
        assert!(matches!(
            fp_div(a, b, 12),
            Err(FixedPointError::InvalidWidth { total_bits: 4 })
        ));
    }

    #[test]
    fn div_by_zero_reported() {
        let fp = FixedPoint::new(4, 4, true).unwrap();
        let a = fp.encode_raw(1).unwrap();
        let z = fp.encode_raw(0).unwrap();
        assert_eq!(fp_div(a, z, 0), Err(FixedPointError::DivideByZero));
    }

    #[test]
    fn div_shift_out_of_range() {
        let fp = FixedPoint::new(4, 4, true).unwrap();
        let a = fp.encode_raw(1).unwrap();
        let b = fp.encode_raw(1).unwrap();
        assert_eq!(fp_div(a, b, 5), Err(FixedPointError::ShiftOutOfRange));
    }

    // ---------- proptest ---------------------------------------------

    fn shape_strategy() -> impl Strategy<Value = (u8, u8, bool)> {
        prop_oneof![
            (Just(1u8), Just(7u8)),
            (Just(2u8), Just(6u8)),
            (Just(4u8), Just(4u8)),
            (Just(4u8), Just(12u8)),
            (Just(8u8), Just(8u8)),
            (Just(8u8), Just(24u8)),
            (Just(16u8), Just(16u8)),
        ]
        .prop_flat_map(|(i, f)| (Just(i), Just(f), any::<bool>()))
    }

    proptest! {
        /// Encode-then-decode is the identity on the numerator.
        #[test]
        fn prop_encode_decode_round_trip(
            (bits_int, bits_frac, signed) in shape_strategy(),
            raw in any::<i64>(),
        ) {
            let fp = FixedPoint::new(bits_int, bits_frac, signed).unwrap();
            let clipped = i128::from(raw).clamp(fp.raw_min(), fp.raw_max());
            let v = fp.encode_raw(clipped).unwrap();
            let (num, _denom) = fp.decode(v);
            prop_assert_eq!(num, clipped);
        }

        /// Any raw outside the declared range is rejected (saturation is
        /// a caller policy — the type traps by default).
        #[test]
        fn prop_out_of_range_traps(
            (bits_int, bits_frac, signed) in shape_strategy(),
        ) {
            let fp = FixedPoint::new(bits_int, bits_frac, signed).unwrap();
            let above = fp.raw_max() + 1;
            let below = fp.raw_min() - 1;
            prop_assert_eq!(
                fp.encode_raw(above),
                Err(FixedPointError::RawOutOfRange)
            );
            prop_assert_eq!(
                fp.encode_raw(below),
                Err(FixedPointError::RawOutOfRange)
            );
        }

        /// Same-shape addition never returns a value outside the declared
        /// range; either it succeeds inside the range or it traps.
        #[test]
        fn prop_add_traps_or_stays_in_range(
            (bits_int, bits_frac, signed) in shape_strategy(),
            a_raw in any::<i64>(),
            b_raw in any::<i64>(),
        ) {
            let fp = FixedPoint::new(bits_int, bits_frac, signed).unwrap();
            let a = fp.encode_raw(i128::from(a_raw).clamp(fp.raw_min(), fp.raw_max())).unwrap();
            let b = fp.encode_raw(i128::from(b_raw).clamp(fp.raw_min(), fp.raw_max())).unwrap();
            match fp_add(a, b) {
                Ok(v) => {
                    prop_assert!(v.raw >= fp.raw_min());
                    prop_assert!(v.raw <= fp.raw_max());
                    prop_assert_eq!(v.fp, fp);
                }
                Err(e) => prop_assert_eq!(e, FixedPointError::Overflow),
            }
        }

        /// Widening multiplication is exact when the result descriptor is
        /// a supported machine width.
        #[test]
        fn prop_mul_is_exact_when_shape_valid(
            (bits_int, bits_frac, signed) in shape_strategy(),
            a_raw in any::<i32>(),
            b_raw in any::<i32>(),
        ) {
            let fp = FixedPoint::new(bits_int, bits_frac, signed).unwrap();
            // Only exercise shapes whose doubled width is still supported.
            let doubled = u32::from(fp.total_bits()) * 2;
            if !SUPPORTED_WIDTHS.iter().any(|&w| u32::from(w) == doubled) {
                return Ok(());
            }
            let a = fp.encode_raw(
                i128::from(a_raw).clamp(fp.raw_min(), fp.raw_max())
            ).unwrap();
            let b = fp.encode_raw(
                i128::from(b_raw).clamp(fp.raw_min(), fp.raw_max())
            ).unwrap();
            let product = fp_mul(a, b).unwrap();
            prop_assert_eq!(product.raw, a.raw * b.raw);
            prop_assert_eq!(product.fp.bits_int, fp.bits_int * 2);
            prop_assert_eq!(product.fp.bits_frac, fp.bits_frac * 2);
        }
    }
}
