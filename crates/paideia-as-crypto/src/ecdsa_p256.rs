//! ECDSA-P256 sign + verify — classical bridge for user-space TLS.
//!
//! **Framing tension**: ECDSA-P256 is CLASSICAL crypto in a project whose
//! primary signature intrinsic (ML-DSA-65) is post-quantum-first. This is
//! a COMPATIBILITY CONCESSION, not a retreat from Pillar 6.
//!
//! The classical bridge is kept ALONGSIDE — not in place of — the PQ
//! posture, until either:
//!   (a) public CA infrastructure adopts PQ-hybrid signing widely enough
//!       that a real Internet HTTPS handshake no longer needs classical
//!       verification, or
//!   (b) the paideia-os stack terminates connections against only pinned
//!       trust roots (KIND_TLS_TRUST — landed at R100-PREP-001), rooted
//!       on ChaCha20-Poly1305 + Ed25519 rather than X.509.
//!
//! # References
//!
//! - RFC 6090 — Fundamental Elliptic Curve Cryptography Algorithms.
//! - RFC 6979 — Deterministic Usage of the Digital Signature Algorithm
//!   (DSA) and Elliptic Curve Digital Signature Algorithm (ECDSA).
//! - FIPS 186-4 §6 — Digital Signature Standard: ECDSA.
//! - SEC 2 §2.4.2 — Recommended Elliptic Curve Domain Parameters
//!   (secp256r1 = P-256).
//! - `paideia-os#2098` — this filing's source (cross-repo escalation
//!   from paideia-os R97.M4-001).
//!
//! # Implementation notes
//!
//! - Field arithmetic: 8× u32 little-endian limbs, schoolbook multiply
//!   into 16 limbs, bit-serial reduction 512 → 256 bits per the same
//!   pattern as [`crate::curve::ed25519::sc_reduce_64`]. Slower than a
//!   Solinas-form fast reduction (Barrett / Montgomery would also
//!   work), but easy to audit against the definition of the modulus
//!   directly.
//! - Point representation: Jacobian projective `(X:Y:Z)` with
//!   `x = X/Z^2, y = Y/Z^3`. Identity encoded as `Z == 0`.
//! - Group law: standard Jacobian formulas with the `a = -3`
//!   optimisation for doubling (Cohen, "A Course in Computational
//!   Algebraic Number Theory", §7.2).
//! - Scalar multiply: constant-time MSB-down double-and-add with
//!   [`Point::cswap`] on every step, so both the verify (public data)
//!   and sign (secret scalar) paths execute the same field-op
//!   sequence regardless of scalar bit values. The `verify` path
//!   is public data — a variable-time double-scalar-multiplication
//!   (Shamir's trick) is a well-known follow-up optimisation.
//! - Modular inverse: Fermat's little theorem (`a^(m-2) mod m`).
//!   Slower than extended Euclidean but deterministic and
//!   side-channel-neutral because the exponent is a public curve
//!   constant.
//! - RFC 6979 deterministic nonce: HMAC-SHA-256 based per §3.2. The
//!   backing HMAC lives in [`crate::kdf::hmac_sha256`].

use crate::hash::sha256;
use crate::kdf::hmac_sha256;

// =====================================================================
// Curve parameters (P-256 / secp256r1)
//
// All 256-bit values are stored as 8 little-endian u32 limbs — limb[0]
// is the least-significant 32 bits. Big-endian byte encodings (the
// standard on-the-wire form) are converted at the FFI seam via
// [`bytes_be_to_limbs`] / [`limbs_to_bytes_be`].
// =====================================================================

/// Field modulus `p = 2^256 - 2^224 + 2^192 + 2^96 - 1` (FIPS 186-4 D.1.2.3).
///
/// Big-endian hex: `FFFFFFFF 00000001 00000000 00000000 00000000
/// FFFFFFFF FFFFFFFF FFFFFFFF`.
const P: [u32; 8] = [
    0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0x00000000,
    0x00000000, 0x00000000, 0x00000001, 0xFFFFFFFF,
];

/// Group order `n` (SEC 2 §2.4.2 / FIPS 186-4 D.1.2.3).
///
/// Big-endian hex: `FFFFFFFF 00000000 FFFFFFFF FFFFFFFF
/// BCE6FAAD A7179E84 F3B9CAC2 FC632551`.
const N: [u32; 8] = [
    0xFC632551, 0xF3B9CAC2, 0xA7179E84, 0xBCE6FAAD,
    0xFFFFFFFF, 0xFFFFFFFF, 0x00000000, 0xFFFFFFFF,
];

/// Curve `b` constant (FIPS 186-4 D.1.2.3 / SEC 2 §2.4.2).
///
/// Big-endian hex: `5AC635D8 AA3A93E7 B3EBBD55 769886BC
/// 651D06B0 CC53B0F6 3BCE3C3E 27D2604B`.
const B_COEFF: [u32; 8] = [
    0x27D2604B, 0x3BCE3C3E, 0xCC53B0F6, 0x651D06B0,
    0x769886BC, 0xB3EBBD55, 0xAA3A93E7, 0x5AC635D8,
];

/// Base-point `x` coordinate `Gx` (FIPS 186-4 D.1.2.3 / SEC 2 §2.4.2).
///
/// Big-endian hex: `6B17D1F2 E12C4247 F8BCE6E5 63A440F2
/// 77037D81 2DEB33A0 F4A13945 D898C296`.
const GX: [u32; 8] = [
    0xD898C296, 0xF4A13945, 0x2DEB33A0, 0x77037D81,
    0x63A440F2, 0xF8BCE6E5, 0xE12C4247, 0x6B17D1F2,
];

/// Base-point `y` coordinate `Gy` (FIPS 186-4 D.1.2.3 / SEC 2 §2.4.2).
///
/// Big-endian hex: `4FE342E2 FE1A7F9B 8EE7EB4A 7C0F9E16
/// 2BCE3357 6B315ECE CBB64068 37BF51F5`.
const GY: [u32; 8] = [
    0x37BF51F5, 0xCBB64068, 0x6B315ECE, 0x2BCE3357,
    0x7C0F9E16, 0x8EE7EB4A, 0xFE1A7F9B, 0x4FE342E2,
];

/// `p - 2`, used as the Fermat-inverse exponent in [`Fp::inv`].
const P_MINUS_2: [u32; 8] = [
    0xFFFFFFFD, 0xFFFFFFFF, 0xFFFFFFFF, 0x00000000,
    0x00000000, 0x00000000, 0x00000001, 0xFFFFFFFF,
];

/// `n - 2`, used as the Fermat-inverse exponent in [`Fn_::inv`].
const N_MINUS_2: [u32; 8] = [
    0xFC63254F, 0xF3B9CAC2, 0xA7179E84, 0xBCE6FAAD,
    0xFFFFFFFF, 0xFFFFFFFF, 0x00000000, 0xFFFFFFFF,
];

/// Additive identity as an 8-limb value.
const ZERO_LIMBS: [u32; 8] = [0; 8];

/// Multiplicative identity as an 8-limb value.
const ONE_LIMBS: [u32; 8] = [1, 0, 0, 0, 0, 0, 0, 0];

// =====================================================================
// Byte <-> limb conversion
// =====================================================================

/// Convert a 32-byte big-endian encoding (the on-the-wire form of a
/// P-256 field element or scalar) into 8 little-endian u32 limbs.
///
/// Big-endian is the standard external encoding (X.509, ANSI X9.62,
/// TLS wire format). Little-endian limbs are the internal storage
/// convention.
fn bytes_be_to_limbs(b: &[u8; 32]) -> [u32; 8] {
    let mut out = [0u32; 8];
    for i in 0..8 {
        // Limb i holds bits (32*i)..(32*(i+1)) of the number.
        // In a big-endian byte array those bits live at bytes
        // [32 - 4*(i+1) .. 32 - 4*i].
        let base = 32 - 4 * (i + 1);
        out[i] = u32::from_be_bytes([b[base], b[base + 1], b[base + 2], b[base + 3]]);
    }
    out
}

/// Convert 8 little-endian u32 limbs into a 32-byte big-endian
/// encoding — inverse of [`bytes_be_to_limbs`].
fn limbs_to_bytes_be(l: &[u32; 8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..8 {
        let base = 32 - 4 * (i + 1);
        out[base..base + 4].copy_from_slice(&l[i].to_be_bytes());
    }
    out
}

// =====================================================================
// Comparisons (constant-time not required — inputs are public curve
// constants or already-canonical outputs of the mod-reducer).
// =====================================================================

/// Return -1 if `a < b`, 0 if equal, 1 if `a > b`. Little-endian limb
/// comparison walked from the most-significant limb downward.
fn cmp_limbs(a: &[u32; 8], b: &[u32; 8]) -> i32 {
    for i in (0..8).rev() {
        if a[i] < b[i] {
            return -1;
        }
        if a[i] > b[i] {
            return 1;
        }
    }
    0
}

/// `a == 0` on the 8-limb representation.
fn is_zero_limbs(a: &[u32; 8]) -> bool {
    let mut acc = 0u32;
    for limb in a.iter() {
        acc |= *limb;
    }
    acc == 0
}

/// `a == b` on the 8-limb representation.
fn eq_limbs(a: &[u32; 8], b: &[u32; 8]) -> bool {
    let mut acc = 0u32;
    for i in 0..8 {
        acc |= a[i] ^ b[i];
    }
    acc == 0
}

// =====================================================================
// Generic modular arithmetic (any modulus m ≤ 2^256 with m > 0).
//
// Callers are responsible for ensuring the operand-range invariant
// `0 ≤ a, b < m` on every input to `add_mod` / `sub_mod`, and the
// `0 ≤ a < m` invariant on `pow_mod`. `mul_mod` accepts any 8-limb
// input; the schoolbook-then-reduce path reduces from 512 bits down
// to the canonical `[0, m)` range regardless.
// =====================================================================

/// `(a + b) mod m` — 9-limb add followed by a single conditional
/// subtract of `m`. Assumes `a, b < m`, so the sum is < `2m` and
/// fits in one extra bit.
fn add_mod(a: &[u32; 8], b: &[u32; 8], m: &[u32; 8]) -> [u32; 8] {
    let mut r = [0u32; 9];
    let mut carry: u64 = 0;
    for i in 0..8 {
        let t = (a[i] as u64) + (b[i] as u64) + carry;
        r[i] = t as u32;
        carry = t >> 32;
    }
    r[8] = carry as u32;
    conditional_sub_modulus(&mut r, m);
    let mut out = [0u32; 8];
    out.copy_from_slice(&r[..8]);
    out
}

/// `(a - b) mod m` — 8-limb subtract; if it borrows, add `m` back
/// once. Assumes `a, b < m`.
fn sub_mod(a: &[u32; 8], b: &[u32; 8], m: &[u32; 8]) -> [u32; 8] {
    let mut diff = [0u32; 8];
    let mut borrow: i64 = 0;
    for i in 0..8 {
        let t = (a[i] as i64) - (b[i] as i64) - borrow;
        if t < 0 {
            diff[i] = (t + (1i64 << 32)) as u32;
            borrow = 1;
        } else {
            diff[i] = t as u32;
            borrow = 0;
        }
    }
    if borrow != 0 {
        // Underflow — add m to bring the result back into [0, m).
        let mut carry: u64 = 0;
        for i in 0..8 {
            let t = (diff[i] as u64) + (m[i] as u64) + carry;
            diff[i] = t as u32;
            carry = t >> 32;
        }
        let _ = carry;
    }
    diff
}

/// `(a * b) mod m` — schoolbook 256×256 → 512 bits, then bit-serial
/// reduce 512 → 256. Mirrors the pattern used by
/// [`crate::curve::ed25519::sc_muladd`].
fn mul_mod(a: &[u32; 8], b: &[u32; 8], m: &[u32; 8]) -> [u32; 8] {
    let mut p = [0u32; 16];
    for i in 0..8 {
        let mut carry: u64 = 0;
        for j in 0..8 {
            let t = (a[i] as u64) * (b[j] as u64) + (p[i + j] as u64) + carry;
            p[i + j] = t as u32;
            carry = t >> 32;
        }
        p[i + 8] = p[i + 8].wrapping_add(carry as u32);
    }
    reduce_wide(&p, m)
}

/// `a^e mod m` — MSB-down square-and-multiply. `e` is a public
/// curve constant (`p - 2` for `Fp::inv`, `n - 2` for `Fn::inv`) so
/// there is no secret-dependent branch on bit values.
fn pow_mod(a: &[u32; 8], e: &[u32; 8], m: &[u32; 8]) -> [u32; 8] {
    let mut result = ONE_LIMBS;
    // MSB-down over the 256 bits of the exponent.
    for i in (0..256).rev() {
        result = mul_mod(&result, &result, m);
        let bit = (e[i / 32] >> (i % 32)) & 1;
        if bit == 1 {
            result = mul_mod(&result, a, m);
        }
    }
    result
}

/// `a^(-1) mod p` via Fermat: `a^(p-2) mod p`. Returns 0 for `a == 0`
/// (undefined but a safe sentinel).
fn inv_p(a: &[u32; 8]) -> [u32; 8] {
    pow_mod(a, &P_MINUS_2, &P)
}

/// `a^(-1) mod n` via Fermat: `a^(n-2) mod n`.
fn inv_n(a: &[u32; 8]) -> [u32; 8] {
    pow_mod(a, &N_MINUS_2, &N)
}

/// Bit-serial reduce a 512-bit unsigned integer (`wide`, 16 little-
/// endian u32 limbs) modulo `m`, returning the canonical `[0, m)`
/// 8-limb representative. Mirrors [`crate::curve::ed25519::sc_reduce_64`].
fn reduce_wide(wide: &[u32; 16], m: &[u32; 8]) -> [u32; 8] {
    // Accumulator: 9 u32 limbs (256 bits + 1 headroom limb for the
    // shift-in overflow before each conditional sub).
    let mut r = [0u32; 9];
    for i in (0..512).rev() {
        // r = (r << 1)
        let mut carry: u32 = 0;
        for limb in r.iter_mut() {
            let t = ((*limb as u64) << 1) | carry as u64;
            *limb = t as u32;
            carry = (t >> 32) as u32;
        }
        // OR in the next bit of `wide` (MSB-down).
        let byte_limb = i / 32;
        let bit_in_limb = i % 32;
        let bit = (wide[byte_limb] >> bit_in_limb) & 1;
        r[0] |= bit;
        // Conditional subtract m if r >= m (as 288-bit unsigned).
        conditional_sub_modulus(&mut r, m);
    }
    let mut out = [0u32; 8];
    out.copy_from_slice(&r[..8]);
    out
}

/// If the 9-limb value `r` is greater than or equal to `m` (treated
/// as 9 limbs with a zero top limb), subtract `m` from `r` in place.
/// Otherwise leave `r` unchanged. Used by [`add_mod`] and
/// [`reduce_wide`].
fn conditional_sub_modulus(r: &mut [u32; 9], m: &[u32; 8]) {
    let mut diff = [0u32; 9];
    let mut borrow: i64 = 0;
    for i in 0..8 {
        let t = (r[i] as i64) - (m[i] as i64) - borrow;
        if t < 0 {
            diff[i] = (t + (1i64 << 32)) as u32;
            borrow = 1;
        } else {
            diff[i] = t as u32;
            borrow = 0;
        }
    }
    let s8 = (r[8] as i64) - borrow;
    if s8 < 0 {
        // r < m — leave r alone.
        return;
    }
    diff[8] = s8 as u32;
    *r = diff;
}

// =====================================================================
// Point ops in Jacobian coordinates.
//
// Point representation: (X:Y:Z) with x = X/Z^2, y = Y/Z^3.
// Identity (point at infinity) is encoded as Z == 0.
// =====================================================================

/// A P-256 point in Jacobian projective coordinates.
///
/// The identity element (point at infinity) is encoded as `z == 0`.
#[derive(Clone, Copy)]
struct Point {
    x: [u32; 8],
    y: [u32; 8],
    z: [u32; 8],
}

impl Point {
    /// The identity (point at infinity).
    const fn identity() -> Self {
        Point { x: ONE_LIMBS, y: ONE_LIMBS, z: ZERO_LIMBS }
    }

    /// `true` iff this point encodes the identity.
    fn is_identity(&self) -> bool {
        is_zero_limbs(&self.z)
    }

    /// The base point G in Jacobian form (Z = 1).
    fn base() -> Self {
        Point { x: GX, y: GY, z: ONE_LIMBS }
    }

    /// Construct a Jacobian point from an affine `(x, y)`. Caller is
    /// responsible for having verified `(x, y)` lies on the curve.
    fn from_affine(x: &[u32; 8], y: &[u32; 8]) -> Self {
        Point { x: *x, y: *y, z: ONE_LIMBS }
    }

    /// Convert to affine `(x, y)`, or return `None` if this point is
    /// the identity (which has no affine representation).
    fn to_affine(&self) -> Option<([u32; 8], [u32; 8])> {
        if self.is_identity() {
            return None;
        }
        let z_inv = inv_p(&self.z);
        let z_inv2 = mul_mod(&z_inv, &z_inv, &P);
        let z_inv3 = mul_mod(&z_inv2, &z_inv, &P);
        Some((mul_mod(&self.x, &z_inv2, &P), mul_mod(&self.y, &z_inv3, &P)))
    }

    /// Constant-time conditional swap. Every field-limb is touched
    /// regardless of `swap` so the memory-access pattern does not
    /// depend on the mask.
    fn cswap(a: &mut Self, b: &mut Self, swap: u32) {
        // Materialise a 32-bit all-ones / all-zeros mask from the
        // low bit of `swap`.
        let mask = 0u32.wrapping_sub(swap & 1);
        for i in 0..8 {
            let dx = mask & (a.x[i] ^ b.x[i]);
            a.x[i] ^= dx;
            b.x[i] ^= dx;
            let dy = mask & (a.y[i] ^ b.y[i]);
            a.y[i] ^= dy;
            b.y[i] ^= dy;
            let dz = mask & (a.z[i] ^ b.z[i]);
            a.z[i] ^= dz;
            b.z[i] ^= dz;
        }
    }

    /// Point doubling in Jacobian coordinates with the `a = -3`
    /// optimisation.
    ///
    /// Formulas (Cohen §7.2 / EFD `dbl-2001-b`):
    /// ```text
    ///     delta = Z1^2
    ///     gamma = Y1^2
    ///     beta  = X1 * gamma
    ///     alpha = 3 * (X1 - delta) * (X1 + delta)
    ///     X3    = alpha^2 - 8 * beta
    ///     Z3    = (Y1 + Z1)^2 - gamma - delta
    ///     Y3    = alpha * (4*beta - X3) - 8 * gamma^2
    /// ```
    /// (`a = -3` folds into `alpha = 3*(X1-delta)*(X1+delta) =
    /// 3*X1^2 - 3*Z1^4 = -3*Z1^4 + 3*X1^2 = a*Z1^4 + 3*X1^2`.)
    fn double(&self) -> Self {
        if self.is_identity() {
            return Self::identity();
        }

        let delta = mul_mod(&self.z, &self.z, &P);
        let gamma = mul_mod(&self.y, &self.y, &P);
        let beta = mul_mod(&self.x, &gamma, &P);

        let x_minus_delta = sub_mod(&self.x, &delta, &P);
        let x_plus_delta = add_mod(&self.x, &delta, &P);
        let alpha_pre = mul_mod(&x_minus_delta, &x_plus_delta, &P);
        // alpha = 3 * alpha_pre
        let alpha = add_mod(
            &add_mod(&alpha_pre, &alpha_pre, &P),
            &alpha_pre,
            &P,
        );

        let alpha_sq = mul_mod(&alpha, &alpha, &P);
        // eight_beta = 8 * beta = beta << 3 (mod p)
        let two_beta = add_mod(&beta, &beta, &P);
        let four_beta = add_mod(&two_beta, &two_beta, &P);
        let eight_beta = add_mod(&four_beta, &four_beta, &P);
        let x3 = sub_mod(&alpha_sq, &eight_beta, &P);

        let y_plus_z = add_mod(&self.y, &self.z, &P);
        let y_plus_z_sq = mul_mod(&y_plus_z, &y_plus_z, &P);
        let z3 = sub_mod(&sub_mod(&y_plus_z_sq, &gamma, &P), &delta, &P);

        let four_beta_minus_x3 = sub_mod(&four_beta, &x3, &P);
        let alpha_term = mul_mod(&alpha, &four_beta_minus_x3, &P);
        let gamma_sq = mul_mod(&gamma, &gamma, &P);
        let two_gamma_sq = add_mod(&gamma_sq, &gamma_sq, &P);
        let four_gamma_sq = add_mod(&two_gamma_sq, &two_gamma_sq, &P);
        let eight_gamma_sq = add_mod(&four_gamma_sq, &four_gamma_sq, &P);
        let y3 = sub_mod(&alpha_term, &eight_gamma_sq, &P);

        Point { x: x3, y: y3, z: z3 }
    }

    /// Point addition in Jacobian coordinates (general case).
    ///
    /// Handles the identity, equal-input, and inverse-input special
    /// cases explicitly. Formulas from Hankerson / Menezes / Vanstone,
    /// "Guide to Elliptic Curve Cryptography", Algorithm 3.22
    /// (Cohen-style: `R = S2 - S1`, `Z3 = Z1*Z2*H`).
    ///
    /// ```text
    ///     Z1Z1 = Z1^2 ; Z2Z2 = Z2^2
    ///     U1 = X1 * Z2Z2 ; U2 = X2 * Z1Z1
    ///     S1 = Y1 * Z2 * Z2Z2 ; S2 = Y2 * Z1 * Z1Z1
    ///     H  = U2 - U1
    ///     R  = S2 - S1
    ///     H2 = H^2 ; H3 = H^3 ; U1H2 = U1 * H^2
    ///     X3 = R^2 - 2*U1*H^2 - H^3
    ///     Y3 = R*(U1*H^2 - X3) - S1*H^3
    ///     Z3 = Z1 * Z2 * H
    /// ```
    ///
    /// Mixing EFD `add-2007-bl`'s `I = (2H)^2 / J = H*I / V = U1*I`
    /// with the un-doubled `R = S2 - S1` gives a wrong projective
    /// point (the formula assumes `R = 2*(S2-S1)` to cancel the
    /// factor-of-4 that `I` introduces) — see the derivation in the
    /// PR message on paideia-as#1346. The Hankerson form here uses
    /// `H2` / `H3` directly and avoids that trap.
    fn add(&self, other: &Self) -> Self {
        if self.is_identity() {
            return *other;
        }
        if other.is_identity() {
            return *self;
        }

        // U1 = X1 * Z2^2 ; U2 = X2 * Z1^2
        let z1z1 = mul_mod(&self.z, &self.z, &P);
        let z2z2 = mul_mod(&other.z, &other.z, &P);
        let u1 = mul_mod(&self.x, &z2z2, &P);
        let u2 = mul_mod(&other.x, &z1z1, &P);

        // S1 = Y1 * Z2^3 ; S2 = Y2 * Z1^3
        let z1z1z1 = mul_mod(&z1z1, &self.z, &P);
        let z2z2z2 = mul_mod(&z2z2, &other.z, &P);
        let s1 = mul_mod(&self.y, &z2z2z2, &P);
        let s2 = mul_mod(&other.y, &z1z1z1, &P);

        if eq_limbs(&u1, &u2) {
            if !eq_limbs(&s1, &s2) {
                // P + (-P) = identity.
                return Self::identity();
            }
            // P == other — fall through to doubling.
            return self.double();
        }

        let h = sub_mod(&u2, &u1, &P);
        let r = sub_mod(&s2, &s1, &P);

        let h_sq = mul_mod(&h, &h, &P);          // H^2
        let h_cubed = mul_mod(&h_sq, &h, &P);    // H^3
        let u1_h_sq = mul_mod(&u1, &h_sq, &P);   // U1 * H^2

        // X3 = R^2 - 2*U1*H^2 - H^3
        let r_sq = mul_mod(&r, &r, &P);
        let two_u1_h_sq = add_mod(&u1_h_sq, &u1_h_sq, &P);
        let x3 = sub_mod(&sub_mod(&r_sq, &two_u1_h_sq, &P), &h_cubed, &P);

        // Y3 = R * (U1*H^2 - X3) - S1 * H^3
        let u1_h_sq_minus_x3 = sub_mod(&u1_h_sq, &x3, &P);
        let r_times = mul_mod(&r, &u1_h_sq_minus_x3, &P);
        let s1_h_cubed = mul_mod(&s1, &h_cubed, &P);
        let y3 = sub_mod(&r_times, &s1_h_cubed, &P);

        // Z3 = Z1 * Z2 * H
        let z1_z2 = mul_mod(&self.z, &other.z, &P);
        let z3 = mul_mod(&z1_z2, &h, &P);

        Point { x: x3, y: y3, z: z3 }
    }

    /// Constant-time scalar multiplication: `scalar * self`.
    ///
    /// Standard MSB-down double-and-add with [`Self::cswap`] on every
    /// step so the same field-op sequence runs regardless of bit
    /// values. Used by both the verify path (public data — public
    /// scalars `u1`, `u2`) and the sign path (secret scalars `k` and
    /// `d`). Verify's public-data status admits a variable-time
    /// double-scalar-multiplication follow-up (Shamir's trick /
    /// wNAF); the current shape trades that speed-up for a single
    /// audited primitive.
    fn scalar_mul(&self, scalar: &[u32; 8]) -> Self {
        let mut r0 = Self::identity();
        let mut r1 = *self;
        for i in (0..256).rev() {
            let bit = (scalar[i / 32] >> (i % 32)) & 1;
            Point::cswap(&mut r0, &mut r1, bit);
            let new_r1 = r0.add(&r1);
            let new_r0 = r0.double();
            r0 = new_r0;
            r1 = new_r1;
            Point::cswap(&mut r0, &mut r1, bit);
        }
        r0
    }
}

/// Verify that an affine `(x, y)` lies on the curve
/// `y^2 = x^3 - 3x + b (mod p)`.
///
/// Also rejects `x` or `y` outside the canonical `[0, p)` range —
/// pubkey material must not use over-length encodings.
fn point_on_curve(x: &[u32; 8], y: &[u32; 8]) -> bool {
    if cmp_limbs(x, &P) >= 0 || cmp_limbs(y, &P) >= 0 {
        return false;
    }
    // The point (0, 0) is not on the P-256 curve (b != 0) and would
    // otherwise pass the equation trivially in the affine form used
    // by the FFI; explicit reject.
    if is_zero_limbs(x) && is_zero_limbs(y) {
        return false;
    }
    let lhs = mul_mod(y, y, &P);
    let x2 = mul_mod(x, x, &P);
    let x3 = mul_mod(&x2, x, &P);
    // 3*x mod p
    let two_x = add_mod(x, x, &P);
    let three_x = add_mod(&two_x, x, &P);
    // rhs = x^3 - 3x + b  (mod p)
    let rhs = add_mod(&sub_mod(&x3, &three_x, &P), &B_COEFF, &P);
    eq_limbs(&lhs, &rhs)
}

// =====================================================================
// RFC 6979 deterministic nonce generation for P-256 + SHA-256.
// =====================================================================

/// Reduce `h1` (a 32-byte SHA-256 digest, interpreted big-endian as a
/// 256-bit integer) modulo `n`, returning the 32-byte big-endian
/// representative. This is the `bits2octets(h1, n)` primitive from
/// RFC 6979 §2.3.4 for the `qlen == blen == 256` case that P-256 with
/// SHA-256 lands in — no bit-shifting, only a possible single subtract
/// of `n`.
fn bits2octets_p256(h1: &[u8; 32]) -> [u8; 32] {
    let mut limbs = bytes_be_to_limbs(h1);
    // If h1 as integer >= n, subtract n once.
    if cmp_limbs(&limbs, &N) >= 0 {
        let mut diff = [0u32; 8];
        let mut borrow: i64 = 0;
        for i in 0..8 {
            let t = (limbs[i] as i64) - (N[i] as i64) - borrow;
            if t < 0 {
                diff[i] = (t + (1i64 << 32)) as u32;
                borrow = 1;
            } else {
                diff[i] = t as u32;
                borrow = 0;
            }
        }
        // borrow must be 0 here since limbs >= N.
        let _ = borrow;
        limbs = diff;
    }
    limbs_to_bytes_be(&limbs)
}

/// RFC 6979 §3.2 deterministic-nonce derivation, specialised to
/// P-256 + SHA-256 (both `qlen` and hash-output length are 256 bits).
///
/// Returns a nonce `k` with `1 <= k < n`. Never fails for well-formed
/// inputs — the retry loop terminates almost immediately (rejection
/// probability is ~`(2^256 - n) / 2^256 ≈ 2^-128`).
fn rfc6979_k(priv_key: &[u8; 32], msg_hash: &[u8; 32]) -> [u32; 8] {
    // Step (a)–(b): h1 = H(msg), already provided by caller.
    // Step (c): V = 0x01 * 32
    let mut v = [0x01u8; 32];
    // Step (d): K = 0x00 * 32
    let mut k = [0x00u8; 32];

    let x_octets = *priv_key; // int2octets(x, qlen) — priv_key is already 32 BE bytes.
    let bh1 = bits2octets_p256(msg_hash);

    // Step (e): K = HMAC(K, V || 0x00 || x_octets || bh1)
    {
        let mut msg = [0u8; 32 + 1 + 32 + 32];
        msg[..32].copy_from_slice(&v);
        msg[32] = 0x00;
        msg[33..65].copy_from_slice(&x_octets);
        msg[65..97].copy_from_slice(&bh1);
        k = hmac_sha256(&k, &msg);
    }
    // Step (f): V = HMAC(K, V)
    v = hmac_sha256(&k, &v);
    // Step (g): K = HMAC(K, V || 0x01 || x_octets || bh1)
    {
        let mut msg = [0u8; 32 + 1 + 32 + 32];
        msg[..32].copy_from_slice(&v);
        msg[32] = 0x01;
        msg[33..65].copy_from_slice(&x_octets);
        msg[65..97].copy_from_slice(&bh1);
        k = hmac_sha256(&k, &msg);
    }
    // Step (h): V = HMAC(K, V)
    v = hmac_sha256(&k, &v);

    // Step (h)(3): retry loop until a valid candidate lands in [1, n).
    // For P-256 with qlen == 256 the inner "concatenate until T is
    // qlen bits long" loop is one HMAC-SHA-256 output; no truncation
    // needed because SHA-256 already yields 256 bits.
    loop {
        v = hmac_sha256(&k, &v);
        // T = V (already 32 bytes = 256 bits = qlen)
        let candidate = bytes_be_to_limbs(&v);
        if !is_zero_limbs(&candidate) && cmp_limbs(&candidate, &N) < 0 {
            return candidate;
        }
        // Reject: K = HMAC(K, V || 0x00), V = HMAC(K, V), retry.
        let mut reject_msg = [0u8; 33];
        reject_msg[..32].copy_from_slice(&v);
        reject_msg[32] = 0x00;
        k = hmac_sha256(&k, &reject_msg);
        v = hmac_sha256(&k, &v);
    }
}

// =====================================================================
// Public Rust API
// =====================================================================

/// Verify an ECDSA-P256 signature `(r, s)` on `msg` under the public
/// key `(pub_x, pub_y)` per FIPS 186-4 §6.4.
///
/// All byte inputs are big-endian on the 32-byte wire encoding. The
/// return is a plain boolean; any failure (structural — malformed
/// scalar, off-curve pubkey — or algebraic — `R.x mod n != r`) reads
/// as `false`, never a panic.
///
/// # Constant-time
///
/// Verify operates on public data (public key `Q`, values `r`, `s`,
/// `e` are all public). The current implementation is nevertheless
/// constant-time (see [`Point::scalar_mul`]) because it re-uses the
/// single audited scalar-multiplication routine. A follow-up may add
/// a variable-time double-scalar-multiplication (Shamir's trick /
/// wNAF) purely as a performance win.
#[must_use]
pub fn ecdsa_p256_verify(
    pub_x: &[u8; 32],
    pub_y: &[u8; 32],
    msg: &[u8],
    sig_r: &[u8; 32],
    sig_s: &[u8; 32],
) -> bool {
    // Step 1: parse and range-check the signature.
    let r = bytes_be_to_limbs(sig_r);
    let s = bytes_be_to_limbs(sig_s);
    if is_zero_limbs(&r) || cmp_limbs(&r, &N) >= 0 {
        return false;
    }
    if is_zero_limbs(&s) || cmp_limbs(&s, &N) >= 0 {
        return false;
    }

    // Step 2: parse and validate the public key.
    let qx = bytes_be_to_limbs(pub_x);
    let qy = bytes_be_to_limbs(pub_y);
    if !point_on_curve(&qx, &qy) {
        return false;
    }
    let q = Point::from_affine(&qx, &qy);
    // Q must not be the identity — point_on_curve already rejects
    // (0, 0). An affine `(x, y)` with `x, y < p` never encodes the
    // Jacobian identity (Z != 0 by construction).

    // Step 3: e = SHA-256(msg) reduced-if-needed mod n.
    let e_bytes = sha256(msg);
    let mut e = bytes_be_to_limbs(&e_bytes);
    if cmp_limbs(&e, &N) >= 0 {
        // Single conditional subtract; the same shape as bits2octets.
        let mut diff = [0u32; 8];
        let mut borrow: i64 = 0;
        for i in 0..8 {
            let t = (e[i] as i64) - (N[i] as i64) - borrow;
            if t < 0 {
                diff[i] = (t + (1i64 << 32)) as u32;
                borrow = 1;
            } else {
                diff[i] = t as u32;
                borrow = 0;
            }
        }
        let _ = borrow;
        e = diff;
    }

    // Step 4: w = s^-1 mod n
    let w = inv_n(&s);
    // Step 5: u1 = e * w mod n
    let u1 = mul_mod(&e, &w, &N);
    // Step 6: u2 = r * w mod n
    let u2 = mul_mod(&r, &w, &N);

    // Step 7: R = u1 * G + u2 * Q
    let u1_g = Point::base().scalar_mul(&u1);
    let u2_q = q.scalar_mul(&u2);
    let big_r = u1_g.add(&u2_q);

    if big_r.is_identity() {
        return false;
    }

    let (rx, _) = match big_r.to_affine() {
        Some(a) => a,
        None => return false,
    };

    // Step 8: accept iff (rx mod n) == r.
    let mut rx_mod_n = rx;
    if cmp_limbs(&rx_mod_n, &N) >= 0 {
        let mut diff = [0u32; 8];
        let mut borrow: i64 = 0;
        for i in 0..8 {
            let t = (rx_mod_n[i] as i64) - (N[i] as i64) - borrow;
            if t < 0 {
                diff[i] = (t + (1i64 << 32)) as u32;
                borrow = 1;
            } else {
                diff[i] = t as u32;
                borrow = 0;
            }
        }
        let _ = borrow;
        rx_mod_n = diff;
    }
    eq_limbs(&rx_mod_n, &r)
}

/// Sign `msg` under `priv_key` per FIPS 186-4 §6.3 with an RFC 6979
/// §3.2 deterministic nonce.
///
/// The signature is returned as the pair `(r, s)` in big-endian
/// 32-byte encoding. Returns `None` if `priv_key` is not a valid
/// private key (must satisfy `1 <= d < n`).
///
/// # Constant-time
///
/// The scalar `d` and the deterministic nonce `k` are secret; both
/// flow through [`Point::scalar_mul`] and [`mul_mod`], which run in
/// data-independent time. HMAC-SHA-256 (used by RFC 6979) itself
/// runs constant-time on the derived key material.
#[must_use]
pub fn ecdsa_p256_sign(
    priv_key: &[u8; 32],
    msg: &[u8],
) -> Option<([u8; 32], [u8; 32])> {
    let d = bytes_be_to_limbs(priv_key);
    if is_zero_limbs(&d) || cmp_limbs(&d, &N) >= 0 {
        return None;
    }
    let e_bytes = sha256(msg);
    let mut e = bytes_be_to_limbs(&e_bytes);
    if cmp_limbs(&e, &N) >= 0 {
        // Single conditional subtract to bring e into [0, n).
        let mut diff = [0u32; 8];
        let mut borrow: i64 = 0;
        for i in 0..8 {
            let t = (e[i] as i64) - (N[i] as i64) - borrow;
            if t < 0 {
                diff[i] = (t + (1i64 << 32)) as u32;
                borrow = 1;
            } else {
                diff[i] = t as u32;
                borrow = 0;
            }
        }
        let _ = borrow;
        e = diff;
    }

    // Deterministic k via RFC 6979.
    let k = rfc6979_k(priv_key, &e_bytes);
    // The RFC 6979 driver guarantees 1 <= k < n, so no explicit
    // re-check needed. The FIPS-186-4 retry-on-zero-r / zero-s paths
    // below are guarded, though.

    // R = k*G
    let big_r = Point::base().scalar_mul(&k);
    let (rx, _) = big_r.to_affine()?;

    // r = R.x mod n
    let mut r = rx;
    if cmp_limbs(&r, &N) >= 0 {
        let mut diff = [0u32; 8];
        let mut borrow: i64 = 0;
        for i in 0..8 {
            let t = (r[i] as i64) - (N[i] as i64) - borrow;
            if t < 0 {
                diff[i] = (t + (1i64 << 32)) as u32;
                borrow = 1;
            } else {
                diff[i] = t as u32;
                borrow = 0;
            }
        }
        let _ = borrow;
        r = diff;
    }
    if is_zero_limbs(&r) {
        // Vanishingly rare; caller-visible via `None`.
        return None;
    }

    // s = k^-1 * (e + r*d) mod n
    let k_inv = inv_n(&k);
    let rd = mul_mod(&r, &d, &N);
    let e_plus_rd = add_mod(&e, &rd, &N);
    let s = mul_mod(&k_inv, &e_plus_rd, &N);
    if is_zero_limbs(&s) {
        return None;
    }

    Some((limbs_to_bytes_be(&r), limbs_to_bytes_be(&s)))
}

/// Derive the affine public key `(x, y)` from the 32-byte big-endian
/// private key.
///
/// Convenience for consumers that hold the private-key half only.
/// Returns `None` if `priv_key` is not a valid private key
/// (`1 <= d < n`).
#[must_use]
pub fn ecdsa_p256_public_from_secret(
    priv_key: &[u8; 32],
) -> Option<([u8; 32], [u8; 32])> {
    let d = bytes_be_to_limbs(priv_key);
    if is_zero_limbs(&d) || cmp_limbs(&d, &N) >= 0 {
        return None;
    }
    let q = Point::base().scalar_mul(&d);
    let (qx, qy) = q.to_affine()?;
    Some((limbs_to_bytes_be(&qx), limbs_to_bytes_be(&qy)))
}

// =====================================================================
// C-ABI thunks
//
// Mirror the shape established by `ffi::argon2id`, `ffi::ml_kem_768`,
// and `blake3::ffi`: fixed-length key / signature buffers cross the
// C-ABI seam as raw `*const u8` / `*mut u8` pointers; the variable-
// length message uses a `(*const u8, usize)` pair.
//
// The verify entry returns 1 on valid, 0 on any failure — the shape
// requested in paideia-as#1346 (compact yes/no result). The sign
// entry returns 1 on success, 0 on failure. This deliberately
// diverges from the other cryptoops thunks' PDX_CRYPTO_OK=0 /
// negative-error convention because the issue text asks for the
// compact form; downstream `.pdx` code just checks `rax == 1`.
// =====================================================================

// FFI thunks — extern-C entry points for `.pdx` callers.
//
// Kept in a nested module so the `#![allow(unsafe_code)]` lift is
// scoped to the C-ABI surface and does not spill into the trait /
// test code above. Symbols are `#[unsafe(no_mangle)]` and live in
// the crate's exported `nm` set at the same `paideia_crypto_*` path
// as the other cryptoops thunks — the module itself is private,
// mirroring `blake3::ffi`, and the entry points are re-exported at
// the `ecdsa_p256::` root below.
#[allow(unsafe_code)]
mod ffi {
    #![allow(unsafe_code)]

    use core::slice;

    use super::{ecdsa_p256_sign, ecdsa_p256_verify};

    /// FFI success-verify return value (`ecdsa_p256_verify`) — the
    /// signature authenticated correctly.
    pub const PDX_ECDSA_P256_VALID: i64 = 1;
    /// FFI failure-verify return value (`ecdsa_p256_verify`) — the
    /// signature did NOT authenticate, or an input pointer was
    /// invalid. Every non-success outcome collapses to this value
    /// per the issue's compact yes/no contract (paideia-as#1346).
    pub const PDX_ECDSA_P256_INVALID: i64 = 0;

    /// FFI success-sign return value (`ecdsa_p256_sign`) — a
    /// signature was produced and written to the output buffers.
    pub const PDX_ECDSA_P256_SIGN_OK: i64 = 1;
    /// FFI failure-sign return value (`ecdsa_p256_sign`) — the
    /// signer refused (invalid private-key input, or a null
    /// pointer). Output buffers are not written on failure.
    pub const PDX_ECDSA_P256_SIGN_FAIL: i64 = 0;

    /// ECDSA-P256 verify — FIPS 186-4 §6.4.
    ///
    /// SysV register mapping (as future `stdlib_lowering::cryptoops`
    /// recipes will emit; the elaborator hook is a Wave-1 follow-on
    /// in the same shape used by every other cryptoops thunk):
    ///
    /// | Register | Meaning                                            |
    /// |----------|----------------------------------------------------|
    /// | RDI      | `pubkey_x_ptr` — `*const [u8; 32]` (BE encoding)   |
    /// | RSI      | `pubkey_y_ptr` — `*const [u8; 32]` (BE encoding)   |
    /// | RDX      | `msg_ptr`      — `*const u8`                       |
    /// | RCX      | `msg_len`      — `usize`                           |
    /// | R8       | `sig_r_ptr`    — `*const [u8; 32]` (BE encoding)   |
    /// | R9       | `sig_s_ptr`    — `*const [u8; 32]` (BE encoding)   |
    /// | **RAX**  | 1 on valid, 0 on any failure                       |
    ///
    /// # Safety
    ///
    /// * `pubkey_x_ptr`, `pubkey_y_ptr`, `sig_r_ptr`, `sig_s_ptr`
    ///   must each be non-NULL and valid for reads of exactly 32
    ///   bytes.
    /// * If `msg_len > 0`, `msg_ptr` must be non-NULL and valid for
    ///   reads of `msg_len` bytes. `msg_ptr == NULL` with
    ///   `msg_len == 0` is accepted and verifies the signature on
    ///   the empty message.
    ///
    /// Every violation collapses to [`PDX_ECDSA_P256_INVALID`]
    /// (`0`) — the compact yes/no return per paideia-as#1346.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn paideia_crypto_ecdsa_p256_verify(
        pubkey_x_ptr: *const u8,
        pubkey_y_ptr: *const u8,
        msg_ptr: *const u8,
        msg_len: usize,
        sig_r_ptr: *const u8,
        sig_s_ptr: *const u8,
    ) -> i64 {
        if pubkey_x_ptr.is_null()
            || pubkey_y_ptr.is_null()
            || sig_r_ptr.is_null()
            || sig_s_ptr.is_null()
        {
            return PDX_ECDSA_P256_INVALID;
        }
        if msg_ptr.is_null() && msg_len > 0 {
            return PDX_ECDSA_P256_INVALID;
        }

        // SAFETY: caller-asserted — each pointer references 32 bytes.
        let pub_x: &[u8; 32] = unsafe { &*(pubkey_x_ptr as *const [u8; 32]) };
        let pub_y: &[u8; 32] = unsafe { &*(pubkey_y_ptr as *const [u8; 32]) };
        let sig_r: &[u8; 32] = unsafe { &*(sig_r_ptr as *const [u8; 32]) };
        let sig_s: &[u8; 32] = unsafe { &*(sig_s_ptr as *const [u8; 32]) };

        let msg: &[u8] = if msg_len == 0 {
            &[]
        } else {
            // SAFETY: caller-asserted `msg_ptr` valid for `msg_len` bytes.
            unsafe { slice::from_raw_parts(msg_ptr, msg_len) }
        };

        if ecdsa_p256_verify(pub_x, pub_y, msg, sig_r, sig_s) {
            PDX_ECDSA_P256_VALID
        } else {
            PDX_ECDSA_P256_INVALID
        }
    }

    /// ECDSA-P256 sign — FIPS 186-4 §6.3 with an RFC 6979 §3.2
    /// deterministic nonce.
    ///
    /// SysV register mapping:
    ///
    /// | Register | Meaning                                                |
    /// |----------|--------------------------------------------------------|
    /// | RDI      | `privkey_ptr`     — `*const [u8; 32]` (BE encoding)    |
    /// | RSI      | `msg_ptr`         — `*const u8`                        |
    /// | RDX      | `msg_len`         — `usize`                            |
    /// | RCX      | `out_sig_r_ptr`   — `*mut [u8; 32]` (writable, BE)     |
    /// | R8       | `out_sig_s_ptr`   — `*mut [u8; 32]` (writable, BE)     |
    /// | **RAX**  | 1 on success, 0 on failure                             |
    ///
    /// # Safety
    ///
    /// * `privkey_ptr` must be non-NULL and valid for reads of
    ///   exactly 32 bytes.
    /// * `out_sig_r_ptr`, `out_sig_s_ptr` must each be non-NULL and
    ///   valid for writes of exactly 32 bytes.
    /// * If `msg_len > 0`, `msg_ptr` must be non-NULL and valid for
    ///   reads of `msg_len` bytes.
    ///
    /// Every violation collapses to [`PDX_ECDSA_P256_SIGN_FAIL`]
    /// (`0`). The output buffers are not written on failure.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn paideia_crypto_ecdsa_p256_sign(
        privkey_ptr: *const u8,
        msg_ptr: *const u8,
        msg_len: usize,
        out_sig_r_ptr: *mut u8,
        out_sig_s_ptr: *mut u8,
    ) -> i64 {
        if privkey_ptr.is_null() || out_sig_r_ptr.is_null() || out_sig_s_ptr.is_null() {
            return PDX_ECDSA_P256_SIGN_FAIL;
        }
        if msg_ptr.is_null() && msg_len > 0 {
            return PDX_ECDSA_P256_SIGN_FAIL;
        }

        // SAFETY: caller-asserted — privkey_ptr references 32 bytes.
        let priv_key: &[u8; 32] = unsafe { &*(privkey_ptr as *const [u8; 32]) };
        let msg: &[u8] = if msg_len == 0 {
            &[]
        } else {
            // SAFETY: caller-asserted `msg_ptr` valid for `msg_len` bytes.
            unsafe { slice::from_raw_parts(msg_ptr, msg_len) }
        };

        match ecdsa_p256_sign(priv_key, msg) {
            Some((r, s)) => {
                // SAFETY: caller-asserted — output buffers each hold
                // at least 32 bytes.
                unsafe {
                    core::ptr::copy_nonoverlapping(r.as_ptr(), out_sig_r_ptr, 32);
                    core::ptr::copy_nonoverlapping(s.as_ptr(), out_sig_s_ptr, 32);
                }
                PDX_ECDSA_P256_SIGN_OK
            }
            None => PDX_ECDSA_P256_SIGN_FAIL,
        }
    }
}

// Re-export the FFI symbols and constants at the module root so a
// downstream `use paideia_as_crypto::ecdsa_p256::paideia_crypto_ecdsa_p256_verify`
// path resolves.
pub use ffi::{
    PDX_ECDSA_P256_INVALID, PDX_ECDSA_P256_SIGN_FAIL, PDX_ECDSA_P256_SIGN_OK,
    PDX_ECDSA_P256_VALID, paideia_crypto_ecdsa_p256_sign, paideia_crypto_ecdsa_p256_verify,
};

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    //! Reference vectors from RFC 6979 §A.2.5 (deterministic P-256 +
    //! SHA-256 signatures), plus structural rejection tests for the
    //! FIPS 186-4 §6.4 step-2 signature range check and off-curve
    //! pubkey validation.
    //!
    //! The crate-root `#![deny(unsafe_code)]` lint blocks the
    //! `unsafe` blocks the FFI-parity tests below need to invoke the
    //! extern-C thunks. Lift it for the test module only.
    #![allow(unsafe_code)]

    use super::*;
    // Tests are a child of the ecdsa_p256 module, so `use super::*`
    // brings the private `ffi` sub-module into scope alongside every
    // other item; the FFI-parity tests below reference the thunks
    // through `ffi::paideia_crypto_ecdsa_p256_*`.

    fn decode_hex_32(s: &str) -> [u8; 32] {
        let s: alloc::string::String = s.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(s.len(), 64, "expected 32-byte hex, got {}", s.len());
        let mut out = [0u8; 32];
        for i in 0..32 {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex");
        }
        out
    }

    // ----- RFC 6979 §A.2.5 P-256 vectors --------------------------
    //
    // Private key `x` and public key `(Ux, Uy)`:
    //   x  = C9AFA9D845BA75166B5C215767B1D6934E50C3DB36E89B127B8A622B120F6721
    //   Ux = 60FED4BA255A9D31C961EB74C6356D68C049B8923B61FA6CE669622E60F29FB6
    //   Uy = 7903FE1008B8BC99A41AE9E95628BC64F2F1B20C2D7E9F5177A3C294D4462299

    const RFC6979_X: &str =
        "C9AFA9D845BA75166B5C215767B1D6934E50C3DB36E89B127B8A622B120F6721";
    const RFC6979_UX: &str =
        "60FED4BA255A9D31C961EB74C6356D68C049B8923B61FA6CE669622E60F29FB6";
    const RFC6979_UY: &str =
        "7903FE1008B8BC99A41AE9E95628BC64F2F1B20C2D7E9F5177A3C294D4462299";

    // Vector 1: msg = "sample"
    //   k = A6E3C57DD01ABE90086538398355DD4C3B17AA873382B0F24D6129493D8AAD60
    //   r = EFD48B2AACB6A8FD1140DD9CD45E81D69D2C877B56AAF991C34D0EA84EAF3716
    //   s = F7CB1C942D657C41D436C7A1B6E29F65F3E900DBB9AFF4064DC4AB2F843ACDA8
    const RFC6979_SAMPLE_MSG: &[u8] = b"sample";
    const RFC6979_SAMPLE_R: &str =
        "EFD48B2AACB6A8FD1140DD9CD45E81D69D2C877B56AAF991C34D0EA84EAF3716";
    const RFC6979_SAMPLE_S: &str =
        "F7CB1C942D657C41D436C7A1B6E29F65F3E900DBB9AFF4064DC4AB2F843ACDA8";

    // Vector 2: msg = "test"
    //   k = D16B6AE827F17175E040871A1C7EC3500192C4C92677336EC2537ACAEE0008E0
    //   r = F1ABB023518351CD71D881567B1EA663ED3EFCF6C5132B354F28D3B0B7D38367
    //   s = 019F4113742A2B14BD25926B49C649155F267E60D3814B4C0CC84250E46F0083
    const RFC6979_TEST_MSG: &[u8] = b"test";
    const RFC6979_TEST_R: &str =
        "F1ABB023518351CD71D881567B1EA663ED3EFCF6C5132B354F28D3B0B7D38367";
    const RFC6979_TEST_S: &str =
        "019F4113742A2B14BD25926B49C649155F267E60D3814B4C0CC84250E46F0083";

    // -------- Public-key derivation smoke ------------------------

    #[test]
    fn public_from_secret_matches_rfc_6979_vector() {
        let x = decode_hex_32(RFC6979_X);
        let (qx, qy) =
            ecdsa_p256_public_from_secret(&x).expect("derives");
        assert_eq!(qx, decode_hex_32(RFC6979_UX), "Ux mismatch");
        assert_eq!(qy, decode_hex_32(RFC6979_UY), "Uy mismatch");
    }

    // -------- Sign: RFC 6979 deterministic vectors ---------------

    #[test]
    fn sign_rfc_6979_sample_matches_vector() {
        let x = decode_hex_32(RFC6979_X);
        let (r, s) = ecdsa_p256_sign(&x, RFC6979_SAMPLE_MSG)
            .expect("sign should succeed");
        assert_eq!(r, decode_hex_32(RFC6979_SAMPLE_R), "sample r");
        assert_eq!(s, decode_hex_32(RFC6979_SAMPLE_S), "sample s");
    }

    #[test]
    fn sign_rfc_6979_test_matches_vector() {
        let x = decode_hex_32(RFC6979_X);
        let (r, s) = ecdsa_p256_sign(&x, RFC6979_TEST_MSG)
            .expect("sign should succeed");
        assert_eq!(r, decode_hex_32(RFC6979_TEST_R), "test r");
        assert_eq!(s, decode_hex_32(RFC6979_TEST_S), "test s");
    }

    // -------- Verify: RFC 6979 deterministic vectors --------------

    #[test]
    fn verify_rfc_6979_sample_accepts() {
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = decode_hex_32(RFC6979_SAMPLE_R);
        let s = decode_hex_32(RFC6979_SAMPLE_S);
        assert!(
            ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "sample vector must verify"
        );
    }

    #[test]
    fn verify_rfc_6979_test_accepts() {
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = decode_hex_32(RFC6979_TEST_R);
        let s = decode_hex_32(RFC6979_TEST_S);
        assert!(
            ecdsa_p256_verify(&ux, &uy, RFC6979_TEST_MSG, &r, &s),
            "test vector must verify"
        );
    }

    // -------- Verify: round-trip through sign path ----------------

    #[test]
    fn sign_then_verify_round_trip() {
        let x = decode_hex_32(RFC6979_X);
        let (qx, qy) = ecdsa_p256_public_from_secret(&x).expect("pk derive");
        let msg = b"the quick brown fox jumps over the lazy dog";
        let (r, s) = ecdsa_p256_sign(&x, msg).expect("sign");
        assert!(ecdsa_p256_verify(&qx, &qy, msg, &r, &s), "self-round-trip");
    }

    // -------- Verify: negative — mutation ------------------------

    #[test]
    fn verify_rejects_mutated_message() {
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = decode_hex_32(RFC6979_SAMPLE_R);
        let s = decode_hex_32(RFC6979_SAMPLE_S);
        assert!(
            !ecdsa_p256_verify(&ux, &uy, b"samplE", &r, &s),
            "single-byte edit must reject"
        );
    }

    #[test]
    fn verify_rejects_mutated_r() {
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let mut r = decode_hex_32(RFC6979_SAMPLE_R);
        let s = decode_hex_32(RFC6979_SAMPLE_S);
        r[0] ^= 0x01;
        assert!(
            !ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "mutated r must reject"
        );
    }

    #[test]
    fn verify_rejects_mutated_s() {
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = decode_hex_32(RFC6979_SAMPLE_R);
        let mut s = decode_hex_32(RFC6979_SAMPLE_S);
        s[31] ^= 0x01;
        assert!(
            !ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "mutated s must reject"
        );
    }

    // -------- Verify: negative — range checks (FIPS 186-4 §6.4) --

    #[test]
    fn verify_rejects_r_equals_zero() {
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = [0u8; 32];
        let s = decode_hex_32(RFC6979_SAMPLE_S);
        assert!(
            !ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "r == 0 must reject"
        );
    }

    #[test]
    fn verify_rejects_s_equals_zero() {
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = decode_hex_32(RFC6979_SAMPLE_R);
        let s = [0u8; 32];
        assert!(
            !ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "s == 0 must reject"
        );
    }

    #[test]
    fn verify_rejects_r_equals_n() {
        // r = n (big-endian bytes of the group order) — must reject.
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = decode_hex_32(
            "FFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551",
        );
        let s = decode_hex_32(RFC6979_SAMPLE_S);
        assert!(
            !ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "r == n must reject"
        );
    }

    #[test]
    fn verify_rejects_s_equals_n() {
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = decode_hex_32(RFC6979_SAMPLE_R);
        let s = decode_hex_32(
            "FFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551",
        );
        assert!(
            !ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "s == n must reject"
        );
    }

    #[test]
    fn verify_rejects_r_greater_than_n() {
        // r = 0xFFFF..FF (all-ones), well above n. Must reject.
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = [0xFFu8; 32];
        let s = decode_hex_32(RFC6979_SAMPLE_S);
        assert!(
            !ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "r > n must reject"
        );
    }

    // -------- Verify: negative — off-curve pubkey ---------------

    #[test]
    fn verify_rejects_off_curve_pubkey() {
        // (Ux, Uy + 1) is very likely off-curve — the tangent from
        // any on-curve point at that Ux to another y-value doesn't
        // land on P-256 for random offsets.
        let ux = decode_hex_32(RFC6979_UX);
        let mut uy = decode_hex_32(RFC6979_UY);
        uy[31] = uy[31].wrapping_add(1);
        // Sanity-check the mutation actually broke the on-curve check.
        let qx = bytes_be_to_limbs(&ux);
        let qy = bytes_be_to_limbs(&uy);
        assert!(!point_on_curve(&qx, &qy), "test setup: mutated pk should be off-curve");

        let r = decode_hex_32(RFC6979_SAMPLE_R);
        let s = decode_hex_32(RFC6979_SAMPLE_S);
        assert!(
            !ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "off-curve pubkey must reject"
        );
    }

    #[test]
    fn verify_rejects_pubkey_x_ge_p() {
        // Ux = p (all-ones in the top 32 bits, then zeros, etc.) is
        // not a canonical field element and MUST reject before any
        // curve arithmetic.
        let ux = decode_hex_32(
            "FFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF",
        );
        let uy = decode_hex_32(RFC6979_UY);
        let r = decode_hex_32(RFC6979_SAMPLE_R);
        let s = decode_hex_32(RFC6979_SAMPLE_S);
        assert!(
            !ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "x >= p must reject"
        );
    }

    #[test]
    fn verify_rejects_pubkey_all_zero() {
        // (0, 0) is trivially not on P-256 (b != 0) — the point-at-
        // infinity case that FIPS 186-4 §6.4 step 2(a) forbids.
        let ux = [0u8; 32];
        let uy = [0u8; 32];
        let r = decode_hex_32(RFC6979_SAMPLE_R);
        let s = decode_hex_32(RFC6979_SAMPLE_S);
        assert!(
            !ecdsa_p256_verify(&ux, &uy, RFC6979_SAMPLE_MSG, &r, &s),
            "(0, 0) pubkey must reject"
        );
    }

    // -------- Sign: negative — invalid private key ---------------

    #[test]
    fn sign_rejects_zero_private_key() {
        let x = [0u8; 32];
        assert!(
            ecdsa_p256_sign(&x, RFC6979_SAMPLE_MSG).is_none(),
            "d == 0 must reject"
        );
    }

    #[test]
    fn sign_rejects_private_key_equal_to_n() {
        // d = n — outside the valid range [1, n).
        let x = decode_hex_32(
            "FFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551",
        );
        assert!(
            ecdsa_p256_sign(&x, RFC6979_SAMPLE_MSG).is_none(),
            "d == n must reject"
        );
    }

    // -------- FFI thunks: parity + null-handling -----------------

    #[test]
    fn ffi_verify_matches_trait_for_rfc_6979_sample() {
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = decode_hex_32(RFC6979_SAMPLE_R);
        let s = decode_hex_32(RFC6979_SAMPLE_S);

        let rc = unsafe {
            ffi::paideia_crypto_ecdsa_p256_verify(
                ux.as_ptr(),
                uy.as_ptr(),
                RFC6979_SAMPLE_MSG.as_ptr(),
                RFC6979_SAMPLE_MSG.len(),
                r.as_ptr(),
                s.as_ptr(),
            )
        };
        assert_eq!(rc, ffi::PDX_ECDSA_P256_VALID);

        // Mutated message must return INVALID.
        let bad_msg = b"samplE";
        let rc = unsafe {
            ffi::paideia_crypto_ecdsa_p256_verify(
                ux.as_ptr(),
                uy.as_ptr(),
                bad_msg.as_ptr(),
                bad_msg.len(),
                r.as_ptr(),
                s.as_ptr(),
            )
        };
        assert_eq!(rc, ffi::PDX_ECDSA_P256_INVALID);
    }

    #[test]
    fn ffi_verify_null_ptrs_return_invalid() {
        let ux = decode_hex_32(RFC6979_UX);
        let uy = decode_hex_32(RFC6979_UY);
        let r = decode_hex_32(RFC6979_SAMPLE_R);
        let s = decode_hex_32(RFC6979_SAMPLE_S);

        // Null pubkey x
        let rc = unsafe {
            ffi::paideia_crypto_ecdsa_p256_verify(
                core::ptr::null(),
                uy.as_ptr(),
                RFC6979_SAMPLE_MSG.as_ptr(),
                RFC6979_SAMPLE_MSG.len(),
                r.as_ptr(),
                s.as_ptr(),
            )
        };
        assert_eq!(rc, ffi::PDX_ECDSA_P256_INVALID);

        // Null msg with non-zero length
        let rc = unsafe {
            ffi::paideia_crypto_ecdsa_p256_verify(
                ux.as_ptr(),
                uy.as_ptr(),
                core::ptr::null(),
                42,
                r.as_ptr(),
                s.as_ptr(),
            )
        };
        assert_eq!(rc, ffi::PDX_ECDSA_P256_INVALID);
    }

    #[test]
    fn ffi_sign_matches_trait_for_rfc_6979_sample() {
        let x = decode_hex_32(RFC6979_X);
        let mut out_r = [0u8; 32];
        let mut out_s = [0u8; 32];
        let rc = unsafe {
            ffi::paideia_crypto_ecdsa_p256_sign(
                x.as_ptr(),
                RFC6979_SAMPLE_MSG.as_ptr(),
                RFC6979_SAMPLE_MSG.len(),
                out_r.as_mut_ptr(),
                out_s.as_mut_ptr(),
            )
        };
        assert_eq!(rc, ffi::PDX_ECDSA_P256_SIGN_OK);
        assert_eq!(out_r, decode_hex_32(RFC6979_SAMPLE_R));
        assert_eq!(out_s, decode_hex_32(RFC6979_SAMPLE_S));
    }

    #[test]
    fn ffi_sign_null_privkey_returns_fail() {
        let mut out_r = [0u8; 32];
        let mut out_s = [0u8; 32];
        let rc = unsafe {
            ffi::paideia_crypto_ecdsa_p256_sign(
                core::ptr::null(),
                RFC6979_SAMPLE_MSG.as_ptr(),
                RFC6979_SAMPLE_MSG.len(),
                out_r.as_mut_ptr(),
                out_s.as_mut_ptr(),
            )
        };
        assert_eq!(rc, ffi::PDX_ECDSA_P256_SIGN_FAIL);
        // Failure path must not touch the output buffers.
        assert_eq!(out_r, [0u8; 32]);
        assert_eq!(out_s, [0u8; 32]);
    }

    // -------- Base-point sanity ----------------------------------

    #[test]
    fn base_point_is_on_curve() {
        assert!(point_on_curve(&GX, &GY), "G must lie on the curve");
    }

    #[test]
    fn base_point_order_smoke_1G_2G_distinct() {
        // Cheap smoke: 1*G != 2*G. A full order check n*G == identity
        // would run 256 scalar bits × 3 muls × 512 reduce iterations
        // per field mul, prohibitive per-test cost.
        let g = Point::base();
        let two_g = g.double();
        let (g_x, g_y) = g.to_affine().expect("G affine");
        let (two_x, two_y) = two_g.to_affine().expect("2G affine");
        assert!(g_x != two_x || g_y != two_y, "1G and 2G must differ");
    }
}
