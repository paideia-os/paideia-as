//! C-ABI thunks over [`crate::aead::ChaCha20Poly1305`] (RFC 8439).
//!
//! Two thunks (`paideia_crypto_chacha20_poly1305_seal` and `_open`) +
//! one `#[repr(C)]` parameter bundle (`AeadParamsC`) + RFC-8439 §2.8.2
//! vector tests.
//!
//! Split out of `ffi/mod.rs` per paideia-as#1354 so parallel authoring
//! of the v0.25-v0.32 crypto waves never collides inside the shared
//! module. Behaviour is byte-identical to the pre-split thunks; only
//! the file location changed. Shared helpers (`aead_err_code`,
//! `params_from_c`) and error codes live in `super`.

// The crate-root `#![deny(unsafe_code)]` lint is lifted here so this
// FFI shim can dereference caller-supplied raw pointers.
#![allow(unsafe_code)]

use core::slice;

use crate::aead::{Aead, ChaCha20Poly1305, TAG_LEN};

use super::{
    PDX_CRYPTO_ERR_BUFFER_TOO_SMALL, PDX_CRYPTO_ERR_INVALID_LENGTH,
    PDX_CRYPTO_ERR_INVALID_PARAM, PDX_CRYPTO_OK, aead_err_code, params_from_c,
};

/// C-ABI parameter bundle for [`paideia_crypto_chacha20_poly1305_seal`]
/// and [`paideia_crypto_chacha20_poly1305_open`].
///
/// The struct is `#[repr(C)]` for stable layout. The `key` and `nonce`
/// pointers reference fixed-size byte arrays (32 and 12 bytes
/// respectively); passing shorter or misaligned buffers is undefined
/// behaviour.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AeadParamsC {
    /// Pointer to a `[u8; 32]` key.
    pub key_ptr: *const u8,
    /// Pointer to a `[u8; 12]` nonce.
    pub nonce_ptr: *const u8,
    /// Pointer to associated-data bytes. May be NULL when `aad_len == 0`.
    pub aad_ptr: *const u8,
    /// Length of associated data, in bytes. Zero is legal.
    pub aad_len: usize,
}

/// Seal `plaintext[..plaintext_len]` under `params`, writing
/// `ciphertext || tag` into `out_ptr[..out_cap]` and the actual
/// number of bytes written into `*written`.
///
/// SysV register mapping (6 args, all in registers):
///
/// | Register | Meaning                                     |
/// |----------|---------------------------------------------|
/// | RDI      | `params` — pointer to [`AeadParamsC`]       |
/// | RSI      | `plaintext_ptr`                             |
/// | RDX      | `plaintext_len`                             |
/// | RCX      | `out_ptr` — writable buffer                 |
/// | R8       | `out_cap`                                   |
/// | R9       | `written` — pointer to `usize` (out param)  |
/// | **RAX**  | return code (see `PDX_CRYPTO_*`)            |
///
/// Sealed output length is `plaintext_len + TAG_LEN`. Callers MUST
/// supply `out_cap >= plaintext_len + TAG_LEN`; otherwise the call
/// returns `PDX_CRYPTO_ERR_BUFFER_TOO_SMALL` and no bytes are
/// written.
///
/// # Safety
///
/// * `params` must point to a live [`AeadParamsC`] whose `key_ptr` and
///   `nonce_ptr` reference 32-byte and 12-byte arrays respectively.
/// * `plaintext_ptr` must be non-NULL when `plaintext_len > 0` and
///   valid for reads of `plaintext_len` bytes.
/// * `out_ptr` must be non-NULL and valid for writes of `out_cap`
///   bytes.
/// * `written` must be non-NULL and valid for a `usize` write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_chacha20_poly1305_seal(
    params: *const AeadParamsC,
    plaintext_ptr: *const u8,
    plaintext_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    written: *mut usize,
) -> i64 {
    if params.is_null() || out_ptr.is_null() || written.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    let needed = plaintext_len.saturating_add(TAG_LEN);
    if out_cap < needed {
        return PDX_CRYPTO_ERR_BUFFER_TOO_SMALL;
    }
    let typed_params = match unsafe { params_from_c(params) } {
        Ok(p) => p,
        Err(code) => return code,
    };
    let plaintext = if plaintext_len == 0 {
        &[][..]
    } else if plaintext_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    } else {
        // SAFETY: caller-asserted precondition.
        unsafe { slice::from_raw_parts(plaintext_ptr, plaintext_len) }
    };

    match ChaCha20Poly1305::seal(&typed_params, plaintext) {
        Ok(sealed) => {
            // Length is always `plaintext_len + TAG_LEN` for CC20-P1305; assert.
            debug_assert_eq!(sealed.len(), needed);
            // SAFETY: `out_cap >= needed` (checked above); `sealed.len() == needed`.
            unsafe {
                core::ptr::copy_nonoverlapping(sealed.as_ptr(), out_ptr, sealed.len());
                *written = sealed.len();
            }
            PDX_CRYPTO_OK
        }
        Err(e) => aead_err_code(&e),
    }
}

/// Open (decrypt + authenticate) `sealed[..sealed_len]` under `params`
/// and write the recovered plaintext into `out_ptr[..out_cap]` with
/// the byte count landed in `*written`.
///
/// Register mapping mirrors [`paideia_crypto_chacha20_poly1305_seal`].
///
/// The recovered-plaintext length is `sealed_len - TAG_LEN` on
/// success. Callers MUST supply `out_cap >= sealed_len - TAG_LEN`
/// (and `sealed_len >= TAG_LEN`); undersized inputs return
/// `PDX_CRYPTO_ERR_INVALID_LENGTH`, undersized outputs return
/// `PDX_CRYPTO_ERR_BUFFER_TOO_SMALL`.
///
/// On tag mismatch the call returns
/// `PDX_CRYPTO_ERR_AUTHENTICATION` and does not write to `out_ptr`.
///
/// # Safety
///
/// See [`paideia_crypto_chacha20_poly1305_seal`]. Additionally,
/// `sealed_ptr` must be non-NULL and valid for reads of `sealed_len`
/// bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn paideia_crypto_chacha20_poly1305_open(
    params: *const AeadParamsC,
    sealed_ptr: *const u8,
    sealed_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    written: *mut usize,
) -> i64 {
    if params.is_null() || out_ptr.is_null() || written.is_null() || sealed_ptr.is_null() {
        return PDX_CRYPTO_ERR_INVALID_PARAM;
    }
    if sealed_len < TAG_LEN {
        return PDX_CRYPTO_ERR_INVALID_LENGTH;
    }
    let plaintext_needed = sealed_len - TAG_LEN;
    if out_cap < plaintext_needed {
        return PDX_CRYPTO_ERR_BUFFER_TOO_SMALL;
    }
    let typed_params = match unsafe { params_from_c(params) } {
        Ok(p) => p,
        Err(code) => return code,
    };
    // SAFETY: caller-asserted precondition; sealed_len is > 0 here.
    let sealed = unsafe { slice::from_raw_parts(sealed_ptr, sealed_len) };

    match ChaCha20Poly1305::open(&typed_params, sealed) {
        Ok(plaintext) => {
            debug_assert_eq!(plaintext.len(), plaintext_needed);
            // SAFETY: `out_cap >= plaintext.len()` (checked above).
            unsafe {
                core::ptr::copy_nonoverlapping(plaintext.as_ptr(), out_ptr, plaintext.len());
                *written = plaintext.len();
            }
            PDX_CRYPTO_OK
        }
        Err(e) => aead_err_code(&e),
    }
}

#[cfg(test)]
mod tests {
    //! FFI-level ChaCha20-Poly1305 tests. Re-seal / re-open the canonical
    //! RFC 8439 §2.8.2 vector through the extern-C thunks — verifying that
    //! the C shim reproduces the same bytes as the trait-level tests in
    //! `aead::chacha20_poly1305`.

    use super::*;
    use crate::aead::{
        RFC_8439_SEC_2_8_2_AAD, RFC_8439_SEC_2_8_2_CIPHERTEXT, RFC_8439_SEC_2_8_2_KEY,
        RFC_8439_SEC_2_8_2_NONCE, RFC_8439_SEC_2_8_2_PLAINTEXT, RFC_8439_SEC_2_8_2_TAG,
    };
    use crate::ffi::PDX_CRYPTO_ERR_AUTHENTICATION;

    #[test]
    fn ffi_chacha20_poly1305_seal_reproduces_rfc_8439_section_2_8_2() {
        let key = RFC_8439_SEC_2_8_2_KEY;
        let nonce = RFC_8439_SEC_2_8_2_NONCE;
        let aad = RFC_8439_SEC_2_8_2_AAD;

        let params = AeadParamsC {
            key_ptr: key.as_ptr(),
            nonce_ptr: nonce.as_ptr(),
            aad_ptr: aad.as_ptr(),
            aad_len: aad.len(),
        };

        let plaintext = RFC_8439_SEC_2_8_2_PLAINTEXT;
        let mut out = vec![0u8; plaintext.len() + TAG_LEN];
        let mut written: usize = 0;
        let rc = unsafe {
            paideia_crypto_chacha20_poly1305_seal(
                &params as *const _,
                plaintext.as_ptr(),
                plaintext.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written as *mut _,
            )
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(written, plaintext.len() + TAG_LEN);

        // Byte-exact match against the RFC ciphertext || tag.
        let (ct, tag) = out[..written].split_at(plaintext.len());
        assert_eq!(ct, &RFC_8439_SEC_2_8_2_CIPHERTEXT[..]);
        assert_eq!(tag, &RFC_8439_SEC_2_8_2_TAG[..]);
    }

    #[test]
    fn ffi_chacha20_poly1305_open_round_trips_rfc_8439_section_2_8_2() {
        let key = RFC_8439_SEC_2_8_2_KEY;
        let nonce = RFC_8439_SEC_2_8_2_NONCE;
        let aad = RFC_8439_SEC_2_8_2_AAD;

        // Reconstruct sealed = ciphertext || tag.
        let mut sealed = Vec::with_capacity(RFC_8439_SEC_2_8_2_CIPHERTEXT.len() + TAG_LEN);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_CIPHERTEXT);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_TAG);

        let params = AeadParamsC {
            key_ptr: key.as_ptr(),
            nonce_ptr: nonce.as_ptr(),
            aad_ptr: aad.as_ptr(),
            aad_len: aad.len(),
        };

        let mut out = vec![0u8; RFC_8439_SEC_2_8_2_PLAINTEXT.len()];
        let mut written: usize = 0;
        let rc = unsafe {
            paideia_crypto_chacha20_poly1305_open(
                &params as *const _,
                sealed.as_ptr(),
                sealed.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written as *mut _,
            )
        };
        assert_eq!(rc, PDX_CRYPTO_OK);
        assert_eq!(written, RFC_8439_SEC_2_8_2_PLAINTEXT.len());
        assert_eq!(&out[..written], RFC_8439_SEC_2_8_2_PLAINTEXT);
    }

    #[test]
    fn ffi_chacha20_poly1305_open_tag_mismatch_returns_authentication_error() {
        let key = RFC_8439_SEC_2_8_2_KEY;
        let nonce = RFC_8439_SEC_2_8_2_NONCE;
        let aad = RFC_8439_SEC_2_8_2_AAD;

        let mut sealed = Vec::with_capacity(RFC_8439_SEC_2_8_2_CIPHERTEXT.len() + TAG_LEN);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_CIPHERTEXT);
        sealed.extend_from_slice(&RFC_8439_SEC_2_8_2_TAG);
        // Flip one bit in the tag.
        *sealed.last_mut().unwrap() ^= 0x01;

        let params = AeadParamsC {
            key_ptr: key.as_ptr(),
            nonce_ptr: nonce.as_ptr(),
            aad_ptr: aad.as_ptr(),
            aad_len: aad.len(),
        };

        let mut out = vec![0u8; RFC_8439_SEC_2_8_2_PLAINTEXT.len()];
        let mut written: usize = 0;
        let rc = unsafe {
            paideia_crypto_chacha20_poly1305_open(
                &params as *const _,
                sealed.as_ptr(),
                sealed.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written as *mut _,
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_AUTHENTICATION);
    }

    #[test]
    fn ffi_chacha20_poly1305_seal_undersized_output_returns_buffer_too_small() {
        let key = RFC_8439_SEC_2_8_2_KEY;
        let nonce = RFC_8439_SEC_2_8_2_NONCE;
        let params = AeadParamsC {
            key_ptr: key.as_ptr(),
            nonce_ptr: nonce.as_ptr(),
            aad_ptr: core::ptr::null(),
            aad_len: 0,
        };
        let plaintext = [0u8; 16];
        // out_cap < plaintext_len + TAG_LEN (16) should be rejected.
        let mut out = [0u8; 8];
        let mut written: usize = 0;
        let rc = unsafe {
            paideia_crypto_chacha20_poly1305_seal(
                &params as *const _,
                plaintext.as_ptr(),
                plaintext.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written as *mut _,
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_BUFFER_TOO_SMALL);
        assert_eq!(written, 0);
    }

    #[test]
    fn ffi_chacha20_poly1305_open_short_sealed_returns_invalid_length() {
        let key = RFC_8439_SEC_2_8_2_KEY;
        let nonce = RFC_8439_SEC_2_8_2_NONCE;
        let params = AeadParamsC {
            key_ptr: key.as_ptr(),
            nonce_ptr: nonce.as_ptr(),
            aad_ptr: core::ptr::null(),
            aad_len: 0,
        };
        // sealed shorter than TAG_LEN (16) is malformed.
        let sealed = [0u8; 8];
        let mut out = [0u8; 8];
        let mut written: usize = 0;
        let rc = unsafe {
            paideia_crypto_chacha20_poly1305_open(
                &params as *const _,
                sealed.as_ptr(),
                sealed.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written as *mut _,
            )
        };
        assert_eq!(rc, PDX_CRYPTO_ERR_INVALID_LENGTH);
    }
}
