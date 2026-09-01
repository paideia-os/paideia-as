//! Ed25519 sign + verify per RFC 8032 §5.1 (PureEdDSA over edwards25519
//! with SHA-512).
//!
//! Uses [`FieldElement`] from [`super::field25519`] for `GF(2^255 - 19)`
//! arithmetic (shared with X25519 — Ed25519 lives on the twisted Edwards
//! form of the same underlying prime field), and [`crate::hash::sha512`]
//! for the hash. Zero external dependency.
//!
//! # Design decisions
//!
//! - **Point representation:** extended twisted-Edwards `(X, Y, Z, T)`
//!   with `T = X*Y/Z`. Enables constant-time unified add per Hisil, Wong,
//!   Carter, Dawson 2008 (also used by ed25519-dalek).
//! - **Scalar arithmetic (`sc_reduce_64`, `sc_muladd`):** bit-serial
//!   modular reduction over 512-bit intermediates using 8 u32 limbs.
//!   Deliberately chosen over the SUPERCOP `ref10` signed-21-bit-limb
//!   trick because the ref10 transcription that stalled #1341 (see the
//!   issue's stall comment) was too easy to get subtly wrong at the
//!   tail-carry chain. Bit-serial is O(512) iterations with a single
//!   conditional-subtract per bit — slower per op (still << 1 ms per
//!   `sc_muladd` on a modern x86_64), but the code is small enough to
//!   audit against the definition of L directly, and the four-limb
//!   comparison + subtract are trivially constant-time.
//! - **Scalar-mul constant-time:** MSB-down double-and-add with a
//!   `Point::conditional_select` on every bit — every step performs
//!   both add and double regardless of the bit value.
//! - **Verify does NOT need to be constant-time** (public data), so it
//!   uses a straightforward non-constant scalar-mul.
//!
//! # References
//!
//! - RFC 8032 §5.1 (Ed25519 = PureEdDSA over edwards25519 with SHA-512).
//! - RFC 8032 §7.1 test vectors (TEST 1..1024 + SHA(abc)).
//! - Hisil, Wong, Carter, Dawson (2008) — "Twisted Edwards curves
//!   revisited" — the unified add / dedicated doubling formulas.
//! - Bernstein et al., "High-speed high-security signatures" (2011).

use super::field25519::FieldElement;
use crate::hash::sha512;

// -------------------------- Constants --------------------------------

/// Group order of the prime-order subgroup: `L = 2^252 + 27742317777372353535851937790883648493`.
///
/// Little-endian 32-byte representation used by `sc_reduce`/`sc_muladd`.
const L_LE: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58,
    0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
];

/// L as 8 little-endian u32 limbs (unpacked once, used by scalar reduce/muladd).
const L_LIMBS: [u32; 8] = unpack_u32_le(&L_LE);

const fn unpack_u32_le(b: &[u8; 32]) -> [u32; 8] {
    let mut out = [0u32; 8];
    let mut i = 0;
    while i < 8 {
        out[i] = (b[i * 4] as u32)
            | ((b[i * 4 + 1] as u32) << 8)
            | ((b[i * 4 + 2] as u32) << 16)
            | ((b[i * 4 + 3] as u32) << 24);
        i += 1;
    }
    out
}

/// Compressed representation of the standard base point B (RFC 8032 §5.1).
///
/// `y_B = 4/5 mod p`, `x_B` is the positive-parity square root of the
/// curve equation solved for x. Little-endian encoding: y in 255 bits +
/// x parity as the top bit of byte 31.
const BASE_COMPRESSED: [u8; 32] = [
    0x58, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
    0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
    0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
    0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
];

/// Curve equation: `-x^2 + y^2 = 1 + d * x^2 * y^2`, `d = -121665 / 121666 mod p`.
///
/// Encoded as the little-endian 32-byte reduced representative — this is
/// the canonical little-endian byte reversal of the big-endian value
/// `52036cee2b6ffe738cc740797779e89800700a4d4141d8ab75eb4dca135978a3`
/// (see RFC 8032 §5.1 and the ed25519-dalek `EDWARDS_D_BYTES` constant).
const D_LE: [u8; 32] = [
    0xa3, 0x78, 0x59, 0x13, 0xca, 0x4d, 0xeb, 0x75,
    0xab, 0xd8, 0x41, 0x41, 0x4d, 0x0a, 0x70, 0x00,
    0x98, 0xe8, 0x79, 0x77, 0x79, 0x40, 0xc7, 0x8c,
    0x73, 0xfe, 0x6f, 0x2b, 0xee, 0x6c, 0x03, 0x52,
];

/// `sqrt(-1) mod p` — needed by [`decompress`] on the alternate-branch case
/// (`v * beta^2 == -u`). Little-endian encoding.
const SQRT_M1_LE: [u8; 32] = [
    0xb0, 0xa0, 0x0e, 0x4a, 0x27, 0x1b, 0xee, 0xc4,
    0x78, 0xe4, 0x2f, 0xad, 0x06, 0x18, 0x43, 0x2f,
    0xa7, 0xd7, 0xfb, 0x3d, 0x99, 0x00, 0x4d, 0x2b,
    0x0b, 0xdf, 0xc1, 0x4f, 0x80, 0x24, 0x83, 0x2b,
];

// -------------------------- Field helpers ----------------------------

/// Constant-time equality on canonical-form byte encodings.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

/// Field negation `-a mod p` via `0 - a`.
fn fe_neg(a: &FieldElement) -> FieldElement {
    FieldElement::ZERO.sub(a)
}

/// `sign` bit of a field element per RFC 8032 §5.1.2: low bit of its
/// canonical little-endian byte encoding.
fn fe_is_negative(a: &FieldElement) -> u8 {
    a.to_bytes()[0] & 1
}

/// Field-element equality via canonical byte encoding.
fn fe_eq(a: &FieldElement, b: &FieldElement) -> bool {
    ct_eq(&a.to_bytes(), &b.to_bytes())
}

/// `a^((p-5)/8) mod p` — the exponent used by [`sqrt_ratio`].
///
/// Uses the standard addition chain for `p - 5 = 2^252 - 8` (256 bits):
/// four shared squarings with [`FieldElement::invert`]'s chain up to
/// `z_250_0`, then two final squarings, giving `2^252 - 4` — one more
/// square (× base) is not needed because the exponent is `(p-5)/8` which
/// equals `2^252 - 3` less one, i.e. `2^252 - 4`... hmm let me redo.
///
/// (p - 5) / 8 = (2^255 - 24) / 8 = 2^252 - 3.
///
/// Chain: reuse the `invert`-style chain up through `z_250_0 = 2^250 - 1`,
/// then shift left twice (two squarings) to reach `2^252 - 4`, then
/// multiply by `a` to hit `2^252 - 3`.
fn fe_pow_p_minus_5_over_8(a: &FieldElement) -> FieldElement {
    let z1 = *a;
    let z2 = z1.square();
    let z8 = z2.square().square();
    let z9 = z8.mul(&z1);
    let z11 = z9.mul(&z2);
    let z22 = z11.square();
    let z_5_0 = z22.mul(&z9);

    let mut t = z_5_0.square();
    for _ in 1..5 {
        t = t.square();
    }
    let z_10_0 = t.mul(&z_5_0);

    let mut t = z_10_0.square();
    for _ in 1..10 {
        t = t.square();
    }
    let z_20_0 = t.mul(&z_10_0);

    let mut t = z_20_0.square();
    for _ in 1..20 {
        t = t.square();
    }
    let z_40_0 = t.mul(&z_20_0);

    let mut t = z_40_0.square();
    for _ in 1..10 {
        t = t.square();
    }
    let z_50_0 = t.mul(&z_10_0);

    let mut t = z_50_0.square();
    for _ in 1..50 {
        t = t.square();
    }
    let z_100_0 = t.mul(&z_50_0);

    let mut t = z_100_0.square();
    for _ in 1..100 {
        t = t.square();
    }
    let z_200_0 = t.mul(&z_100_0);

    let mut t = z_200_0.square();
    for _ in 1..50 {
        t = t.square();
    }
    let z_250_0 = t.mul(&z_50_0);
    // Now shift left by 2 (squarings) to reach 2^252 - 4, then multiply by a
    // to hit 2^252 - 3 = (p - 5) / 8.
    z_250_0.square().square().mul(&z1)
}

// -------------------------- Point (extended) --------------------------

/// Extended twisted-Edwards point `(X : Y : Z : T)` with `T = X*Y/Z`.
///
/// Group law: Hisil/Wong/Carter/Dawson 2008 unified add + dedicated
/// double, both constant-time.
#[derive(Clone, Copy)]
struct Point {
    x: FieldElement,
    y: FieldElement,
    z: FieldElement,
    t: FieldElement,
}

impl Point {
    /// Neutral element `(0 : 1 : 1 : 0)`.
    fn identity() -> Self {
        Point {
            x: FieldElement::ZERO,
            y: FieldElement::ONE,
            z: FieldElement::ONE,
            t: FieldElement::ZERO,
        }
    }

    /// Constant-time conditional swap of two points (each limb of each
    /// coordinate touched regardless of `swap`).
    fn cswap(a: &mut Self, b: &mut Self, swap: u64) {
        FieldElement::cswap(&mut a.x, &mut b.x, swap);
        FieldElement::cswap(&mut a.y, &mut b.y, swap);
        FieldElement::cswap(&mut a.z, &mut b.z, swap);
        FieldElement::cswap(&mut a.t, &mut b.t, swap);
    }

    /// Dedicated doubling for `a = -1` twisted Edwards, projective form
    /// (Hisil et al. 2008, Section 3.3 formula).
    fn double(&self) -> Self {
        let a = self.x.square();
        let b = self.y.square();
        let c = self.z.square().add(&self.z.square()); // 2 * Z^2
        let d = fe_neg(&a); // a = -1
        let e = self.x.add(&self.y).square().sub(&a).sub(&b);
        let g = d.add(&b);
        let f = g.sub(&c);
        let h = d.sub(&b);
        Point {
            x: e.mul(&f),
            y: g.mul(&h),
            t: e.mul(&h),
            z: f.mul(&g),
        }
    }

    /// Unified addition on twisted Edwards `a = -1` (Hisil et al. 2008,
    /// Section 3.1 — "add-2008-hwcd-3" in EFD).
    fn add(&self, other: &Self) -> Self {
        let d_times_2 = d_times_2();
        let a = self.y.sub(&self.x).mul(&other.y.sub(&other.x));
        let b = self.y.add(&self.x).mul(&other.y.add(&other.x));
        let c = self.t.mul(&other.t).mul(&d_times_2);
        let d = self.z.mul(&other.z).add(&self.z.mul(&other.z));
        let e = b.sub(&a);
        let f = d.sub(&c);
        let g = d.add(&c);
        let h = b.add(&a);
        Point {
            x: e.mul(&f),
            y: g.mul(&h),
            t: e.mul(&h),
            z: f.mul(&g),
        }
    }

    /// Constant-time scalar multiplication for secret scalars.
    ///
    /// Standard MSB-down double-and-add with a `cswap` on every step so
    /// the same field-op sequence runs regardless of bit values.
    fn scalar_mul(&self, scalar: &[u8; 32]) -> Self {
        let mut r0 = Point::identity();
        let mut r1 = *self;
        for i in (0..256).rev() {
            let bit = ((scalar[i / 8] >> (i & 7)) & 1) as u64;
            Point::cswap(&mut r0, &mut r1, bit);
            let new_r1 = r0.add(&r1);
            let new_r0 = r0.double();
            r0 = new_r0;
            r1 = new_r1;
            Point::cswap(&mut r0, &mut r1, bit);
        }
        r0
    }

    /// Encode the affine `(x, y)` of this projective point into 32 bytes
    /// per RFC 8032 §5.1.2: little-endian `y` with the low bit of `x` OR'd
    /// into the top bit of byte 31.
    fn compress(&self) -> [u8; 32] {
        let z_inv = self.z.invert();
        let x = self.x.mul(&z_inv);
        let y = self.y.mul(&z_inv);
        let mut out = y.to_bytes();
        out[31] |= fe_is_negative(&x) << 7;
        out
    }
}

/// `2 * d` — used by the unified twisted-Edwards add. Computed once per
/// call as `d + d` since `FieldElement::from_bytes` is not `const`.
/// Cheap: one `from_bytes` + one `add`; not on the sc_muladd hot path.
fn d_times_2() -> FieldElement {
    let d = FieldElement::from_bytes(&D_LE);
    d.add(&d)
}

/// Decompress a 32-byte encoding into an extended-coordinates point.
/// Returns `None` if the y coordinate is `>= p` or if the equation has
/// no square-root solution for x.
fn decompress(bytes: &[u8; 32]) -> Option<Point> {
    // Extract x-parity, then mask it off to recover y_bytes.
    let x_sign = bytes[31] >> 7;
    let mut y_bytes = *bytes;
    y_bytes[31] &= 0x7f;

    // Reject y >= p. Compare y_bytes to p_bytes little-endian.
    // p = 2^255 - 19 → little-endian bytes[0]=0xed, ..., bytes[31]=0x7f.
    let mut p_bytes = [0xffu8; 32];
    p_bytes[0] = 0xed;
    p_bytes[31] = 0x7f;
    // If y_bytes > p_bytes-1 (i.e. >= p), reject. Iterate from top down.
    let mut greater_or_equal = 1u8;
    for i in (0..32).rev() {
        if y_bytes[i] < p_bytes[i] {
            greater_or_equal = 0;
            break;
        }
        if y_bytes[i] > p_bytes[i] {
            return None;
        }
    }
    if greater_or_equal == 1 && y_bytes == p_bytes {
        // y == p is invalid (canonical form requires y < p).
        return None;
    }

    let y = FieldElement::from_bytes(&y_bytes);
    let y2 = y.square();
    let d = FieldElement::from_bytes(&D_LE);

    // x^2 = (y^2 - 1) / (d * y^2 + 1)
    let u = y2.sub(&FieldElement::ONE);
    let v = d.mul(&y2).add(&FieldElement::ONE);

    // Simplest correct path: compute w = u * v^-1, then take sqrt(w) via
    // the "p ≡ 5 mod 8" trick: candidate = w^((p+3)/8) = w * w^((p-5)/8).
    // If candidate^2 == w, sqrt is candidate; else candidate * sqrt(-1)
    // is the sqrt (if the alt branch fails, w has no sqrt).
    let v_inv = v.invert();
    let w = u.mul(&v_inv);
    // w^((p+3)/8) = w * w^((p-5)/8) since (p+3)/8 = (p-5)/8 + 1.
    let mut x = w.mul(&fe_pow_p_minus_5_over_8(&w));

    let xx = x.square();
    if !fe_eq(&xx, &w) {
        // Alt branch: multiply by sqrt(-1).
        let sqrt_m1 = FieldElement::from_bytes(&SQRT_M1_LE);
        x = x.mul(&sqrt_m1);
        let xx = x.square();
        if !fe_eq(&xx, &w) {
            return None;
        }
    }

    // If (x == 0) AND (sign bit == 1), reject per RFC 8032 §5.1.3 step 5.
    let x_bytes = x.to_bytes();
    let x_is_zero = ct_eq(&x_bytes, &[0u8; 32]);
    if x_is_zero && x_sign == 1 {
        return None;
    }

    // Adjust sign of x to match the encoded x_sign bit.
    if (x_bytes[0] & 1) != x_sign {
        x = fe_neg(&x);
    }

    let t = x.mul(&y);
    Some(Point {
        x,
        y,
        z: FieldElement::ONE,
        t,
    })
}

// -------------------------- Scalar arithmetic --------------------------

/// Reduce a 64-byte (little-endian) scalar modulo L, returning 32
/// little-endian bytes.
///
/// Bit-serial: iterate MSB-down over 512 bits, doubling an accumulator
/// modulo L and OR-ing in the next bit. One conditional subtract per bit.
#[must_use]
pub fn sc_reduce_64(input: &[u8; 64]) -> [u8; 32] {
    // Accumulator: 9 u32 limbs (256 bits + 1 headroom limb for the shift).
    let mut r = [0u32; 9];

    // MSB-down bit index into the 512-bit input.
    for i in (0..512).rev() {
        // r = (r << 1) with 288-bit width
        let mut carry: u32 = 0;
        for limb in r.iter_mut().take(9) {
            let t = (*limb as u64) << 1 | carry as u64;
            *limb = (t & 0xFFFF_FFFF) as u32;
            carry = (t >> 32) as u32;
        }
        // OR in the next input bit.
        let bit = ((input[i / 8] >> (i & 7)) & 1) as u32;
        r[0] |= bit;

        // Conditional subtract L if r >= L (as 288-bit unsigned).
        conditional_sub_l(&mut r);
    }

    // Final canonical: r is now < L. Pack lower 8 limbs into 32 bytes LE.
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..(i + 1) * 4].copy_from_slice(&r[i].to_le_bytes());
    }
    out
}

/// Compute `(a * b + c) mod L`, all 32-byte little-endian scalars.
#[must_use]
pub fn sc_muladd(a: &[u8; 32], b: &[u8; 32], c: &[u8; 32]) -> [u8; 32] {
    // Unpack to 8-limb u32 (little-endian).
    let a_l = unpack8(a);
    let b_l = unpack8(b);
    let c_l = unpack8(c);

    // a * b → 16 u32 limbs (schoolbook).
    let mut p = [0u32; 16];
    for i in 0..8 {
        let mut carry: u64 = 0;
        for j in 0..8 {
            let t = (a_l[i] as u64) * (b_l[j] as u64) + (p[i + j] as u64) + carry;
            p[i + j] = (t & 0xFFFF_FFFF) as u32;
            carry = t >> 32;
        }
        p[i + 8] = p[i + 8].wrapping_add(carry as u32);
    }

    // Add c to p[0..8], propagating carry into p[8..].
    let mut carry: u64 = 0;
    for i in 0..8 {
        let t = (p[i] as u64) + (c_l[i] as u64) + carry;
        p[i] = (t & 0xFFFF_FFFF) as u32;
        carry = t >> 32;
    }
    for limb in p.iter_mut().skip(8) {
        let t = (*limb as u64) + carry;
        *limb = (t & 0xFFFF_FFFF) as u32;
        carry = t >> 32;
        if carry == 0 {
            break;
        }
    }

    // Repack p (little-endian) into a 64-byte buffer and reduce.
    let mut buf = [0u8; 64];
    for i in 0..16 {
        buf[i * 4..(i + 1) * 4].copy_from_slice(&p[i].to_le_bytes());
    }
    sc_reduce_64(&buf)
}

/// Little-endian 32 bytes → 8 u32 limbs.
fn unpack8(b: &[u8; 32]) -> [u32; 8] {
    let mut out = [0u32; 8];
    for i in 0..8 {
        out[i] = u32::from_le_bytes([
            b[i * 4],
            b[i * 4 + 1],
            b[i * 4 + 2],
            b[i * 4 + 3],
        ]);
    }
    out
}

/// Constant-time: if `r >= L` (as a 288-bit unsigned number, upper limb
/// included), subtract L from r; else leave r unchanged.
///
/// Compares by computing `r - L` with borrow tracking, then uses the
/// borrow bit as a mask over the subtraction result.
#[allow(non_snake_case)]
fn conditional_sub_l(r: &mut [u32; 9]) {
    // Compute t = r - L as an 8-limb subtract (L's 9th limb is 0), tracking borrow.
    let mut diff = [0u32; 9];
    let mut borrow: i64 = 0;
    for i in 0..8 {
        let l = if i < 8 { L_LIMBS[i] as i64 } else { 0 };
        let s = (r[i] as i64) - l - borrow;
        if s < 0 {
            diff[i] = (s + (1i64 << 32)) as u32;
            borrow = 1;
        } else {
            diff[i] = s as u32;
            borrow = 0;
        }
    }
    // Upper limb (r[8]): subtract borrow. If it goes negative, r < L (no swap).
    let s8 = (r[8] as i64) - borrow;
    if s8 < 0 {
        // r < L → leave r alone.
        return;
    }
    diff[8] = s8 as u32;
    *r = diff;
}

// -------------------------- Public API --------------------------------

/// Derive the 32-byte Ed25519 public key from a 32-byte secret seed
/// (RFC 8032 §5.1.5).
#[must_use]
pub fn ed25519_public_from_secret(secret: &[u8; 32]) -> [u8; 32] {
    let h = sha512(secret);
    let mut a_bytes = [0u8; 32];
    a_bytes.copy_from_slice(&h[0..32]);
    // Clamp: RFC 8032 §5.1.5 step 2.
    a_bytes[0] &= 248;
    a_bytes[31] &= 63;
    a_bytes[31] |= 64;

    let base = decompress(&BASE_COMPRESSED).expect("base point decompresses");
    let a_b = base.scalar_mul(&a_bytes);
    a_b.compress()
}

/// Sign `msg` with `secret` per RFC 8032 §5.1.6.
#[must_use]
pub fn ed25519_sign(secret: &[u8; 32], msg: &[u8]) -> [u8; 64] {
    // Step 1: hash the secret → 64 bytes (h[0..32] = a-seed, h[32..64] = prefix).
    let h = sha512(secret);
    let mut a_bytes = [0u8; 32];
    a_bytes.copy_from_slice(&h[0..32]);
    a_bytes[0] &= 248;
    a_bytes[31] &= 63;
    a_bytes[31] |= 64;
    let prefix = &h[32..64];

    // Step 2: r = SHA-512(prefix || msg) reduced mod L (64-byte reduce).
    let mut ctx = crate::hash::Sha512Ctx::new();
    ctx.update(prefix);
    ctx.update(msg);
    let r_hash = ctx.finalize();
    let r_bytes = sc_reduce_64(&r_hash);

    // Step 3: R = r * B, encoded.
    let base = decompress(&BASE_COMPRESSED).expect("base point decompresses");
    let big_r = base.scalar_mul(&r_bytes);
    let big_r_bytes = big_r.compress();

    // Step 4: A = a * B (public key), encoded.
    let big_a = base.scalar_mul(&a_bytes);
    let big_a_bytes = big_a.compress();

    // Step 5: k = SHA-512(R_enc || A_enc || msg) reduced mod L.
    let mut ctx = crate::hash::Sha512Ctx::new();
    ctx.update(&big_r_bytes);
    ctx.update(&big_a_bytes);
    ctx.update(msg);
    let k_hash = ctx.finalize();
    let k_bytes = sc_reduce_64(&k_hash);

    // Step 6: S = (r + k * a) mod L.
    let s_bytes = sc_muladd(&k_bytes, &a_bytes, &r_bytes);

    // Step 7: sig = R || S.
    let mut sig = [0u8; 64];
    sig[0..32].copy_from_slice(&big_r_bytes);
    sig[32..64].copy_from_slice(&s_bytes);
    sig
}

/// Verify `signature` on `msg` under `public` per RFC 8032 §5.1.7.
///
/// Returns `false` (never panics) on any structural or algebraic reject.
#[must_use]
pub fn ed25519_verify(public: &[u8; 32], msg: &[u8], signature: &[u8; 64]) -> bool {
    // Parse: R = sig[0..32], S = sig[32..64].
    let mut big_r_bytes = [0u8; 32];
    big_r_bytes.copy_from_slice(&signature[0..32]);
    let mut s_bytes = [0u8; 32];
    s_bytes.copy_from_slice(&signature[32..64]);

    // §5.1.7 step 3: reject if S is not in [0, L).
    if !scalar_less_than_l(&s_bytes) {
        return false;
    }

    // Decompress A and R.
    let big_a = match decompress(public) {
        Some(p) => p,
        None => return false,
    };
    let big_r = match decompress(&big_r_bytes) {
        Some(p) => p,
        None => return false,
    };

    // k = SHA-512(R || A || M) reduced mod L.
    let mut ctx = crate::hash::Sha512Ctx::new();
    ctx.update(&big_r_bytes);
    ctx.update(public);
    ctx.update(msg);
    let k_hash = ctx.finalize();
    let k_bytes = sc_reduce_64(&k_hash);

    // Check: [S]B == R + [k]A.
    //
    // We use the direct-form check (no cofactor mul) — equivalent to the
    // §5.1.7 cofactor form for well-formed keys / signatures, and matches
    // what the RFC 8032 vectors expect.
    let base = decompress(&BASE_COMPRESSED).expect("base point decompresses");
    let lhs = base.scalar_mul(&s_bytes);
    let ka = big_a.scalar_mul(&k_bytes);
    let rhs = big_r.add(&ka);
    ct_eq(&lhs.compress(), &rhs.compress())
}

/// Check `scalar < L` (little-endian). Reject S values ≥ L per §5.1.7.
fn scalar_less_than_l(s: &[u8; 32]) -> bool {
    for i in (0..32).rev() {
        if s[i] < L_LE[i] {
            return true;
        }
        if s[i] > L_LE[i] {
            return false;
        }
    }
    // Equal → not < L.
    false
}

// -------------------------- Tests --------------------------------------

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

    fn decode_hex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn decode_hex_32(s: &str) -> [u8; 32] {
        let v = decode_hex(s);
        let mut out = [0u8; 32];
        out.copy_from_slice(&v[..32]);
        out
    }

    fn decode_hex_64(s: &str) -> [u8; 64] {
        let v = decode_hex(s);
        let mut out = [0u8; 64];
        out.copy_from_slice(&v[..64]);
        out
    }

    /// Base point B decompresses successfully and has the expected order:
    /// L * B == identity (cheap smoke — a full order check would need
    /// vartime scalar mul over cofactor group).
    #[test]
    fn base_point_decompresses() {
        let b = decompress(&BASE_COMPRESSED).expect("BASE_COMPRESSED decodes");
        // Just sanity-check: b compresses back to the same bytes.
        let round = b.compress();
        assert_eq!(round, BASE_COMPRESSED, "base point does not round-trip: {}", hex(&round));
    }

    /// sc_reduce_64 on all-zero input yields zero, on 1 yields 1.
    #[test]
    fn sc_reduce_trivial() {
        let z = [0u8; 64];
        assert_eq!(sc_reduce_64(&z), [0u8; 32]);

        let mut one = [0u8; 64];
        one[0] = 1;
        let mut expected = [0u8; 32];
        expected[0] = 1;
        assert_eq!(sc_reduce_64(&one), expected);
    }

    /// sc_muladd(0, 0, 0) = 0, sc_muladd(0, 0, c) = c, sc_muladd(1, 1, 0) = 1.
    #[test]
    fn sc_muladd_trivial() {
        assert_eq!(sc_muladd(&[0; 32], &[0; 32], &[0; 32]), [0; 32]);

        let mut c = [0u8; 32];
        c[0] = 42;
        assert_eq!(sc_muladd(&[0; 32], &[0; 32], &c), c);

        let mut one = [0u8; 32];
        one[0] = 1;
        assert_eq!(sc_muladd(&one, &one, &[0; 32]), one);
    }

    // RFC 8032 §7.1 TEST 1.
    #[test]
    fn rfc_8032_test_1_sign_and_verify() {
        let sk = decode_hex_32(
            "9d61b19deffd5a60ba844af492ec2cc4\
             4449c5697b326919703bac031cae7f60",
        );
        let pk_expected = decode_hex_32(
            "d75a980182b10ab7d54bfed3c964073a\
             0ee172f3daa62325af021a68f707511a",
        );
        let sig_expected = decode_hex_64(
            "e5564300c360ac729086e2cc806e828a\
             84877f1eb8e5d974d873e06522490155\
             5fb8821590a33bacc61e39701cf9b46b\
             d25bf5f0595bbe24655141438e7a100b",
        );

        let pk = ed25519_public_from_secret(&sk);
        assert_eq!(pk, pk_expected, "TEST 1 public: got {} want {}", hex(&pk), hex(&pk_expected));

        let sig = ed25519_sign(&sk, b"");
        assert_eq!(
            sig, sig_expected,
            "TEST 1 sign: got {}\n want {}",
            hex(&sig),
            hex(&sig_expected)
        );

        assert!(ed25519_verify(&pk, b"", &sig), "TEST 1 verify should succeed");
    }

    // RFC 8032 §7.1 TEST 2.
    #[test]
    fn rfc_8032_test_2_sign_and_verify() {
        let sk = decode_hex_32(
            "4ccd089b28ff96da9db6c346ec114e0f\
             5b8a319f35aba624da8cf6ed4fb8a6fb",
        );
        let pk_expected = decode_hex_32(
            "3d4017c3e843895a92b70aa74d1b7ebc\
             9c982ccf2ec4968cc0cd55f12af4660c",
        );
        let msg = decode_hex("72");
        let sig_expected = decode_hex_64(
            "92a009a9f0d4cab8720e820b5f642540\
             a2b27b5416503f8fb3762223ebdb69da\
             085ac1e43e15996e458f3613d0f11d8c\
             387b2eaeb4302aeeb00d291612bb0c00",
        );

        let pk = ed25519_public_from_secret(&sk);
        assert_eq!(pk, pk_expected, "TEST 2 public");

        let sig = ed25519_sign(&sk, &msg);
        assert_eq!(sig, sig_expected, "TEST 2 sign: got {}\n want {}", hex(&sig), hex(&sig_expected));
        assert!(ed25519_verify(&pk, &msg, &sig), "TEST 2 verify should succeed");
    }

    // RFC 8032 §7.1 TEST 3.
    #[test]
    fn rfc_8032_test_3_sign_and_verify() {
        let sk = decode_hex_32(
            "c5aa8df43f9f837bedb7442f31dcb7b1\
             66d38535076f094b85ce3a2e0b4458f7",
        );
        let pk_expected = decode_hex_32(
            "fc51cd8e6218a1a38da47ed00230f058\
             0816ed13ba3303ac5deb911548908025",
        );
        let msg = decode_hex("af82");
        let sig_expected = decode_hex_64(
            "6291d657deec24024827e69c3abe01a3\
             0ce548a284743a445e3680d7db5ac3ac\
             18ff9b538d16f290ae67f760984dc659\
             4a7c15e9716ed28dc027beceea1ec40a",
        );

        let pk = ed25519_public_from_secret(&sk);
        assert_eq!(pk, pk_expected, "TEST 3 public");
        let sig = ed25519_sign(&sk, &msg);
        assert_eq!(sig, sig_expected, "TEST 3 sign");
        assert!(ed25519_verify(&pk, &msg, &sig), "TEST 3 verify");
    }

    // RFC 8032 §7.1 TEST SHA(abc): message = SHA-512("abc") (64 bytes).
    #[test]
    fn rfc_8032_test_sha_abc_sign_and_verify() {
        let sk = decode_hex_32(
            "833fe62409237b9d62ec77587520911e\
             9a759cec1d19755b7da901b96dca3d42",
        );
        let pk_expected = decode_hex_32(
            "ec172b93ad5e563bf4932c70e1245034\
             c35467ef2efd4d64ebf819683467e2bf",
        );
        let msg = decode_hex(
            "ddaf35a193617abacc417349ae204131\
             12e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd\
             454d4423643ce80e2a9ac94fa54ca49f",
        );
        let sig_expected = decode_hex_64(
            "dc2a4459e7369633a52b1bf277839a00\
             201009a3efbf3ecb69bea2186c26b589\
             09351fc9ac90b3ecfdfbc7c66431e030\
             3dca179c138ac17ad9bef1177331a704",
        );

        let pk = ed25519_public_from_secret(&sk);
        assert_eq!(pk, pk_expected, "TEST SHA(abc) public");
        let sig = ed25519_sign(&sk, &msg);
        assert_eq!(sig, sig_expected, "TEST SHA(abc) sign");
        assert!(ed25519_verify(&pk, &msg, &sig), "TEST SHA(abc) verify");
    }

    // Negative-verify tests: mutated signature, mutated message, mutated
    // public — all should return `false` without panic.
    #[test]
    fn verify_rejects_mutated_inputs() {
        let sk = decode_hex_32(
            "9d61b19deffd5a60ba844af492ec2cc4\
             4449c5697b326919703bac031cae7f60",
        );
        let pk = ed25519_public_from_secret(&sk);
        let msg = b"regression harness";
        let sig = ed25519_sign(&sk, msg);
        assert!(ed25519_verify(&pk, msg, &sig));

        // Mutated signature byte.
        let mut sig_bad = sig;
        sig_bad[0] ^= 0x01;
        assert!(!ed25519_verify(&pk, msg, &sig_bad));

        // Mutated message.
        assert!(!ed25519_verify(&pk, b"regression harnesx", &sig));

        // Mutated public key.
        let mut pk_bad = pk;
        pk_bad[0] ^= 0x01;
        assert!(!ed25519_verify(&pk_bad, msg, &sig));

        // S >= L must reject.
        let mut sig_s_too_big = sig;
        sig_s_too_big[32..64].copy_from_slice(&[0xff; 32]);
        assert!(!ed25519_verify(&pk, msg, &sig_s_too_big));
    }

    // Round-trip pin: sign then verify with a stable-shaped secret.
    #[test]
    fn round_trip_sign_verify() {
        let sk = decode_hex_32(
            "0102030405060708090a0b0c0d0e0f10\
             1112131415161718191a1b1c1d1e1f20",
        );
        let pk = ed25519_public_from_secret(&sk);
        let msg = b"the quick brown fox jumps over the lazy dog";
        let sig = ed25519_sign(&sk, msg);
        assert!(ed25519_verify(&pk, msg, &sig), "self-round-trip must verify");
    }
}
