//! HMAC-SHA256 (RFC 2104 / FIPS 198-1) and HKDF (RFC 5869) over SHA-256.
//!
//! # References
//!
//! - RFC 2104 — HMAC: Keyed-Hashing for Message Authentication.
//! - FIPS 198-1 — The Keyed-Hash Message Authentication Code (HMAC).
//! - RFC 5869 §2.2 — HKDF-Extract.
//! - RFC 5869 §2.3 — HKDF-Expand.
//! - RFC 4231 — Identifiers and Test Vectors for HMAC-SHA-256 (…) family.
//!
//! # Consumers
//!
//! Both primitives are prerequisites for TLS 1.3's key schedule (RFC 8446
//! §7.1) — `libpdx-net` M3 depends on `hkdf_extract` / `hkdf_expand`
//! for `HKDF-Expand-Label` and its derived-secret ladder.

use crate::hash::sha256::Sha256Ctx;

/// SHA-256 block size (bytes). HMAC-SHA256 pads or hashes the key down
/// to this length before the two-pass compression.
const SHA256_BLOCK_LEN: usize = 64;

/// SHA-256 output size (bytes). Used everywhere as the length of a PRK
/// (pseudo-random key) and the width of the HKDF-Expand feedback tag.
const SHA256_OUT_LEN: usize = 32;

/// HMAC-SHA256 (RFC 2104 / FIPS 198-1).
///
/// `HMAC(K, m) = H((K' ⊕ opad) ‖ H((K' ⊕ ipad) ‖ m))` where `K'` is
/// `K` right-padded with zeros to the SHA-256 block size (64 bytes) if
/// `len(K) ≤ 64`, otherwise `K' = H(K)` (right-padded to 64 bytes).
///
/// `ipad = 0x36…36` (64 bytes), `opad = 0x5c…5c` (64 bytes).
#[must_use]
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; SHA256_OUT_LEN] {
    // Derive K' per RFC 2104 §2 / FIPS 198-1 §5.
    let mut k_prime = [0u8; SHA256_BLOCK_LEN];
    if key.len() > SHA256_BLOCK_LEN {
        // Hash the over-long key down first.
        let hashed = crate::hash::sha256(key);
        k_prime[..SHA256_OUT_LEN].copy_from_slice(&hashed);
        // Remaining bytes stay zero — that's the right-pad.
    } else {
        k_prime[..key.len()].copy_from_slice(key);
        // Remaining bytes are already zero.
    }

    // Inner: H((K' ⊕ ipad) ‖ m)
    let mut ipad_key = [0u8; SHA256_BLOCK_LEN];
    for i in 0..SHA256_BLOCK_LEN {
        ipad_key[i] = k_prime[i] ^ 0x36;
    }
    let mut inner = Sha256Ctx::new();
    inner.update(&ipad_key);
    inner.update(msg);
    let inner_digest = inner.finalize();

    // Outer: H((K' ⊕ opad) ‖ inner_digest)
    let mut opad_key = [0u8; SHA256_BLOCK_LEN];
    for i in 0..SHA256_BLOCK_LEN {
        opad_key[i] = k_prime[i] ^ 0x5c;
    }
    let mut outer = Sha256Ctx::new();
    outer.update(&opad_key);
    outer.update(&inner_digest);
    outer.finalize()
}

/// HKDF-Extract (RFC 5869 §2.2) over SHA-256.
///
/// `PRK = HMAC-SHA256(salt, IKM)`. Per §2.2, an empty `salt` MUST be
/// treated as `HashLen` zero bytes — HMAC-SHA256's own key-handling
/// already produces that result (a 32-byte all-zero key is
/// within-block, so `K'` == the 64-byte all-zero pad), so passing `&[]`
/// here is safe and RFC-conformant with no special case.
#[must_use]
pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; SHA256_OUT_LEN] {
    // The RFC 5869 §2.2 "empty salt" case reduces to HMAC with a 32-byte
    // zero salt (HashLen zero bytes). Route both branches through the
    // same code path — HMAC-SHA256(0^32, ikm) and HMAC-SHA256([], ikm)
    // yield identical output since our HMAC pads / zero-fills K' to 64
    // bytes either way.
    if salt.is_empty() {
        let zero_salt = [0u8; SHA256_OUT_LEN];
        hmac_sha256(&zero_salt, ikm)
    } else {
        hmac_sha256(salt, ikm)
    }
}

/// Errors returned by [`hkdf_expand`].
#[derive(Debug, PartialEq, Eq)]
pub enum HkdfExpandError {
    /// Requested output length exceeds `255 * HashLen` per RFC 5869 §2.3.
    OutputTooLong,
}

/// HKDF-Expand (RFC 5869 §2.3) over SHA-256.
///
/// Writes `out.len()` derived bytes into `out`. `out.len()` must be
/// `≤ 255 * HashLen` (255 * 32 = 8160 bytes); larger requests are
/// refused with [`HkdfExpandError::OutputTooLong`], matching the RFC's
/// hard upper bound on `L`.
///
/// # Algorithm
///
/// `T(0) = empty`; `T(i) = HMAC(PRK, T(i-1) ‖ info ‖ [i])` for
/// `i = 1..=N` where `N = ceil(L / HashLen)`. Output is
/// `T(1) ‖ T(2) ‖ … ‖ T(N)`, truncated to `L` bytes.
pub fn hkdf_expand(
    prk: &[u8],
    info: &[u8],
    out: &mut [u8],
) -> Result<(), HkdfExpandError> {
    if out.len() > 255 * SHA256_OUT_LEN {
        return Err(HkdfExpandError::OutputTooLong);
    }

    let mut previous: [u8; SHA256_OUT_LEN] = [0u8; SHA256_OUT_LEN];
    let mut previous_len = 0usize; // 0 for T(0), HashLen thereafter.
    let mut written = 0usize;
    let mut counter: u8 = 1;

    while written < out.len() {
        // T(i) = HMAC(PRK, T(i-1) ‖ info ‖ [i])
        let mut ctx = Sha256Ctx::new();
        // Reuse hmac_sha256's two-pass structure inline so we don't have
        // to concatenate T(i-1) ‖ info ‖ [i] into a fresh buffer.
        let mut k_prime = [0u8; SHA256_BLOCK_LEN];
        if prk.len() > SHA256_BLOCK_LEN {
            let hashed = crate::hash::sha256(prk);
            k_prime[..SHA256_OUT_LEN].copy_from_slice(&hashed);
        } else {
            k_prime[..prk.len()].copy_from_slice(prk);
        }

        let mut ipad_key = [0u8; SHA256_BLOCK_LEN];
        for i in 0..SHA256_BLOCK_LEN {
            ipad_key[i] = k_prime[i] ^ 0x36;
        }
        ctx.update(&ipad_key);
        if previous_len > 0 {
            ctx.update(&previous[..previous_len]);
        }
        ctx.update(info);
        ctx.update(&[counter]);
        let inner = ctx.finalize();

        let mut opad_key = [0u8; SHA256_BLOCK_LEN];
        for i in 0..SHA256_BLOCK_LEN {
            opad_key[i] = k_prime[i] ^ 0x5c;
        }
        let mut outer = Sha256Ctx::new();
        outer.update(&opad_key);
        outer.update(&inner);
        let t_i = outer.finalize();

        // Copy the leading bytes of T(i) into out.
        let take = (out.len() - written).min(SHA256_OUT_LEN);
        out[written..written + take].copy_from_slice(&t_i[..take]);
        written += take;

        previous = t_i;
        previous_len = SHA256_OUT_LEN;
        counter = counter.wrapping_add(1);
    }

    Ok(())
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

    fn decode_hex(s: &str) -> Vec<u8> {
        let s = s.replace(' ', "");
        let mut out = Vec::with_capacity(s.len() / 2);
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let hi = char::from(bytes[i]).to_digit(16).expect("hex digit") as u8;
            let lo = char::from(bytes[i + 1]).to_digit(16).expect("hex digit") as u8;
            out.push((hi << 4) | lo);
            i += 2;
        }
        out
    }

    // ============================================================
    // RFC 4231 — HMAC-SHA-256 test vectors §4.2..§4.8
    // ============================================================
    //
    // Cases 6 and 7 explicitly exercise the long-key hashed-key path
    // (131-byte key > 64-byte block size) — non-negotiable coverage.

    #[test]
    fn rfc_4231_test_case_1() {
        // Key = 0x0b × 20; Data = "Hi There"
        let key = [0x0bu8; 20];
        let data = b"Hi There";
        let mac = hmac_sha256(&key, data);
        assert_eq!(
            hex(&mac),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn rfc_4231_test_case_2() {
        // Key = "Jefe"; Data = "what do ya want for nothing?"
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn rfc_4231_test_case_3() {
        // Key = 0xaa × 20; Data = 0xdd × 50
        let key = [0xaau8; 20];
        let data = [0xddu8; 50];
        let mac = hmac_sha256(&key, &data);
        assert_eq!(
            hex(&mac),
            "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe"
        );
    }

    #[test]
    fn rfc_4231_test_case_4() {
        // Key = 0x01..0x19 (25 bytes); Data = 0xcd × 50
        let key: Vec<u8> = (1u8..=25).collect();
        let data = [0xcdu8; 50];
        let mac = hmac_sha256(&key, &data);
        assert_eq!(
            hex(&mac),
            "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b"
        );
    }

    #[test]
    fn rfc_4231_test_case_6_long_key() {
        // Key = 0xaa × 131 (>64 bytes → forces the hashed-key path).
        // Data = "Test Using Larger Than Block-Size Key - Hash Key First"
        let key = [0xaau8; 131];
        let data = b"Test Using Larger Than Block-Size Key - Hash Key First";
        let mac = hmac_sha256(&key, data);
        assert_eq!(
            hex(&mac),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn rfc_4231_test_case_7_long_key_long_data() {
        // Key = 0xaa × 131; Data = 152-byte message.
        let key = [0xaau8; 131];
        let data = b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm.";
        let mac = hmac_sha256(&key, data);
        assert_eq!(
            hex(&mac),
            "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2"
        );
    }

    // ============================================================
    // RFC 5869 — HKDF-SHA-256 test vectors §A.1..§A.3
    // ============================================================

    #[test]
    fn rfc_5869_a1_sha256_basic() {
        // Test Case 1 — basic SHA-256 with all fields non-empty.
        let ikm = decode_hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt = decode_hex("000102030405060708090a0b0c");
        let info = decode_hex("f0f1f2f3f4f5f6f7f8f9");
        let l = 42;

        let prk = hkdf_extract(&salt, &ikm);
        assert_eq!(
            hex(&prk),
            "077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5"
        );

        let mut okm = vec![0u8; l];
        hkdf_expand(&prk, &info, &mut okm).expect("expand ok");
        assert_eq!(
            hex(&okm),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
        );
    }

    #[test]
    fn rfc_5869_a2_sha256_longer() {
        // Test Case 2 — longer inputs and output.
        let ikm = decode_hex(concat!(
            "000102030405060708090a0b0c0d0e0f",
            "101112131415161718191a1b1c1d1e1f",
            "202122232425262728292a2b2c2d2e2f",
            "303132333435363738393a3b3c3d3e3f",
            "404142434445464748494a4b4c4d4e4f"
        ));
        let salt = decode_hex(concat!(
            "606162636465666768696a6b6c6d6e6f",
            "707172737475767778797a7b7c7d7e7f",
            "808182838485868788898a8b8c8d8e8f",
            "909192939495969798999a9b9c9d9e9f",
            "a0a1a2a3a4a5a6a7a8a9aaabacadaeaf"
        ));
        let info = decode_hex(concat!(
            "b0b1b2b3b4b5b6b7b8b9babbbcbdbebf",
            "c0c1c2c3c4c5c6c7c8c9cacbcccdcecf",
            "d0d1d2d3d4d5d6d7d8d9dadbdcdddedf",
            "e0e1e2e3e4e5e6e7e8e9eaebecedeeef",
            "f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff"
        ));
        let l = 82;

        let prk = hkdf_extract(&salt, &ikm);
        assert_eq!(
            hex(&prk),
            "06a6b88c5853361a06104c9ceb35b45cef760014904671014a193f40c15fc244"
        );

        let mut okm = vec![0u8; l];
        hkdf_expand(&prk, &info, &mut okm).expect("expand ok");
        assert_eq!(
            hex(&okm),
            concat!(
                "b11e398dc80327a1c8e7f78c596a4934",
                "4f012eda2d4efad8a050cc4c19afa97c",
                "59045a99cac7827271cb41c65e590e09",
                "da3275600c2f09b8367793a9aca3db71",
                "cc30c58179ec3e87c14c01d5c1f3434f",
                "1d87"
            )
        );
    }

    #[test]
    fn rfc_5869_a3_sha256_empty_salt_and_info() {
        // Test Case 3 — SHA-256 with empty salt and info.
        let ikm = decode_hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
        let salt: [u8; 0] = [];
        let info: [u8; 0] = [];
        let l = 42;

        let prk = hkdf_extract(&salt, &ikm);
        assert_eq!(
            hex(&prk),
            "19ef24a32c717b167f33a91d6f648bdf96596776afdb6377ac434c1c293ccb04"
        );

        let mut okm = vec![0u8; l];
        hkdf_expand(&prk, &info, &mut okm).expect("expand ok");
        assert_eq!(
            hex(&okm),
            "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8"
        );
    }

    // ============================================================
    // HKDF boundary conditions
    // ============================================================

    #[test]
    fn hkdf_expand_max_length_ok() {
        // 255 * 32 = 8160 bytes is the RFC 5869 §2.3 upper bound; MUST succeed.
        let prk = [0x42u8; 32];
        let mut out = vec![0u8; 255 * 32];
        hkdf_expand(&prk, b"", &mut out).expect("max length must succeed");
    }

    #[test]
    fn hkdf_expand_over_length_rejected() {
        // 255 * 32 + 1 = 8161 bytes exceeds the RFC 5869 §2.3 upper bound.
        let prk = [0x42u8; 32];
        let mut out = vec![0u8; 255 * 32 + 1];
        let err = hkdf_expand(&prk, b"", &mut out).expect_err("must reject");
        assert_eq!(err, HkdfExpandError::OutputTooLong);
    }

    #[test]
    fn hkdf_extract_empty_salt_matches_zero_salt() {
        // RFC 5869 §2.2 equivalence: empty salt ≡ HashLen zero bytes.
        // Both routes must produce identical PRK.
        let ikm = [0x0bu8; 22];
        let via_empty = hkdf_extract(&[], &ikm);
        let via_zeros = hkdf_extract(&[0u8; 32], &ikm);
        assert_eq!(via_empty, via_zeros, "RFC 5869 §2.2 equivalence broken");
    }
}
