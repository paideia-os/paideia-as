//! Field arithmetic in GF(2^255 - 19).
//!
//! Representation: five unsigned 51-bit limbs held in `u64` (radix 2^51).
//! An element `x` is `x = l[0] + l[1] * 2^51 + l[2] * 2^102 + l[3] * 2^153 + l[4] * 2^204`.
//! Limbs are allowed to grow slightly above 51 bits between operations; the
//! carry-normalisation in [`FieldElement::reduce`] brings them back to
//! `< 2^52` (and after a final reduce, `< 2^51`).
//!
//! This module is shared between X25519 (Montgomery ladder) and Ed25519
//! (Edwards curve arithmetic, filed as #1341) — both operate on the same
//! prime field `p = 2^255 - 19`. Only the additive, subtractive,
//! multiplicative, squaring, inversion, and conditional-swap primitives
//! live here; the curve group law lives in the callers.
//!
//! # Constant-time posture
//!
//! Every operation on secret inputs uses only u64 / u128 add / xor /
//! shift / multiply — no data-dependent branches, no data-dependent
//! memory accesses. In particular, [`FieldElement::cswap`] is written so
//! *every* limb is touched irrespective of the swap bit.
//!
//! # References
//!
//! - RFC 7748 §5 — Curve25519 field arithmetic.
//! - D. J. Bernstein, "Curve25519: new Diffie-Hellman speed records" (2006).

/// One field element in radix 2^51 (five 51-bit limbs).
#[derive(Clone, Copy, Debug)]
pub struct FieldElement(pub [u64; 5]);

/// Low limb of `2^52 - 38` — used as `+ 2*p` counterweight in subtraction.
const P_TIMES_2_LOW: u64 = 0x_f_ffff_ffff_ffda;
/// Middle/high limb of `2^52 - 2` — used as `+ 2*p` counterweight in subtraction.
const P_TIMES_2_HIGH: u64 = 0x_f_ffff_ffff_fffe;

/// Mask keeping the low 51 bits of a limb.
const LOW_51: u64 = (1u64 << 51) - 1;

impl FieldElement {
    /// Additive identity.
    pub const ZERO: Self = FieldElement([0, 0, 0, 0, 0]);

    /// Multiplicative identity.
    pub const ONE: Self = FieldElement([1, 0, 0, 0, 0]);

    /// Field addition — limbwise. Result limbs grow by at most one bit,
    /// so a single [`Self::reduce`] restores the 51-bit invariant.
    #[must_use]
    pub fn add(&self, other: &Self) -> Self {
        FieldElement([
            self.0[0].wrapping_add(other.0[0]),
            self.0[1].wrapping_add(other.0[1]),
            self.0[2].wrapping_add(other.0[2]),
            self.0[3].wrapping_add(other.0[3]),
            self.0[4].wrapping_add(other.0[4]),
        ])
    }

    /// Field subtraction. To avoid negative limbs we add `2*p` (whose
    /// low limb is `2^52 - 38` and whose upper four limbs are
    /// `2^52 - 2`) before subtracting each limb of `other`. Result
    /// limbs are `< 2^53`; [`Self::reduce`] cleans them up.
    #[must_use]
    pub fn sub(&self, other: &Self) -> Self {
        FieldElement([
            self.0[0].wrapping_add(P_TIMES_2_LOW).wrapping_sub(other.0[0]),
            self.0[1].wrapping_add(P_TIMES_2_HIGH).wrapping_sub(other.0[1]),
            self.0[2].wrapping_add(P_TIMES_2_HIGH).wrapping_sub(other.0[2]),
            self.0[3].wrapping_add(P_TIMES_2_HIGH).wrapping_sub(other.0[3]),
            self.0[4].wrapping_add(P_TIMES_2_HIGH).wrapping_sub(other.0[4]),
        ])
    }

    /// Field multiplication modulo `2^255 - 19`.
    ///
    /// Uses schoolbook multiplication with the well-known `x19` reduction
    /// trick: for cross-terms `a[i] * b[j]` with `i + j >= 5`, the product
    /// carries a `2^255` factor, and `2^255 ≡ 19 (mod p)`, so we fold each
    /// such term into the low half of the schedule by multiplying by 19.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Self {
        let a = &self.0;
        let b = &other.0;

        // Precompute 19 * b[i] for the reduction fold.
        let b1_19 = (b[1] as u128) * 19;
        let b2_19 = (b[2] as u128) * 19;
        let b3_19 = (b[3] as u128) * 19;
        let b4_19 = (b[4] as u128) * 19;

        // 128-bit accumulators for each output limb.
        let c0: u128 = (a[0] as u128) * (b[0] as u128)
            + (a[1] as u128) * b4_19
            + (a[2] as u128) * b3_19
            + (a[3] as u128) * b2_19
            + (a[4] as u128) * b1_19;

        let c1: u128 = (a[0] as u128) * (b[1] as u128)
            + (a[1] as u128) * (b[0] as u128)
            + (a[2] as u128) * b4_19
            + (a[3] as u128) * b3_19
            + (a[4] as u128) * b2_19;

        let c2: u128 = (a[0] as u128) * (b[2] as u128)
            + (a[1] as u128) * (b[1] as u128)
            + (a[2] as u128) * (b[0] as u128)
            + (a[3] as u128) * b4_19
            + (a[4] as u128) * b3_19;

        let c3: u128 = (a[0] as u128) * (b[3] as u128)
            + (a[1] as u128) * (b[2] as u128)
            + (a[2] as u128) * (b[1] as u128)
            + (a[3] as u128) * (b[0] as u128)
            + (a[4] as u128) * b4_19;

        let c4: u128 = (a[0] as u128) * (b[4] as u128)
            + (a[1] as u128) * (b[3] as u128)
            + (a[2] as u128) * (b[2] as u128)
            + (a[3] as u128) * (b[1] as u128)
            + (a[4] as u128) * (b[0] as u128);

        propagate_carries(c0, c1, c2, c3, c4)
    }

    /// Field squaring — specialised schoolbook square that leverages
    /// symmetry (halves the number of partial products vs [`Self::mul`]).
    #[must_use]
    pub fn square(&self) -> Self {
        let a = &self.0;
        // Doubled operands for the off-diagonal cross terms.
        let a0_2 = a[0] * 2;
        let a1_2 = a[1] * 2;
        // Off-diagonals from the high half get a 19x fold.
        let a3_19 = a[3] * 19;
        let a4_19 = a[4] * 19;
        // For rows where BOTH multiplicands are already ×19'd (a[i]*a[j] with
        // i,j >= 3 and i+j >= 5), factor the 19 into the "other" side so we
        // don't double-multiply.
        let a4_19_2 = a4_19 * 2;
        let a3_2 = a[3] * 2;

        let c0: u128 = (a[0] as u128) * (a[0] as u128)
            + (a1_2 as u128) * (a4_19 as u128)
            + (a3_2 as u128) * ((a[2] as u128) * 19);

        let c1: u128 = (a0_2 as u128) * (a[1] as u128)
            + (a[2] as u128) * (a4_19 as u128) * 2
            + (a[3] as u128) * (a3_19 as u128);

        let c2: u128 = (a0_2 as u128) * (a[2] as u128)
            + (a[1] as u128) * (a[1] as u128)
            + (a[3] as u128) * (a4_19_2 as u128);

        let c3: u128 = (a0_2 as u128) * (a[3] as u128)
            + (a1_2 as u128) * (a[2] as u128)
            + (a[4] as u128) * (a4_19 as u128);

        let c4: u128 = (a0_2 as u128) * (a[4] as u128)
            + (a1_2 as u128) * (a[3] as u128)
            + (a[2] as u128) * (a[2] as u128);

        propagate_carries(c0, c1, c2, c3, c4)
    }

    /// Multiply by the Montgomery-ladder constant `a24 = (A - 2) / 4 = 121665`
    /// (RFC 7748 §5 — `A = 486662` is the curve equation coefficient).
    ///
    /// Specialised because the caller invokes it once per ladder step and
    /// the generic 5×5 multiplier would be wasteful for a 17-bit constant.
    #[must_use]
    pub fn mul_a24(&self) -> Self {
        let a = &self.0;
        let c0 = (a[0] as u128) * 121665;
        let c1 = (a[1] as u128) * 121665;
        let c2 = (a[2] as u128) * 121665;
        let c3 = (a[3] as u128) * 121665;
        let c4 = (a[4] as u128) * 121665;
        propagate_carries(c0, c1, c2, c3, c4)
    }

    /// Constant-time conditional swap: swaps `a` and `b` iff `swap == 1`.
    /// `swap` MUST be 0 or 1 (higher bits are ignored, but callers should
    /// pass the raw bit for clarity).
    ///
    /// The XOR-based construction touches every limb regardless of the
    /// swap value, so cache-timing side-channels cannot distinguish the
    /// two branches.
    pub fn cswap(a: &mut Self, b: &mut Self, swap: u64) {
        let mask = 0u64.wrapping_sub(swap & 1);
        for i in 0..5 {
            let t = mask & (a.0[i] ^ b.0[i]);
            a.0[i] ^= t;
            b.0[i] ^= t;
        }
    }

    /// One carry-normalisation pass — brings each limb below 2^52.
    /// Callers usually don't need this directly; it exists so tests can
    /// pin the invariant.
    #[must_use]
    pub fn reduce(&self) -> Self {
        let mut r = self.0;
        let mut carry: u64;

        carry = r[0] >> 51;
        r[0] &= LOW_51;
        r[1] = r[1].wrapping_add(carry);

        carry = r[1] >> 51;
        r[1] &= LOW_51;
        r[2] = r[2].wrapping_add(carry);

        carry = r[2] >> 51;
        r[2] &= LOW_51;
        r[3] = r[3].wrapping_add(carry);

        carry = r[3] >> 51;
        r[3] &= LOW_51;
        r[4] = r[4].wrapping_add(carry);

        carry = r[4] >> 51;
        r[4] &= LOW_51;
        r[0] = r[0].wrapping_add(carry.wrapping_mul(19));

        // One more pass in case limb 0 overflowed.
        carry = r[0] >> 51;
        r[0] &= LOW_51;
        r[1] = r[1].wrapping_add(carry);

        FieldElement(r)
    }

    /// Field inversion via Fermat's little theorem: `a^(p-2) mod p`.
    ///
    /// Uses the standard addition chain for `p - 2` that costs 254
    /// squarings + 11 multiplications (see e.g. Bernstein's `curve25519-donna`
    /// notes). Constant-time — no data-dependent branches.
    #[must_use]
    pub fn invert(&self) -> Self {
        // Addition chain based on the exponent p - 2 = 2^255 - 21.
        let z1 = *self;
        let z2 = z1.square(); // 2
        let z8 = z2.square().square(); // 8 = 2 * 4
        let z9 = z8.mul(&z1); // 9
        let z11 = z9.mul(&z2); // 11
        let z22 = z11.square(); // 2^5 - 10
        let z_5_0 = z22.mul(&z9); // 2^5 - 1

        // 2^10 - 2^5
        let mut t = z_5_0.square();
        for _ in 1..5 {
            t = t.square();
        }
        let z_10_0 = t.mul(&z_5_0); // 2^10 - 1

        // 2^20 - 2^10
        let mut t = z_10_0.square();
        for _ in 1..10 {
            t = t.square();
        }
        let z_20_0 = t.mul(&z_10_0); // 2^20 - 1

        // 2^40 - 2^20
        let mut t = z_20_0.square();
        for _ in 1..20 {
            t = t.square();
        }
        let z_40_0 = t.mul(&z_20_0); // 2^40 - 1

        // 2^50 - 2^10
        let mut t = z_40_0.square();
        for _ in 1..10 {
            t = t.square();
        }
        let z_50_0 = t.mul(&z_10_0); // 2^50 - 1

        // 2^100 - 2^50
        let mut t = z_50_0.square();
        for _ in 1..50 {
            t = t.square();
        }
        let z_100_0 = t.mul(&z_50_0); // 2^100 - 1

        // 2^200 - 2^100
        let mut t = z_100_0.square();
        for _ in 1..100 {
            t = t.square();
        }
        let z_200_0 = t.mul(&z_100_0); // 2^200 - 1

        // 2^250 - 2^50
        let mut t = z_200_0.square();
        for _ in 1..50 {
            t = t.square();
        }
        let z_250_0 = t.mul(&z_50_0); // 2^250 - 1

        // 2^255 - 2^5
        let mut t = z_250_0.square();
        for _ in 1..5 {
            t = t.square();
        }
        // 2^255 - 21 == p - 2
        t.mul(&z11)
    }

    /// Decode 32 little-endian bytes into a field element per RFC 7748 §5
    /// `decodeUCoordinate`. The high bit of the last byte is masked
    /// (RFC 7748 requirement for Curve25519).
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        // Read as 64-bit little-endian words and slice into 51-bit limbs.
        let load8 = |i: usize| -> u64 {
            u64::from_le_bytes([
                bytes[i],
                bytes[i + 1],
                bytes[i + 2],
                bytes[i + 3],
                bytes[i + 4],
                bytes[i + 5],
                bytes[i + 6],
                bytes[i + 7],
            ])
        };

        // Slice the little-endian byte stream into 51-bit limbs.
        // RFC 7748 §5 also masks the topmost bit of the 256-bit value
        // (bit 255 = bit 7 of bytes[31]). That bit lands in bit 51 of
        // `l4` before the `& LOW_51` mask, so the mask alone clears it —
        // no separate step needed.
        let l0 = load8(0) & LOW_51;
        let l1 = (load8(6) >> 3) & LOW_51;
        let l2 = (load8(12) >> 6) & LOW_51;
        let l3 = (load8(19) >> 1) & LOW_51;
        let l4 = (load8(24) >> 12) & LOW_51;

        FieldElement([l0, l1, l2, l3, l4])
    }

    /// Encode a field element to 32 little-endian bytes, fully reduced
    /// modulo `p`. Constant-time (touches every byte regardless of value).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 32] {
        // Full carry-normalise into canonical form.
        let r = self.reduce();
        let mut l = r.0;

        // Conditional subtract p — add 19 to l[0], propagate carries, then
        // check whether the result exceeded 2^255. If it did, the
        // "add 19" version is the canonical form; else the original.
        // Simpler equivalent: run through the carry chain twice, then
        // conditionally subtract p.

        // Add 19 (equivalent to +2^255 mod p, exposes whether l >= p).
        let mut q = (l[0] + 19) >> 51;
        q = (l[1] + q) >> 51;
        q = (l[2] + q) >> 51;
        q = (l[3] + q) >> 51;
        q = (l[4] + q) >> 51;
        // q is 1 iff l >= p, so add 19*q to l[0] and mask.
        l[0] = l[0].wrapping_add(19u64.wrapping_mul(q));
        l[1] = l[1].wrapping_add(l[0] >> 51);
        l[0] &= LOW_51;
        l[2] = l[2].wrapping_add(l[1] >> 51);
        l[1] &= LOW_51;
        l[3] = l[3].wrapping_add(l[2] >> 51);
        l[2] &= LOW_51;
        l[4] = l[4].wrapping_add(l[3] >> 51);
        l[3] &= LOW_51;
        l[4] &= LOW_51;

        // Repack 5 * 51 = 255 bits into 32 bytes little-endian. Each limb
        // straddles a byte boundary because 51 is not a multiple of 8:
        //   l[0]: bits   0..50   → bytes 0..6 (byte 6 also holds l[1]'s low 3 bits)
        //   l[1]: bits  51..101  → bytes 6..12 (byte 12 also holds l[2]'s low 6 bits)
        //   l[2]: bits 102..152  → bytes 12..19 (byte 19 also holds l[3]'s low 1 bit)
        //   l[3]: bits 153..203  → bytes 19..25 (byte 25 also holds l[4]'s low 4 bits)
        //   l[4]: bits 204..254  → bytes 25..31 (byte 31's high bit stays clear)
        let mut out = [0u8; 32];
        out[0] = l[0] as u8;
        out[1] = (l[0] >> 8) as u8;
        out[2] = (l[0] >> 16) as u8;
        out[3] = (l[0] >> 24) as u8;
        out[4] = (l[0] >> 32) as u8;
        out[5] = (l[0] >> 40) as u8;
        out[6] = ((l[0] >> 48) | (l[1] << 3)) as u8;
        out[7] = (l[1] >> 5) as u8;
        out[8] = (l[1] >> 13) as u8;
        out[9] = (l[1] >> 21) as u8;
        out[10] = (l[1] >> 29) as u8;
        out[11] = (l[1] >> 37) as u8;
        out[12] = ((l[1] >> 45) | (l[2] << 6)) as u8;
        out[13] = (l[2] >> 2) as u8;
        out[14] = (l[2] >> 10) as u8;
        out[15] = (l[2] >> 18) as u8;
        out[16] = (l[2] >> 26) as u8;
        out[17] = (l[2] >> 34) as u8;
        out[18] = (l[2] >> 42) as u8;
        out[19] = ((l[2] >> 50) | (l[3] << 1)) as u8;
        out[20] = (l[3] >> 7) as u8;
        out[21] = (l[3] >> 15) as u8;
        out[22] = (l[3] >> 23) as u8;
        out[23] = (l[3] >> 31) as u8;
        out[24] = (l[3] >> 39) as u8;
        out[25] = ((l[3] >> 47) | (l[4] << 4)) as u8;
        out[26] = (l[4] >> 4) as u8;
        out[27] = (l[4] >> 12) as u8;
        out[28] = (l[4] >> 20) as u8;
        out[29] = (l[4] >> 28) as u8;
        out[30] = (l[4] >> 36) as u8;
        out[31] = (l[4] >> 44) as u8;

        out
    }
}

/// Common carry-propagation tail shared by [`FieldElement::mul`],
/// [`FieldElement::square`], and [`FieldElement::mul_a24`].
///
/// Takes five 128-bit accumulators, propagates carries, folds the
/// resulting overflow back into limb 0 via the `19x` reduction
/// (`2^255 ≡ 19 (mod p)`), and returns a valid 5-limb representation.
fn propagate_carries(c0: u128, c1: u128, c2: u128, c3: u128, c4: u128) -> FieldElement {
    let mut r = [0u64; 5];
    r[0] = (c0 as u64) & LOW_51;
    let carry = (c0 >> 51) as u64;

    let t1 = c1 + carry as u128;
    r[1] = (t1 as u64) & LOW_51;
    let carry = (t1 >> 51) as u64;

    let t2 = c2 + carry as u128;
    r[2] = (t2 as u64) & LOW_51;
    let carry = (t2 >> 51) as u64;

    let t3 = c3 + carry as u128;
    r[3] = (t3 as u64) & LOW_51;
    let carry = (t3 >> 51) as u64;

    let t4 = c4 + carry as u128;
    r[4] = (t4 as u64) & LOW_51;
    let carry = (t4 >> 51) as u64;

    r[0] = r[0].wrapping_add(carry.wrapping_mul(19));

    // Final carry pass on limb 0 (may have overflowed 51 bits after the fold).
    let carry = r[0] >> 51;
    r[0] &= LOW_51;
    r[1] = r[1].wrapping_add(carry);

    FieldElement(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }

    fn decode_hex_le(s: &str) -> [u8; 32] {
        let s = s.replace(' ', "");
        let mut out = [0u8; 32];
        for i in 0..32 {
            let hi = u8::from_str_radix(&s[i * 2..i * 2 + 1], 16).expect("hex");
            let lo = u8::from_str_radix(&s[i * 2 + 1..i * 2 + 2], 16).expect("hex");
            out[i] = (hi << 4) | lo;
        }
        out
    }

    #[test]
    fn one_times_one_is_one() {
        let a = FieldElement::ONE.mul(&FieldElement::ONE);
        let b = a.to_bytes();
        let mut expected = [0u8; 32];
        expected[0] = 1;
        assert_eq!(b, expected);
    }

    #[test]
    fn add_then_sub_recovers() {
        let a = FieldElement::from_bytes(&decode_hex_le(
            "0900000000000000000000000000000000000000000000000000000000000000",
        ));
        let b = FieldElement::from_bytes(&decode_hex_le(
            "1100000000000000000000000000000000000000000000000000000000000000",
        ));
        let s = a.add(&b);
        let r = s.sub(&b);
        assert_eq!(r.to_bytes(), a.to_bytes());
    }

    #[test]
    fn invert_times_self_is_one() {
        // Non-trivial element to avoid a trivial identity.
        let a = FieldElement::from_bytes(&decode_hex_le(
            "a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4",
        ));
        let inv = a.invert();
        let prod = a.mul(&inv).to_bytes();
        let mut expected = [0u8; 32];
        expected[0] = 1;
        assert_eq!(prod, expected, "a * a^-1 != 1; got {}", hex(&prod));
    }

    #[test]
    fn cswap_swaps_only_when_bit_set() {
        let mut x = FieldElement([1, 2, 3, 4, 5]);
        let mut y = FieldElement([10, 20, 30, 40, 50]);

        FieldElement::cswap(&mut x, &mut y, 0);
        assert_eq!(x.0, [1, 2, 3, 4, 5]);
        assert_eq!(y.0, [10, 20, 30, 40, 50]);

        FieldElement::cswap(&mut x, &mut y, 1);
        assert_eq!(x.0, [10, 20, 30, 40, 50]);
        assert_eq!(y.0, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn from_bytes_round_trip() {
        let raw = decode_hex_le(
            "e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c",
        );
        let fe = FieldElement::from_bytes(&raw);
        let out = fe.to_bytes();
        // RFC 7748 §5 masks the top bit of the input — so byte 31 may
        // differ in its high bit. Compare with the masked expectation.
        let mut expected = raw;
        expected[31] &= 0x7f;
        assert_eq!(out, expected);
    }

    #[test]
    fn mul_a24_matches_generic_mul() {
        let a = FieldElement::from_bytes(&decode_hex_le(
            "a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4",
        ));
        let via_specialised = a.mul_a24();
        let mut c121665 = [0u8; 32];
        c121665[0] = (121665u64 & 0xff) as u8;
        c121665[1] = ((121665u64 >> 8) & 0xff) as u8;
        c121665[2] = ((121665u64 >> 16) & 0xff) as u8;
        let via_generic = a.mul(&FieldElement::from_bytes(&c121665));
        assert_eq!(via_specialised.to_bytes(), via_generic.to_bytes());
    }
}
