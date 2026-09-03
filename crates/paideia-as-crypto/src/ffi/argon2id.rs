//! C-ABI thunk over [`crate::kdf::Argon2id`] (RFC 9106).
//!
//! One thunk (`paideia_crypto_argon2id_derive`) + one `#[repr(C)]`
//! parameter bundle (`Argon2idParamsC`) + RFC-9106 §5.3 vector tests.
//!
//! Split out of `ffi/mod.rs` per paideia-as#1354 so parallel authoring
//! of the v0.25-v0.32 crypto waves never collides inside the shared
//! module. Behaviour is byte-identical to the pre-split thunk; only the
//! file location changed. Shared error codes (`PDX_CRYPTO_*`) live in
//! `super` and every re-export from `crate::ffi::` continues to name
//! `paideia_crypto_argon2id_derive` at the same path.

// The crate-root `#![deny(unsafe_code)]` lint is lifted here so this
// FFI shim can dereference caller-supplied raw pointers. The `#[unsafe(
// no_mangle)]` attribute on the thunk is a Rust 2024 requirement for
// exported symbols and does not itself unsafe-gate the body.
#![allow(unsafe_code)]

use core::slice;

use crate::kdf::{Argon2id, Argon2idParams, Kdf, KdfError};

use super::{
    PDX_CRYPTO_ERR_INVALID_LENGTH, PDX_CRYPTO_ERR_INVALID_PARAM, PDX_CRYPTO_ERR_PRIMITIVE,
    PDX_CRYPTO_OK,
};

/// C-ABI parameter bundle for [`paideia_crypto_argon2id_derive`].
///
/// A NULL `secret_ptr` (with any `secret_len`) or a NULL `ad_ptr`
/// (with any `ad_len`) is interpreted as "field absent" — this
/// mirrors the `Option<&[u8]>` shape on [`Argon2idParams`].
///
/// The struct is `#[repr(C)]` so its layout is stable across
/// compilation units: the `.pdx` side lays out a matching struct in
/// static data (or on the stack) and hands its address in RDI.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Argon2idParamsC {
    /// Pointer to `password` bytes.
    pub password_ptr: *const u8,
    /// Length of `password`, in bytes.
    pub password_len: usize,
    /// Pointer to `salt` bytes.
    pub salt_ptr: *const u8,
    /// Length of `salt`, in bytes.
    pub salt_len: usize,
    /// Pointer to `secret` bytes, or NULL for "no secret".
    pub secret_ptr: *const u8,
    /// Length of `secret`, in bytes. Ignored when `secret_ptr` is NULL.
    pub secret_len: usize,
    /// Pointer to associated-data bytes, or NULL for "no AD".
    pub ad_ptr: *const u8,
    /// Length of associated data, in bytes. Ignored when `ad_ptr` is NULL.
    pub ad_len: usize,
    /// Memory cost, KiB (RFC 9106 `m`).
    pub m_cost_kib: u32,
    /// Time cost / iterations (RFC 9106 `t`).
    pub t_cost: u32,
    /// Parallelism (RFC 9106 `p`).
    pub p_cost: u32,
    /// Reserved for future use; MUST be zero.
    pub _reserved: u32,
}

/// Derive Argon2id key material into `out_ptr[..out_len]`.
///
/// SysV register mapping (as emitted by `stdlib_lowering::cryptoops`):
///
/// | Register | Meaning                              |
/// |----------|--------------------------------------|
/// | RDI      | `params` — pointer to a valid `Argon2idParamsC` |
/// | RSI      | `out_ptr` — pointer to writable buffer of `out_len` bytes |
/// | RDX      | `out_len` — buffer length in bytes    |
/// | **RAX**  | return code (see `PDX_CRYPTO_*`)     |
///
/// # Safety
///
/// * `params` must point to a live, correctly-initialised
///   [`Argon2idParamsC`] whose pointer fields either satisfy the slice
///   invariants (aligned, valid for reads of the paired length) or are
///   NULL where the field is documented as optional.
/// * `out_ptr` must be non-NULL and valid for writes of `out_len`
///   bytes.
///
/// Returns `PDX_CRYPTO_OK` on success, one of the negative error
/// codes on failure. On failure the output buffer is not written.
///
/// # Panics
///
/// Never panics under valid inputs. UB (not a panic) on precondition
/// violation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_argon2id_derive(
    params: *const Argon2idParamsC,
    out_ptr: *mut u8,
    out_len: usize,
) -> i64 {
    if params.is_null() || out_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    // SAFETY: caller-asserted precondition.
    let p = unsafe { &*params };
    if p.password_ptr.is_null() || p.salt_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }

    // SAFETY: password/salt/secret/ad pointer + length pairs are the
    // caller's responsibility per the doc comment above.
    let password = unsafe { slice::from_raw_parts(p.password_ptr, p.password_len) };
    let salt = unsafe { slice::from_raw_parts(p.salt_ptr, p.salt_len) };
    let secret = if p.secret_ptr.is_null() {
        None
    } else {
        Some(unsafe { slice::from_raw_parts(p.secret_ptr, p.secret_len) })
    };
    let ad = if p.ad_ptr.is_null() {
        None
    } else {
        Some(unsafe { slice::from_raw_parts(p.ad_ptr, p.ad_len) })
    };
    let out = unsafe { slice::from_raw_parts_mut(out_ptr, out_len) };

    let params_typed = Argon2idParams {
        password,
        salt,
        secret,
        associated_data: ad,
        m_cost_kib: p.m_cost_kib,
        t_cost: p.t_cost,
        p_cost: p.p_cost,
    };

    match Argon2id::derive(&params_typed, out) {
        Ok(()) => PDX_CRYPTO_OK,
        Err(KdfError::InvalidParams(_)) => PDX_CRYPTO_ERR_INVALID_PARAM,
        Err(KdfError::InvalidOutputLen(_)) => PDX_CRYPTO_ERR_INVALID_LENGTH,
        Err(KdfError::Primitive(_)) => PDX_CRYPTO_ERR_PRIMITIVE,
    }
}

#[cfg(test)]
mod tests {
    //! FFI-level Argon2id tests. Re-derive the canonical RFC 9106 §5.3
    //! vector through the extern-C thunk — verifying that the C shim
    //! reproduces the same bytes as the trait-level test in
    //! `kdf::argon2id`, and therefore that the extern-C surface is a
    //! faithful projection of the trait API.

    use super::*;
    use crate::kdf::RFC_9106_ARGON2ID_TAG;

    #[test]
    fn ffi_argon2id_derive_reproduces_rfc_9106_section_5_3() {
        let password = [0x01u8; 32];
        let salt = [0x02u8; 16];
        let secret = [0x03u8; 8];
        let ad = [0x04u8; 12];

        let p = Argon2idParamsC {
            password_ptr: password.as_ptr(),
            password_len: password.len(),
            salt_ptr: salt.as_ptr(),
            salt_len: salt.len(),
            secret_ptr: secret.as_ptr(),
            secret_len: secret.len(),
            ad_ptr: ad.as_ptr(),
            ad_len: ad.len(),
            m_cost_kib: 32,
            t_cost: 3,
            p_cost: 4,
            _reserved: 0,
        };

        let mut out = [0u8; 32];
        let rc = unsafe {
            paideia_crypto_argon2id_derive(&p as *const _, out.as_mut_ptr(), out.len())
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(out, RFC_9106_ARGON2ID_TAG);
    }

    #[test]
    fn ffi_argon2id_null_params_returns_invalid_param() {
        let mut out = [0u8; 32];
        let rc = unsafe {
            paideia_crypto_argon2id_derive(core::ptr::null(), out.as_mut_ptr(), out.len())
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_PARAM);
    }

    #[test]
    fn ffi_argon2id_short_output_returns_invalid_length() {
        // T < 4 rejected by the trait; the FFI must forward the code.
        let password = [0u8; 8];
        let salt = [0u8; 16];
        let p = Argon2idParamsC {
            password_ptr: password.as_ptr(),
            password_len: password.len(),
            salt_ptr: salt.as_ptr(),
            salt_len: salt.len(),
            secret_ptr: core::ptr::null(),
            secret_len: 0,
            ad_ptr: core::ptr::null(),
            ad_len: 0,
            m_cost_kib: 1 << 16,
            t_cost: 3,
            p_cost: 4,
            _reserved: 0,
        };
        let mut out = [0u8; 3];
        let rc = unsafe {
            paideia_crypto_argon2id_derive(&p as *const _, out.as_mut_ptr(), out.len())
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_LENGTH);
    }
}
