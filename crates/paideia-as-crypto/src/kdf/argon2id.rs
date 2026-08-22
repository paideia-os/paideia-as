//! Argon2id KDF — RFC 9106.
//!
//! # Reference
//!
//! Biryukov, Dinu, Khovratovich, Josefsson, *"Argon2 Memory-Hard
//! Function for Password Hashing and Proof-of-Work Applications"*,
//! [RFC 9106](https://www.rfc-editor.org/rfc/rfc9106.html),
//! September 2021.
//!
//! # Recommended parameters (RFC 9106 §4)
//!
//! - FIRST RECOMMENDED (memory-constrained, e.g. login servers):
//!   `t = 3`, `m = 2^16 KiB (64 MiB)`, `p = 4`, tag ≥ 128 bits.
//! - SECOND RECOMMENDED (memory-abundant):
//!   `t = 1`, `m = 2^21 KiB (2 GiB)`, `p = 4`, tag ≥ 128 bits.
//!
//! PaideiaOS §2.1 (interactive login on user desktops) targets the
//! FIRST recommended profile; the sealed-`user_sk` migration path
//! (§9.2) selects the profile based on device class at enrollment.
//!
//! # Test vector (RFC 9106 §5.3)
//!
//! ```text
//! Version   0x13 (v1.3)
//! Algorithm Argon2id
//! t = 3, m = 32 (KiB), p = 4, tag length = 32 bytes
//! password        = 0x01 * 32
//! salt            = 0x02 * 16
//! secret / key    = 0x03 *  8
//! associated data = 0x04 * 12
//! Tag:
//!   0d 64 0d f5 8d 78 76 6c  08 c0 37 a3 4a 8b 53 c9
//!   d0 1e f0 45 2d 75 b6 5e  b5 25 20 e9 6b 01 e6 59
//! ```
//!
//! The tag is exposed as [`RFC_9106_ARGON2ID_TAG`] so the integration
//! suite (v0.33-005) can re-use the same constant.

use super::{Kdf, KdfError};

use argon2::{Algorithm, Argon2, AssociatedData, ParamsBuilder, Version};

/// Argon2id KDF marker type. All operations are provided by the trait
/// impl below; the struct exists so downstream code can name the
/// algorithm (`Argon2id::derive(...)`) without threading a generic
/// parameter through every call site.
#[derive(Debug, Clone, Copy)]
pub struct Argon2id;

/// Bundle of inputs to one Argon2id derivation.
///
/// Field semantics match RFC 9106 §3.1 exactly:
///
/// - `password` (`P`): shared secret, 0..2^32 − 1 bytes.
/// - `salt` (`S`): 8..2^32 − 1 bytes; SHOULD be at least 16 bytes for
///   password hashing.
/// - `secret` (`K`, "pepper"): 0..32 bytes; optional server-side key.
/// - `associated_data` (`X`): 0..32 bytes; bound into the derivation
///   without appearing in output.
/// - `m_cost_kib` (`m`): memory in KiB; MUST be ≥ 8 · `p_cost`.
/// - `t_cost` (`t`): iteration count; MUST be ≥ 1.
/// - `p_cost` (`p`): parallelism; MUST be in `1..=2^24 − 1`.
///
/// The lifetime `'a` ties the borrowed byte slices together so the
/// caller controls allocation. This mirrors `Kdf::Params<'a>` GAT
/// declaration in `super`.
#[derive(Debug, Clone, Copy)]
pub struct Argon2idParams<'a> {
    /// Password / passphrase (`P` in RFC 9106).
    pub password: &'a [u8],
    /// Salt (`S`). RFC 9106 §3.1: length in `8..2^32-1`.
    pub salt: &'a [u8],
    /// Optional secret / pepper (`K`). Length in `0..=32`.
    pub secret: Option<&'a [u8]>,
    /// Optional associated data (`X`). Length in `0..=32`.
    pub associated_data: Option<&'a [u8]>,
    /// Memory cost in KiB. MUST be ≥ 8 · `p_cost`.
    pub m_cost_kib: u32,
    /// Time cost / iteration count. MUST be ≥ 1.
    pub t_cost: u32,
    /// Parallelism (lanes). MUST be in `1..=2^24 - 1`.
    pub p_cost: u32,
}

impl<'a> Argon2idParams<'a> {
    /// FIRST RECOMMENDED profile from RFC 9106 §4:
    /// `t = 3`, `m = 65_536 KiB (64 MiB)`, `p = 4`.
    ///
    /// Suitable for interactive passphrase unlock on a login server or
    /// user desktop. Yields a ~500 ms derivation on 2023-class silicon
    /// with 4 lanes.
    pub const RFC_9106_FIRST_RECOMMENDED_M_KIB: u32 = 1 << 16;
    /// FIRST RECOMMENDED time cost.
    pub const RFC_9106_FIRST_RECOMMENDED_T: u32 = 3;
    /// FIRST RECOMMENDED parallelism.
    pub const RFC_9106_FIRST_RECOMMENDED_P: u32 = 4;

    /// SECOND RECOMMENDED profile from RFC 9106 §4:
    /// `t = 1`, `m = 2_097_152 KiB (2 GiB)`, `p = 4`.
    pub const RFC_9106_SECOND_RECOMMENDED_M_KIB: u32 = 1 << 21;
    /// SECOND RECOMMENDED time cost.
    pub const RFC_9106_SECOND_RECOMMENDED_T: u32 = 1;
    /// SECOND RECOMMENDED parallelism.
    pub const RFC_9106_SECOND_RECOMMENDED_P: u32 = 4;

    /// Convenience constructor for the FIRST RECOMMENDED profile.
    /// Callers still supply `password`, `salt`, and optional
    /// `secret` / `associated_data`.
    pub fn first_recommended(password: &'a [u8], salt: &'a [u8]) -> Self {
        Self {
            password,
            salt,
            secret: None,
            associated_data: None,
            m_cost_kib: Self::RFC_9106_FIRST_RECOMMENDED_M_KIB,
            t_cost: Self::RFC_9106_FIRST_RECOMMENDED_T,
            p_cost: Self::RFC_9106_FIRST_RECOMMENDED_P,
        }
    }
}

/// Expected 32-byte tag from RFC 9106 §5.3 (`v = 0x13`, Argon2id,
/// `t = 3`, `m = 32`, `p = 4`, password / salt / secret / AD as
/// specified in the RFC). Re-used by the integration suite in
/// v0.33-005.
pub const RFC_9106_ARGON2ID_TAG: [u8; 32] = [
    0x0d, 0x64, 0x0d, 0xf5, 0x8d, 0x78, 0x76, 0x6c, 0x08, 0xc0, 0x37, 0xa3, 0x4a, 0x8b, 0x53, 0xc9,
    0xd0, 0x1e, 0xf0, 0x45, 0x2d, 0x75, 0xb6, 0x5e, 0xb5, 0x25, 0x20, 0xe9, 0x6b, 0x01, 0xe6, 0x59,
];

impl Kdf for Argon2id {
    type Params<'a> = Argon2idParams<'a>;

    fn derive(params: &Self::Params<'_>, output: &mut [u8]) -> Result<(), KdfError> {
        // RFC 9106 §3.1: tag length T MUST satisfy 4 ≤ T ≤ 2^32 − 1.
        // The `argon2` crate additionally rejects T > u32::MAX via
        // ParamsBuilder::output_len; we short-circuit the low bound to
        // give a stable error string.
        if output.len() < 4 {
            return Err(KdfError::InvalidOutputLen(output.len()));
        }

        let mut builder = ParamsBuilder::new();
        builder
            .m_cost(params.m_cost_kib)
            .t_cost(params.t_cost)
            .p_cost(params.p_cost)
            .output_len(output.len());

        if let Some(ad) = params.associated_data {
            let ad = AssociatedData::new(ad)
                .map_err(|e| KdfError::Primitive(format!("associated_data: {e}")))?;
            builder.data(ad);
        }

        let algo_params = builder
            .build()
            .map_err(|e| KdfError::InvalidParams(rfc9106_reason(e)))?;

        let ctx = match params.secret {
            Some(k) => Argon2::new_with_secret(k, Algorithm::Argon2id, Version::V0x13, algo_params)
                .map_err(|e| KdfError::Primitive(format!("secret: {e}")))?,
            None => Argon2::new(Algorithm::Argon2id, Version::V0x13, algo_params),
        };

        ctx.hash_password_into(params.password, params.salt, output)
            .map_err(|e| KdfError::Primitive(format!("hash_password_into: {e}")))?;

        Ok(())
    }
}

/// Map the argon2 crate's `Error` into a stable, test-pinnable
/// reason string. We keep the surface small because the crate's
/// `Error` variants are not part of our public API.
fn rfc9106_reason(err: argon2::Error) -> &'static str {
    use argon2::Error::*;
    match err {
        MemoryTooLittle => "m_cost < 8 * p_cost",
        MemoryTooMuch => "m_cost > 2^32 - 1",
        TimeTooSmall => "t_cost < 1",
        ThreadsTooFew => "p_cost < 1",
        ThreadsTooMany => "p_cost > 2^24 - 1",
        OutputTooShort => "output_len < 4",
        OutputTooLong => "output_len > 2^32 - 1",
        SaltTooShort => "salt too short",
        SaltTooLong => "salt too long",
        PwdTooLong => "password too long",
        AdTooLong => "associated_data too long",
        SecretTooLong => "secret too long",
        _ => "argon2 rejected parameters",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 9106 §5.3 — canonical Argon2id v=0x13 test vector.
    ///
    /// This is the acceptance-critical vector: the byte-exact tag lives
    /// in `RFC_9106_ARGON2ID_TAG` and MUST match a fresh derivation
    /// with the RFC's specified inputs.
    #[test]
    fn rfc_9106_section_5_3_argon2id_v13() {
        let password = [0x01u8; 32];
        let salt = [0x02u8; 16];
        let secret = [0x03u8; 8];
        let ad = [0x04u8; 12];

        let params = Argon2idParams {
            password: &password,
            salt: &salt,
            secret: Some(&secret),
            associated_data: Some(&ad),
            m_cost_kib: 32,
            t_cost: 3,
            p_cost: 4,
        };

        let mut out = [0u8; 32];
        Argon2id::derive(&params, &mut out).expect("derive succeeds");

        assert_eq!(
            out, RFC_9106_ARGON2ID_TAG,
            "Argon2id RFC 9106 §5.3 tag mismatch"
        );
    }

    /// Same inputs, no secret / AD: sanity-check that the derivation
    /// is deterministic (idempotent under fixed inputs) and that
    /// disabling AD / secret does *not* accidentally collide with the
    /// RFC vector.
    #[test]
    fn deterministic_and_ad_secret_bind_into_tag() {
        let password = [0x01u8; 32];
        let salt = [0x02u8; 16];
        let plain = Argon2idParams {
            password: &password,
            salt: &salt,
            secret: None,
            associated_data: None,
            m_cost_kib: 32,
            t_cost: 3,
            p_cost: 4,
        };

        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        Argon2id::derive(&plain, &mut a).unwrap();
        Argon2id::derive(&plain, &mut b).unwrap();
        assert_eq!(a, b, "Argon2id must be deterministic");

        // Dropping the secret + AD must change the tag; otherwise the
        // impl is silently ignoring one of those inputs.
        assert_ne!(
            a, RFC_9106_ARGON2ID_TAG,
            "removing secret / AD must change the tag"
        );
    }

    /// Output shorter than 4 bytes MUST be rejected per RFC 9106 §3.1.
    #[test]
    fn rejects_tag_shorter_than_4_bytes() {
        let params = Argon2idParams::first_recommended(b"x", &[0u8; 16]);
        let mut out = [0u8; 3];
        let err = Argon2id::derive(&params, &mut out).expect_err("must reject T < 4");
        match err {
            KdfError::InvalidOutputLen(n) => assert_eq!(n, 3),
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    /// `m_cost < 8 * p_cost` MUST be rejected per RFC 9106 §3.1.
    /// This pins the parameter-validation path so a future refactor
    /// that swaps the underlying crate cannot regress it.
    #[test]
    fn rejects_memory_below_lower_bound() {
        // 8 * p_cost = 32; m = 16 is below the floor.
        let password = [0u8; 8];
        let salt = [0u8; 16];
        let params = Argon2idParams {
            password: &password,
            salt: &salt,
            secret: None,
            associated_data: None,
            m_cost_kib: 16,
            t_cost: 1,
            p_cost: 4,
        };
        let mut out = [0u8; 32];
        let err = Argon2id::derive(&params, &mut out).expect_err("must reject small m");
        match err {
            KdfError::InvalidParams(reason) => {
                assert_eq!(reason, "m_cost < 8 * p_cost");
            }
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    /// FIRST RECOMMENDED profile constants match RFC 9106 §4.
    #[test]
    fn first_recommended_profile_constants() {
        let p = Argon2idParams::first_recommended(b"pw", &[0u8; 16]);
        assert_eq!(p.m_cost_kib, 1 << 16, "m = 64 MiB");
        assert_eq!(p.t_cost, 3);
        assert_eq!(p.p_cost, 4);
        assert!(p.secret.is_none());
        assert!(p.associated_data.is_none());
    }
}
