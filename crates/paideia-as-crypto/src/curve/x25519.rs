//! X25519 key exchange per RFC 7748 §5.
//!
//! Uses the standard constant-time Montgomery ladder over the
//! X-coordinates of Curve25519 (`y^2 = x^3 + 486662 x^2 + x` over
//! `GF(2^255 - 19)`). Field arithmetic lives in
//! [`crate::curve::field25519`].
//!
//! # API
//!
//! - [`x25519_scalarmult`] — the RFC 7748 §5 primitive: given a 32-byte
//!   scalar and a 32-byte u-coordinate, return the u-coordinate of the
//!   scalar multiplied by the input point.
//! - [`x25519_public_from_secret`] — convenience wrapper that
//!   multiplies the base point (`u = 9`) by the input scalar to derive
//!   an X25519 public key from a secret.
//!
//! # Constant-time
//!
//! The ladder uses [`FieldElement::cswap`] for every conditional
//! branch on scalar bits, and every arithmetic operation touches every
//! limb. See the RFC 7748 §5 pseudocode — this is a faithful
//! transliteration.
//!
//! # Deferred
//!
//! - The 1-million-iteration self-test from RFC 7748 §5.2 is present
//!   but `#[ignore]`d (CI-time-prohibitive; run manually with
//!   `cargo test --release -- --ignored`).

use super::field25519::FieldElement;

/// Compute `scalar * point` as an X-only Montgomery scalar mult on
/// Curve25519, returning the resulting u-coordinate as 32 little-endian
/// bytes (RFC 7748 §5).
#[must_use]
pub fn x25519_scalarmult(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32] {
    // RFC 7748 §5 "clamping" — clear bits 0, 1, 2 of scalar[0] and
    // clear bit 7 / set bit 6 of scalar[31].
    let mut k = *scalar;
    k[0] &= 248;
    k[31] &= 127;
    k[31] |= 64;

    // RFC 7748 §5: decodeUCoordinate implicitly masks bit 255 (top bit
    // of byte 31). FieldElement::from_bytes handles that.
    let x1 = FieldElement::from_bytes(point);

    // Ladder state (RFC 7748 §5):
    //   x_2 = 1,  z_2 = 0
    //   x_3 = x1, z_3 = 1
    //   swap = 0
    let mut x2 = FieldElement::ONE;
    let mut z2 = FieldElement::ZERO;
    let mut x3 = x1;
    let mut z3 = FieldElement::ONE;
    let mut swap: u64 = 0;

    // For t = 254 down to 0 (bits of the clamped scalar, MSB-to-LSB).
    for t in (0..=254).rev() {
        let k_t = ((k[t / 8] >> (t & 7)) & 1) as u64;
        swap ^= k_t;
        FieldElement::cswap(&mut x2, &mut x3, swap);
        FieldElement::cswap(&mut z2, &mut z3, swap);
        swap = k_t;

        // Differential doubling + addition (RFC 7748 §5 pseudocode).
        let a = x2.add(&z2);
        let aa = a.square();
        let b = x2.sub(&z2);
        let bb = b.square();
        let e = aa.sub(&bb);
        let c = x3.add(&z3);
        let d = x3.sub(&z3);
        let da = d.mul(&a);
        let cb = c.mul(&b);

        let x3_new = da.add(&cb).square();
        let z3_new = x1.mul(&(da.sub(&cb).square()));
        let x2_new = aa.mul(&bb);
        let z2_new = e.mul(&aa.add(&e.mul_a24()));

        x2 = x2_new;
        z2 = z2_new;
        x3 = x3_new;
        z3 = z3_new;
    }

    // Undo the final swap.
    FieldElement::cswap(&mut x2, &mut x3, swap);
    FieldElement::cswap(&mut z2, &mut z3, swap);

    // Return u = x_2 * (z_2)^-1.
    let z_inv = z2.invert();
    x2.mul(&z_inv).to_bytes()
}

/// Derive an X25519 public key from a secret via multiplication by the
/// standard base point `u = 9` (RFC 7748 §5).
#[must_use]
pub fn x25519_public_from_secret(secret: &[u8; 32]) -> [u8; 32] {
    let mut base = [0u8; 32];
    base[0] = 9;
    x25519_scalarmult(secret, &base)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_hex(s: &str) -> [u8; 32] {
        let s = s.replace(' ', "");
        let mut out = [0u8; 32];
        for i in 0..32 {
            let hi = u8::from_str_radix(&s[i * 2..i * 2 + 1], 16).expect("hex");
            let lo = u8::from_str_radix(&s[i * 2 + 1..i * 2 + 2], 16).expect("hex");
            out[i] = (hi << 4) | lo;
        }
        out
    }

    // ============================================================
    // RFC 7748 §5.2 — X25519 test vectors (first two, individually)
    // ============================================================

    #[test]
    fn rfc_7748_5_2_first_vector() {
        // scalar
        let k = decode_hex(
            "a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4",
        );
        // input u
        let u = decode_hex(
            "e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c",
        );
        // expected output
        let expected = decode_hex(
            "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552",
        );

        let out = x25519_scalarmult(&k, &u);
        assert_eq!(out, expected);
    }

    #[test]
    fn rfc_7748_5_2_second_vector() {
        let k = decode_hex(
            "4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d",
        );
        let u = decode_hex(
            "e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493",
        );
        let expected = decode_hex(
            "95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957",
        );

        let out = x25519_scalarmult(&k, &u);
        assert_eq!(out, expected);
    }

    // ============================================================
    // RFC 7748 §5.2 — iteration test (K = U = 9 initially; K = out; U = old K)
    // ============================================================
    //
    // After 1 iteration:  K = 422c8e7a6227d7bca1350b3e2bb7279f7897b87b b6854b783c60e80311ae3079
    // After 1000 iters:   K = 684cf59ba83309552800ef566f2f4d3c1c3887c49360e3875f2eb94d99532c51
    // After 1_000_000:    K = 7c3911e0ab2586fd864497297e575e6f3bc601c0883c30df5f4dd2d24f665424
    //
    // The 1M-iteration line is `#[ignore]`d — it takes many minutes even
    // on a fast machine. Run manually with `--ignored`.

    #[test]
    fn rfc_7748_5_2_iteration_after_1_iter() {
        let mut k = [0u8; 32];
        k[0] = 9;
        let mut u = k;
        let out = x25519_scalarmult(&k, &u);
        u = k;
        k = out;
        assert_eq!(
            k,
            decode_hex(
                "422c8e7a6227d7bca1350b3e2bb7279f7897b87bb6854b783c60e80311ae3079"
            )
        );
        let _ = u; // silence unused
    }

    #[test]
    fn rfc_7748_5_2_iteration_after_1000_iters() {
        let mut k = [0u8; 32];
        k[0] = 9;
        let mut u = k;
        for _ in 0..1000 {
            let out = x25519_scalarmult(&k, &u);
            u = k;
            k = out;
        }
        assert_eq!(
            k,
            decode_hex(
                "684cf59ba83309552800ef566f2f4d3c1c3887c49360e3875f2eb94d99532c51"
            )
        );
    }

    #[test]
    #[ignore = "RFC 7748 §5.2 1M-iteration test — CI-time-prohibitive; run with --ignored"]
    fn rfc_7748_5_2_iteration_after_1_000_000_iters() {
        let mut k = [0u8; 32];
        k[0] = 9;
        let mut u = k;
        for _ in 0..1_000_000 {
            let out = x25519_scalarmult(&k, &u);
            u = k;
            k = out;
        }
        assert_eq!(
            k,
            decode_hex(
                "7c3911e0ab2586fd864497297e575e6f3bc601c0883c30df5f4dd2d24f665424"
            )
        );
    }

    // ============================================================
    // RFC 7748 §6.1 — Diffie-Hellman worked example
    // ============================================================

    #[test]
    fn rfc_7748_6_1_diffie_hellman() {
        // Alice's private / public
        let alice_sk = decode_hex(
            "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a",
        );
        let alice_pk_expected = decode_hex(
            "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a",
        );

        // Bob's private / public
        let bob_sk = decode_hex(
            "5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb",
        );
        let bob_pk_expected = decode_hex(
            "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f",
        );

        // Shared secret
        let shared_expected = decode_hex(
            "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742",
        );

        let alice_pk = x25519_public_from_secret(&alice_sk);
        assert_eq!(alice_pk, alice_pk_expected);

        let bob_pk = x25519_public_from_secret(&bob_sk);
        assert_eq!(bob_pk, bob_pk_expected);

        let alice_shared = x25519_scalarmult(&alice_sk, &bob_pk);
        let bob_shared = x25519_scalarmult(&bob_sk, &alice_pk);
        assert_eq!(alice_shared, shared_expected);
        assert_eq!(bob_shared, shared_expected);
    }
}
